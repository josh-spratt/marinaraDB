pub mod record;

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::io::{Seek, SeekFrom};

use crate::Scottatura;
use crate::stove::pot_file::PotFile;
use crate::warming_tray::WarmingTray;
use record::{LogRecord, RecordType, CHECKPOINT_THRESHOLD};

pub struct PrepLog {
    file: File,
    next_lsn: u64,
    since_checkpoint: u64,
}

impl PrepLog {
    pub fn init(path: &str, start_lsn: u64) -> Result<Self, Scottatura> {
        let file = OpenOptions::new()
            .read(true).write(true).create(true).truncate(true)
            .open(path)
            .map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;
        Ok(Self { file, next_lsn: start_lsn, since_checkpoint: 0 })
    }

    pub fn append_write(&mut self, simmer_id: u64, page_num: u64, offset: u16, after_image: &[u8]) -> Result<u64, Scottatura> {
        let lsn = self.next_lsn;
        let record = LogRecord {
            lsn, simmer_id, record_type: RecordType::Write,
            page_num, offset, after_image: after_image.to_vec(),
        };
        self.write_record(&record)?;
        self.next_lsn += 1;
        self.since_checkpoint += 1;
        Ok(lsn)
    }

    pub fn append_commit(&mut self, simmer_id: u64) -> Result<(), Scottatura> {
        self.file.sync_all().map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;
        let record = LogRecord {
            lsn: self.next_lsn, simmer_id, record_type: RecordType::Commit,
            page_num: 0, offset: 0, after_image: Vec::new(),
        };
        self.write_record(&record)?;
        self.file.sync_all().map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;
        self.next_lsn += 1;
        self.since_checkpoint += 1;
        Ok(())
    }

    pub fn append_rollback(&mut self, simmer_id: u64) -> Result<(), Scottatura> {
        let record = LogRecord {
            lsn: self.next_lsn, simmer_id, record_type: RecordType::Rollback,
            page_num: 0, offset: 0, after_image: Vec::new(),
        };
        self.write_record(&record)?;
        self.next_lsn += 1;
        self.since_checkpoint += 1;
        Ok(())
    }

    pub fn should_checkpoint(&self) -> bool {
        self.since_checkpoint >= CHECKPOINT_THRESHOLD
    }

    pub fn checkpoint(&mut self, tray: &mut WarmingTray, pot_file: &mut PotFile) -> Result<(), Scottatura> {
        let checkpoint_lsn = self.next_lsn;
        let record = LogRecord {
            lsn: checkpoint_lsn, simmer_id: 0, record_type: RecordType::Checkpoint,
            page_num: 0, offset: 0, after_image: checkpoint_lsn.to_le_bytes().to_vec(),
        };
        let encoded = record.encode();
        self.file.write_all(&encoded).map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;
        self.next_lsn += 1;

        tray.flush_all(pot_file)?;
        pot_file.sync_all()?;

        self.file.sync_all().map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;

        self.file.set_len(0).map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;
        self.file.seek(SeekFrom::Start(0)).map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;
        self.file.write_all(&encoded).map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;
        self.file.sync_all().map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;

        self.since_checkpoint = 0;
        Ok(())
    }

    fn write_record(&mut self, record: &LogRecord) -> Result<(), Scottatura> {
        self.file.write_all(&record.encode())
            .map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn fresh_path() -> String {
        let f = NamedTempFile::new().unwrap();
        let p = f.path().to_str().unwrap().to_string();
        drop(f);
        p
    }

    #[test]
    fn test_init_creates_empty_file() {
        let path = fresh_path();
        PrepLog::init(&path, 1).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_write_then_decode_roundtrip() {
        let path = fresh_path();
        let mut log = PrepLog::init(&path, 1).unwrap();
        let lsn = log.append_write(0, 5, 0, &[0xAB; 20]).unwrap();
        assert_eq!(lsn, 1);
        let bytes = std::fs::read(&path).unwrap();
        let (decoded, _) = LogRecord::decode(&bytes).unwrap().unwrap();
        assert_eq!(decoded.page_num, 5);
        assert_eq!(decoded.after_image, vec![0xAB; 20]);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_commit_appends_record_after_write() {
        let path = fresh_path();
        let mut log = PrepLog::init(&path, 1).unwrap();
        log.append_write(0, 5, 0, &[0xAB; 20]).unwrap();
        log.append_commit(0).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let (w, consumed) = LogRecord::decode(&bytes).unwrap().unwrap();
        assert_eq!(w.record_type, RecordType::Write);
        let (c, _) = LogRecord::decode(&bytes[consumed..]).unwrap().unwrap();
        assert_eq!(c.record_type, RecordType::Commit);
        assert_eq!(c.lsn, 2);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_monotonic_lsns_across_appends() {
        let path = fresh_path();
        let mut log = PrepLog::init(&path, 1).unwrap();
        let lsn1 = log.append_write(0, 1, 0, &[]).unwrap();
        log.append_commit(0).unwrap();
        let lsn2 = log.append_write(0, 2, 0, &[]).unwrap();
        assert_eq!(lsn1, 1);
        assert_eq!(lsn2, 3);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_should_checkpoint_flips_at_threshold() {
        let path = fresh_path();
        let mut log = PrepLog::init(&path, 1).unwrap();
        for _ in 0..(CHECKPOINT_THRESHOLD - 1) {
            log.append_rollback(0).unwrap();
        }
        assert!(!log.should_checkpoint());
        log.append_rollback(0).unwrap();
        assert!(log.should_checkpoint());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_checkpoint_record_shape() {
        let path = fresh_path();
        let mut log = PrepLog::init(&path, 1).unwrap();
        log.append_rollback(0).unwrap(); // lsn 1

        let pot_path = fresh_path();
        let mut pot_file = PotFile::create(&pot_path).unwrap();
        let mut tray = WarmingTray::new(16);

        log.checkpoint(&mut tray, &mut pot_file).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let (rec, consumed) = LogRecord::decode(&bytes).unwrap().unwrap();
        assert_eq!(rec.record_type, RecordType::Checkpoint);
        assert_eq!(rec.lsn, 2);
        assert_eq!(consumed, bytes.len());
        assert!(!log.should_checkpoint());

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&pot_path).ok();
    }

    #[test]
    fn test_records_after_checkpoint_retained_earlier_dropped() {
        let path = fresh_path();
        let mut log = PrepLog::init(&path, 1).unwrap();
        log.append_rollback(0).unwrap(); // lsn 1 — dropped by the checkpoint

        let pot_path = fresh_path();
        let mut pot_file = PotFile::create(&pot_path).unwrap();
        let mut tray = WarmingTray::new(16);
        log.checkpoint(&mut tray, &mut pot_file).unwrap(); // lsn 2 — retained

        log.append_rollback(0).unwrap(); // lsn 3 — appended after, retained

        let bytes = std::fs::read(&path).unwrap();
        let (first, consumed) = LogRecord::decode(&bytes).unwrap().unwrap();
        assert_eq!(first.record_type, RecordType::Checkpoint);
        assert_eq!(first.lsn, 2);
        let (second, _) = LogRecord::decode(&bytes[consumed..]).unwrap().unwrap();
        assert_eq!(second.record_type, RecordType::Rollback);
        assert_eq!(second.lsn, 3);

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&pot_path).ok();
    }
}
