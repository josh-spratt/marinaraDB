use std::collections::HashSet;
use std::fs;

use crate::prep_log::record::{LogRecord, RecordType};
use crate::stove::page::{Page, PAGE_SIZE};
use crate::stove::pot_file::PotFile;
use crate::Scottatura;

pub fn recover(pot_file: &mut PotFile, prep_path: &str) -> Result<u64, Scottatura> {
    let bytes = match fs::read(prep_path) {
        Ok(b) => b,
        Err(_) => return Ok(1),
    };

    let mut committed: HashSet<u64> = HashSet::new();
    let mut pending: Vec<LogRecord> = Vec::new();
    let mut max_lsn = 0u64;
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        match LogRecord::decode(&bytes[cursor..]) {
            Ok(Some((record, consumed))) => {
                max_lsn = max_lsn.max(record.lsn);
                match record.record_type {
                    RecordType::Checkpoint => {
                        pending.clear();
                        committed.clear();
                    }
                    RecordType::Commit => {
                        committed.insert(record.simmer_id);
                    }
                    RecordType::Write => {
                        pending.push(record);
                    }
                    RecordType::Rollback => {}
                }
                cursor += consumed;
            }
            Ok(None) | Err(_) => break,
        }
    }

    for record in pending {
        if committed.contains(&record.simmer_id) && record.after_image.len() == PAGE_SIZE {
            let mut data = [0u8; PAGE_SIZE];
            data.copy_from_slice(&record.after_image);
            pot_file.write_page(&Page { number: record.page_num, data })?;
        }
    }

    pot_file.sync_all()?;
    Ok(max_lsn + 1)
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

    fn write_bytes(path: &str, records: &[LogRecord]) {
        let mut buf = Vec::new();
        for r in records {
            buf.extend(r.encode());
        }
        std::fs::write(path, buf).unwrap();
    }

    fn page_image(fill: u8) -> Vec<u8> {
        vec![fill; PAGE_SIZE]
    }

    #[test]
    fn test_missing_file_returns_lsn_one() {
        let pot_path = fresh_path();
        let mut pot = PotFile::create(&pot_path).unwrap();
        let next = recover(&mut pot, "/nonexistent/path/does-not-exist.prep").unwrap();
        assert_eq!(next, 1);
        std::fs::remove_file(&pot_path).ok();
    }

    #[test]
    fn test_replays_committed_write() {
        let prep_path = fresh_path();
        let pot_path = fresh_path();
        write_bytes(&prep_path, &[
            LogRecord { lsn: 1, simmer_id: 0, record_type: RecordType::Write, page_num: 3, offset: 0, after_image: page_image(0xAA) },
            LogRecord { lsn: 2, simmer_id: 0, record_type: RecordType::Commit, page_num: 0, offset: 0, after_image: vec![] },
        ]);
        let mut pot = PotFile::create(&pot_path).unwrap();
        let next_lsn = recover(&mut pot, &prep_path).unwrap();
        assert_eq!(next_lsn, 3);
        let page = pot.read_page(3).unwrap();
        assert_eq!(page.data[0], 0xAA);
        std::fs::remove_file(&prep_path).ok();
        std::fs::remove_file(&pot_path).ok();
    }

    #[test]
    fn test_discards_uncommitted_write() {
        let prep_path = fresh_path();
        let pot_path = fresh_path();
        write_bytes(&prep_path, &[
            LogRecord { lsn: 1, simmer_id: 0, record_type: RecordType::Write, page_num: 3, offset: 0, after_image: page_image(0xAA) },
        ]);
        let mut pot = PotFile::create(&pot_path).unwrap();
        pot.allocate_page(crate::stove::page::PageType::Heap).unwrap(); // page 1
        pot.allocate_page(crate::stove::page::PageType::Heap).unwrap(); // page 2
        pot.allocate_page(crate::stove::page::PageType::Heap).unwrap(); // page 3
        recover(&mut pot, &prep_path).unwrap();
        let page = pot.read_page(3).unwrap();
        // byte 0 is the page-type tag set by allocation itself (always 0x01 for
        // Heap), so check a byte past the header to see whether the after-image
        // was replayed over it.
        assert_eq!(page.data[16], 0); // never committed — not replayed
        std::fs::remove_file(&prep_path).ok();
        std::fs::remove_file(&pot_path).ok();
    }

    #[test]
    fn test_stops_at_bad_crc_but_keeps_earlier_valid_records() {
        let prep_path = fresh_path();
        let pot_path = fresh_path();
        let good = LogRecord { lsn: 1, simmer_id: 0, record_type: RecordType::Write, page_num: 3, offset: 0, after_image: page_image(0xAA) };
        let commit = LogRecord { lsn: 2, simmer_id: 0, record_type: RecordType::Commit, page_num: 0, offset: 0, after_image: vec![] };
        let mut buf = good.encode();
        buf.extend(commit.encode());
        let mut corrupted_tail = LogRecord { lsn: 3, simmer_id: 0, record_type: RecordType::Write, page_num: 4, offset: 0, after_image: page_image(0xBB) }.encode();
        let last = corrupted_tail.len() - 1;
        corrupted_tail[last] ^= 0xFF;
        buf.extend(corrupted_tail);
        std::fs::write(&prep_path, &buf).unwrap();

        let mut pot = PotFile::create(&pot_path).unwrap();
        let next_lsn = recover(&mut pot, &prep_path).unwrap();
        assert_eq!(next_lsn, 3); // scan stopped before the corrupted record's LSN
        assert_eq!(pot.read_page(3).unwrap().data[0], 0xAA); // earlier valid record still replayed
        std::fs::remove_file(&prep_path).ok();
        std::fs::remove_file(&pot_path).ok();
    }

    #[test]
    fn test_checkpoint_boundary_prevents_double_replay() {
        let prep_path = fresh_path();
        let pot_path = fresh_path();
        let pre_write = LogRecord { lsn: 1, simmer_id: 0, record_type: RecordType::Write, page_num: 3, offset: 0, after_image: page_image(0xAA) };
        let pre_commit = LogRecord { lsn: 2, simmer_id: 0, record_type: RecordType::Commit, page_num: 0, offset: 0, after_image: vec![] };
        let checkpoint = LogRecord { lsn: 3, simmer_id: 0, record_type: RecordType::Checkpoint, page_num: 0, offset: 0, after_image: 3u64.to_le_bytes().to_vec() };
        let post_write = LogRecord { lsn: 4, simmer_id: 0, record_type: RecordType::Write, page_num: 3, offset: 0, after_image: page_image(0xCC) };
        let post_commit = LogRecord { lsn: 5, simmer_id: 0, record_type: RecordType::Commit, page_num: 0, offset: 0, after_image: vec![] };

        let mut buf = Vec::new();
        for r in [&pre_write, &pre_commit, &checkpoint, &post_write, &post_commit] {
            buf.extend(r.encode());
        }
        std::fs::write(&prep_path, &buf).unwrap();

        let mut pot = PotFile::create(&pot_path).unwrap();
        let next_lsn = recover(&mut pot, &prep_path).unwrap();
        assert_eq!(next_lsn, 6);
        assert_eq!(pot.read_page(3).unwrap().data[0], 0xCC); // only the post-checkpoint image applied
        std::fs::remove_file(&prep_path).ok();
        std::fs::remove_file(&pot_path).ok();
    }
}
