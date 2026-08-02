use crate::Scottatura;

pub const MAGIC: u32 = 0x5052_4550;
pub const CHECKPOINT_THRESHOLD: u64 = 1000;
pub const HEADER_SIZE: usize = 36;
pub const CRC_SIZE: usize = 4;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    Write = 0x01,
    Commit = 0x02,
    Rollback = 0x03,
    Checkpoint = 0x04,
}

impl RecordType {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::Write),
            0x02 => Some(Self::Commit),
            0x03 => Some(Self::Rollback),
            0x04 => Some(Self::Checkpoint),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogRecord {
    pub lsn: u64,
    pub simmer_id: u64,
    pub record_type: RecordType,
    pub page_num: u64,
    pub offset: u16,
    pub after_image: Vec<u8>,
}

impl LogRecord {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_SIZE + self.after_image.len() + CRC_SIZE);
        buf.extend_from_slice(&MAGIC.to_le_bytes());
        buf.extend_from_slice(&self.lsn.to_le_bytes());
        buf.extend_from_slice(&self.simmer_id.to_le_bytes());
        buf.push(self.record_type as u8);
        buf.extend_from_slice(&[0u8; 3]);
        buf.extend_from_slice(&self.page_num.to_le_bytes());
        buf.extend_from_slice(&self.offset.to_le_bytes());
        buf.extend_from_slice(&(self.after_image.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.after_image);
        let crc = crc32fast::hash(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());
        buf
    }

    pub fn decode(buf: &[u8]) -> Result<Option<(LogRecord, usize)>, Scottatura> {
        if buf.len() < HEADER_SIZE {
            return Ok(None);
        }
        let magic = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        if magic != MAGIC {
            return Ok(None);
        }
        let lsn = u64::from_le_bytes(buf[4..12].try_into().unwrap());
        let simmer_id = u64::from_le_bytes(buf[12..20].try_into().unwrap());
        let record_type = match RecordType::from_u8(buf[20]) {
            Some(rt) => rt,
            None => return Ok(None),
        };
        let page_num = u64::from_le_bytes(buf[24..32].try_into().unwrap());
        let offset = u16::from_le_bytes(buf[32..34].try_into().unwrap());
        let length = u16::from_le_bytes(buf[34..36].try_into().unwrap()) as usize;
        let total_len = HEADER_SIZE + length + CRC_SIZE;
        if buf.len() < total_len {
            return Ok(None);
        }
        let computed_crc = crc32fast::hash(&buf[0..HEADER_SIZE + length]);
        let stored_crc = u32::from_le_bytes(buf[HEADER_SIZE + length..total_len].try_into().unwrap());
        if stored_crc != computed_crc {
            return Err(Scottatura::PrepLogCrcMismatch { lsn });
        }
        let after_image = buf[HEADER_SIZE..HEADER_SIZE + length].to_vec();
        Ok(Some((
            LogRecord { lsn, simmer_id, record_type, page_num, offset, after_image },
            total_len,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_write() -> LogRecord {
        LogRecord {
            lsn: 1,
            simmer_id: 0,
            record_type: RecordType::Write,
            page_num: 5,
            offset: 0,
            after_image: vec![0xAA; 100],
        }
    }

    #[test]
    fn test_roundtrip_write_record() {
        let rec = sample_write();
        let encoded = rec.encode();
        let (decoded, consumed) = LogRecord::decode(&encoded).unwrap().unwrap();
        assert_eq!(decoded, rec);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_roundtrip_commit_record() {
        let rec = LogRecord {
            lsn: 2, simmer_id: 7, record_type: RecordType::Commit,
            page_num: 0, offset: 0, after_image: vec![],
        };
        let encoded = rec.encode();
        let (decoded, _) = LogRecord::decode(&encoded).unwrap().unwrap();
        assert_eq!(decoded, rec);
    }

    #[test]
    fn test_roundtrip_rollback_record() {
        let rec = LogRecord {
            lsn: 3, simmer_id: 7, record_type: RecordType::Rollback,
            page_num: 0, offset: 0, after_image: vec![],
        };
        let encoded = rec.encode();
        let (decoded, _) = LogRecord::decode(&encoded).unwrap().unwrap();
        assert_eq!(decoded, rec);
    }

    #[test]
    fn test_roundtrip_checkpoint_record() {
        let rec = LogRecord {
            lsn: 4, simmer_id: 0, record_type: RecordType::Checkpoint,
            page_num: 0, offset: 0, after_image: 4u64.to_le_bytes().to_vec(),
        };
        let encoded = rec.encode();
        let (decoded, _) = LogRecord::decode(&encoded).unwrap().unwrap();
        assert_eq!(decoded, rec);
    }

    #[test]
    fn test_too_short_buffer_returns_none() {
        let buf = [0u8; 10];
        assert_eq!(LogRecord::decode(&buf).unwrap(), None);
    }

    #[test]
    fn test_truncated_payload_returns_none() {
        let rec = sample_write();
        let mut encoded = rec.encode();
        encoded.truncate(encoded.len() - 10);
        assert_eq!(LogRecord::decode(&encoded).unwrap(), None);
    }

    #[test]
    fn test_corrupted_crc_errors() {
        let rec = sample_write();
        let mut encoded = rec.encode();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xFF;
        let result = LogRecord::decode(&encoded);
        assert!(matches!(result, Err(Scottatura::PrepLogCrcMismatch { lsn: 1 })));
    }

    #[test]
    fn test_back_to_back_records_decode_via_consumed_offset() {
        let r1 = sample_write();
        let r2 = LogRecord {
            lsn: 2, simmer_id: 0, record_type: RecordType::Commit,
            page_num: 0, offset: 0, after_image: vec![],
        };
        let mut buf = r1.encode();
        buf.extend(r2.encode());

        let (decoded1, consumed1) = LogRecord::decode(&buf).unwrap().unwrap();
        assert_eq!(decoded1, r1);
        let (decoded2, _) = LogRecord::decode(&buf[consumed1..]).unwrap().unwrap();
        assert_eq!(decoded2, r2);
    }
}
