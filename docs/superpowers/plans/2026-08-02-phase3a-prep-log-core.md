# Phase 3a: Prep Log Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `prep_log` stub with a real write-ahead log — on-disk
record format, append-only writer, checkpoint-and-truncate, and crash
recovery on `Pot::open` — and wire it through the one write path that exists
today (heap insert/delete) under the hardcoded autocommit Simmer ID `0`, so
recovery is provably exercised end-to-end.

**Architecture:** Six tasks. `record.rs` defines the on-disk byte format
first (pure encode/decode, no I/O). `mod.rs` builds the append-only writer on
top of it. Checkpoint/truncate adds `WarmingTray::flush_all` and
`PotFile::sync_all` as prerequisites, then `PrepLog::checkpoint`. `recovery.rs`
replays a `.prep` file against a raw `PotFile`. The final task wires
`PrepLog` into `Pot::open` (recovery before catalog load) and into
`heap_chain_insert`/`heap_chain_delete` (log-then-flush).

**Tech Stack:** Rust 2021 edition (existing crate: `bincode 1`, `thiserror 1`,
`crc32fast 1`, `serde 1`, `tempfile 3` dev-dep) — no new dependencies.

**Out of scope (Phase 3b — Simmer transactions):** `BEGIN`/`COMMIT`/`ROLLBACK`
SQL statements, locking, Simmer lifecycle, real (non-zero) Simmer IDs. The
`PrepLog` API built here (`append_write`/`append_commit`/`append_rollback`/
`checkpoint`) is designed to be consumed unchanged by Phase 3b.

## Global Constraints

- Page size: exactly 4096 bytes (`crate::stove::page::PAGE_SIZE`) — every
  `Write` record in this phase logs a full page image, per the locked design
  decision in the spec.
- All multi-byte integers are little-endian on disk, matching every other
  on-disk format in this codebase.
- All public APIs return `Result<T, Scottatura>`.
- No mocking of storage or WAL I/O — tests that touch disk use `tempfile`.
- `cargo test` and `cargo clippy --all-targets -- -D warnings` must be clean
  after every task, before committing.
- Record layout, write ordering, checkpoint procedure, and recovery algorithm
  all come from `docs/wal-format.md` — do not deviate from it without
  updating that doc first.

---

## File Map

| File | Responsibility |
|---|---|
| `src/lib.rs` | add `PrepLogCrcMismatch`, `PrepLogIoError` Scottatura variants |
| `src/prep_log/record.rs` | new — `RecordType`, `LogRecord::encode`/`decode` |
| `src/prep_log/mod.rs` | replace stub — `PrepLog`: writer, checkpoint |
| `src/prep_log/recovery.rs` | new — `recover(pot_file, prep_path) -> Result<u64, Scottatura>` |
| `src/warming_tray/mod.rs` | add `flush_all` |
| `src/stove/pot_file.rs` | add `sync_all` |
| `src/stove/pot.rs` | add `prep_log` field, recovery-on-open, log-then-flush heap writes |
| `tests/prep_log_recovery.rs` | new — integration tests through `Pot`'s public API |
| `CHANGELOG.md` | record the addition under `[Unreleased]/Added` |

---

## Task 1: Scottatura error variants

**Files:**
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: nothing new
- Produces:
  - `Scottatura::PrepLogCrcMismatch { lsn: u64 }` (`STOVE_003`)
  - `Scottatura::PrepLogIoError { message: String }` (`STOVE_005`)

- [ ] **Step 1: Add the two variants to the `Scottatura` enum**

In `src/lib.rs`, add these variants inside `pub enum Scottatura { ... }` (placement doesn't matter — thiserror enums aren't order-sensitive):

```rust
    #[error("Scottatura [STOVE_003]: Prep Log CRC32 mismatch at LSN {lsn} — truncating log at this point.")]
    PrepLogCrcMismatch { lsn: u64 },

    #[error("Scottatura [STOVE_005]: I/O error on prep log: {message}")]
    PrepLogIoError { message: String },
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles cleanly (new variants are unused so far — no warnings for unused enum variants in Rust).

- [ ] **Step 3: Commit**

```bash
git add src/lib.rs
git commit -m "feat: add PrepLogCrcMismatch (STOVE_003) and PrepLogIoError (STOVE_005) error variants"
```

---

## Task 2: On-disk record format

**Files:**
- Create: `src/prep_log/record.rs`
- Modify: `src/prep_log/mod.rs` (add `pub mod record;`, remove stub comment)

**Interfaces:**
- Consumes: `Scottatura` from `lib.rs`
- Produces:
  - `pub const MAGIC: u32 = 0x5052_4550`
  - `pub const CHECKPOINT_THRESHOLD: u64 = 1000`
  - `pub const HEADER_SIZE: usize = 36`
  - `pub const CRC_SIZE: usize = 4`
  - `pub enum RecordType { Write = 0x01, Commit = 0x02, Rollback = 0x03, Checkpoint = 0x04 }` with `RecordType::from_u8(u8) -> Option<Self>`
  - `pub struct LogRecord { pub lsn: u64, pub simmer_id: u64, pub record_type: RecordType, pub page_num: u64, pub offset: u16, pub after_image: Vec<u8> }`
  - `LogRecord::encode(&self) -> Vec<u8>`
  - `LogRecord::decode(buf: &[u8]) -> Result<Option<(LogRecord, usize)>, Scottatura>` — the `usize` is bytes consumed. `Ok(None)` for a buffer too short to hold a complete record (crash-boundary truncation). `Err(Scottatura::PrepLogCrcMismatch { lsn })` for a complete-length record whose CRC32 doesn't match.

Record layout (from `docs/wal-format.md`):

```
Offset  Size  Field
0       4     Magic: 0x50524550 ("PREP")
4       8     LSN (u64 LE)
12      8     Simmer ID (u64 LE)
20      1     Record type (u8)
21      3     Reserved [u8; 3] — zero-filled
24      8     Page number (u64 LE)
32      2     Offset in page (u16 LE)
34      2     After-image length (u16 LE)
36      N     After-image
36+N    4     CRC32 of bytes 0..36+N (u32 LE)
```

- [ ] **Step 1: Create `src/prep_log/record.rs` with the type definitions and failing tests**

```rust
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
        todo!()
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
        todo!()
    }

    pub fn decode(buf: &[u8]) -> Result<Option<(LogRecord, usize)>, Scottatura> {
        todo!()
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
```

Note: `assert_eq!(LogRecord::decode(&buf).unwrap(), None)` requires `Option<(LogRecord, usize)>: PartialEq`, which follows from `LogRecord: PartialEq` (already derived) and `usize: PartialEq` — no extra derive needed.

- [ ] **Step 2: Add the module to `src/prep_log/mod.rs`**

Replace the entire contents of `src/prep_log/mod.rs` (currently just `// Write-ahead log — Phase 4`) with:

```rust
pub mod record;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test prep_log::record`
Expected: FAIL with "not yet implemented" panics from the `todo!()` bodies.

- [ ] **Step 4: Implement `RecordType::from_u8`, `LogRecord::encode`, `LogRecord::decode`**

Replace the `todo!()` bodies in `src/prep_log/record.rs`:

```rust
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test prep_log::record`
Expected: All 8 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/prep_log/record.rs src/prep_log/mod.rs
git commit -m "feat: Prep Log on-disk record format — LogRecord encode/decode with CRC32"
```

---

## Task 3: Append-only log writer

**Files:**
- Modify: `src/prep_log/mod.rs`

**Interfaces:**
- Consumes: `LogRecord`, `RecordType`, `CHECKPOINT_THRESHOLD` from `prep_log::record`; `Scottatura` from `lib`
- Produces:
  - `pub struct PrepLog`
  - `PrepLog::init(path: &str, start_lsn: u64) -> Result<Self, Scottatura>` — always creates/truncates a fresh, empty `.prep` file
  - `PrepLog::append_write(&mut self, simmer_id: u64, page_num: u64, offset: u16, after_image: &[u8]) -> Result<u64, Scottatura>` — returns the LSN used
  - `PrepLog::append_commit(&mut self, simmer_id: u64) -> Result<(), Scottatura>`
  - `PrepLog::append_rollback(&mut self, simmer_id: u64) -> Result<(), Scottatura>`
  - `PrepLog::should_checkpoint(&self) -> bool`

Durability ordering (from `docs/wal-format.md`): `append_write` is buffered
(no forced fsync). `append_commit` fsyncs the buffered writes first, then
writes and fsyncs the commit record itself. `append_rollback` is buffered,
no forced fsync (nothing needs to survive a crash for an aborted write).

- [ ] **Step 1: Add `PrepLog` with `todo!()` bodies and failing tests to `src/prep_log/mod.rs`**

Append this to `src/prep_log/mod.rs` (below the existing `pub mod record;`):

```rust
use std::fs::{File, OpenOptions};
use std::io::Write as _;

use crate::Scottatura;
use record::{LogRecord, RecordType, CHECKPOINT_THRESHOLD};

pub struct PrepLog {
    file: File,
    next_lsn: u64,
    since_checkpoint: u64,
}

impl PrepLog {
    pub fn init(path: &str, start_lsn: u64) -> Result<Self, Scottatura> {
        todo!()
    }

    pub fn append_write(&mut self, simmer_id: u64, page_num: u64, offset: u16, after_image: &[u8]) -> Result<u64, Scottatura> {
        todo!()
    }

    pub fn append_commit(&mut self, simmer_id: u64) -> Result<(), Scottatura> {
        todo!()
    }

    pub fn append_rollback(&mut self, simmer_id: u64) -> Result<(), Scottatura> {
        todo!()
    }

    pub fn should_checkpoint(&self) -> bool {
        todo!()
    }

    fn write_record(&mut self, record: &LogRecord) -> Result<(), Scottatura> {
        todo!()
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
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test prep_log::tests`
Expected: FAIL with todo panics.

- [ ] **Step 3: Implement all `PrepLog` methods**

Replace the `todo!()` bodies:

```rust
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

    fn write_record(&mut self, record: &LogRecord) -> Result<(), Scottatura> {
        self.file.write_all(&record.encode())
            .map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test prep_log`
Expected: All tests pass (8 from Task 2 + 5 new).

- [ ] **Step 5: Commit**

```bash
git add src/prep_log/mod.rs
git commit -m "feat: PrepLog append-only writer with fsync-ordered commit durability"
```

---

## Task 4: Checkpoint + truncate

**Files:**
- Modify: `src/warming_tray/mod.rs` (add `flush_all`)
- Modify: `src/stove/pot_file.rs` (add `sync_all`)
- Modify: `src/prep_log/mod.rs` (add `PrepLog::checkpoint`)

**Interfaces:**
- Consumes: `WarmingTray` from `warming_tray`; `PotFile` from `stove::pot_file`; `LogRecord`, `RecordType` from `prep_log::record`
- Produces:
  - `WarmingTray::flush_all(&mut self, file: &mut PotFile) -> Result<(), Scottatura>` — flushes every dirty page regardless of owning Simmer ID
  - `PotFile::sync_all(&self) -> Result<(), Scottatura>`
  - `PrepLog::checkpoint(&mut self, tray: &mut WarmingTray, pot_file: &mut PotFile) -> Result<(), Scottatura>`

- [ ] **Step 1: Add `flush_all` with a failing test to `src/warming_tray/mod.rs`**

Add this method to `impl WarmingTray` (alongside `flush_simmer`):

```rust
    pub fn flush_all(&mut self, file: &mut PotFile) -> Result<(), Scottatura> {
        for entry in self.cache.values_mut() {
            if entry.dirty {
                file.write_page(&entry.page)?;
                entry.dirty = false;
                entry.simmer_id = None;
            }
        }
        Ok(())
    }
```

Add this test to the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn test_flush_all_flushes_multiple_simmers() {
        let (mut pot, path) = make_pot_with_page(); // page 1
        pot.allocate_page(PageType::Heap).unwrap(); // page 2
        let mut tray = WarmingTray::new(16);
        tray.fetch(1, &mut pot).unwrap().set_next_page(11);
        tray.mark_dirty(1, 1);
        tray.fetch(2, &mut pot).unwrap().set_next_page(22);
        tray.mark_dirty(2, 2);

        tray.flush_all(&mut pot).unwrap();

        assert_eq!(pot.read_page(1).unwrap().next_page(), 11);
        assert_eq!(pot.read_page(2).unwrap().next_page(), 22);
        std::fs::remove_file(&path).ok();
    }
```

- [ ] **Step 2: Run the new test**

Run: `cargo test warming_tray::tests::test_flush_all_flushes_multiple_simmers`
Expected: PASS (this method has no `todo!()` — it's implemented directly since it's a near-identical variant of the existing `flush_simmer`).

- [ ] **Step 3: Add `sync_all` with a test to `src/stove/pot_file.rs`**

Add this method to `impl PotFile` (alongside `write_page`):

```rust
    pub fn sync_all(&self) -> Result<(), Scottatura> {
        self.file.sync_all()?;
        Ok(())
    }
```

Add this test to the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn test_sync_all_succeeds() {
        let path = fresh_path();
        let pot = PotFile::create(&path).unwrap();
        pot.sync_all().unwrap();
        std::fs::remove_file(&path).ok();
    }
```

- [ ] **Step 4: Run the new test**

Run: `cargo test stove::pot_file::tests::test_sync_all_succeeds`
Expected: PASS.

- [ ] **Step 5: Add `PrepLog::checkpoint` with `todo!()` and failing tests to `src/prep_log/mod.rs`**

Add these imports near the top of `src/prep_log/mod.rs`:

```rust
use std::io::{Seek, SeekFrom};

use crate::stove::pot_file::PotFile;
use crate::warming_tray::WarmingTray;
```

Add this method to `impl PrepLog`:

```rust
    pub fn checkpoint(&mut self, tray: &mut WarmingTray, pot_file: &mut PotFile) -> Result<(), Scottatura> {
        todo!()
    }
```

Add these tests to the `#[cfg(test)] mod tests` block:

```rust
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
```

- [ ] **Step 6: Run tests to verify the new ones fail**

Run: `cargo test prep_log::tests::test_checkpoint`
Expected: FAIL with todo panic.

- [ ] **Step 7: Implement `checkpoint`**

```rust
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

        self.file.set_len(0).map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;
        self.file.seek(SeekFrom::Start(0)).map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;
        self.file.write_all(&encoded).map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;
        self.file.sync_all().map_err(|e| Scottatura::PrepLogIoError { message: e.to_string() })?;

        self.since_checkpoint = 0;
        Ok(())
    }
```

- [ ] **Step 8: Run all prep_log tests to verify they pass**

Run: `cargo test prep_log`
Expected: All pass.

- [ ] **Step 9: Run full suite and clippy**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: Clean.

- [ ] **Step 10: Commit**

```bash
git add src/warming_tray/mod.rs src/stove/pot_file.rs src/prep_log/mod.rs
git commit -m "feat: checkpoint-and-truncate — WarmingTray::flush_all, PotFile::sync_all, PrepLog::checkpoint"
```

---

## Task 5: Crash recovery scan

**Files:**
- Create: `src/prep_log/recovery.rs`
- Modify: `src/prep_log/mod.rs` (add `pub mod recovery;`)

**Interfaces:**
- Consumes: `LogRecord`, `RecordType` from `prep_log::record`; `Page`, `PAGE_SIZE` from `stove::page`; `PotFile` from `stove::pot_file`; `Scottatura` from `lib`
- Produces:
  - `pub fn recover(pot_file: &mut PotFile, prep_path: &str) -> Result<u64, Scottatura>` — returns the next LSN to resume numbering from; `Ok(1)` if `prep_path` doesn't exist.

Algorithm (from `docs/wal-format.md`): scan forward from byte 0. A
`Checkpoint` record clears any pending/committed state accumulated so far
(everything before it is already durable in `.pot`, by construction — a
checkpoint only completes after `flush_all` + `sync_all`). A `Commit` record
adds its Simmer ID to a committed set. A `Write` record is buffered as
pending. A `Rollback` record is a no-op. The first `Ok(None)` or `Err` from
`LogRecord::decode` stops the scan — a corrupt or truncated tail is silently
discarded, not fatal. After the scan, every pending write whose Simmer ID
ended up in the committed set is replayed into `.pot`, in the order
encountered.

- [ ] **Step 1: Create `src/prep_log/recovery.rs` with a failing test suite**

```rust
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
        assert_eq!(page.data[0], 0); // never committed — not replayed
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
```

This task's implementation is written directly (not `todo!()`-stubbed) since
`recover`'s logic is the whole point of the task and there's no meaningful
smaller increment to TDD it in two passes — the test suite above is the
correctness spec for it.

- [ ] **Step 2: Add the module to `src/prep_log/mod.rs`**

Add `pub mod recovery;` below the existing `pub mod record;`.

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test prep_log::recovery`
Expected: All 5 tests pass.

- [ ] **Step 4: Run full suite and clippy**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: Clean.

- [ ] **Step 5: Commit**

```bash
git add src/prep_log/recovery.rs src/prep_log/mod.rs
git commit -m "feat: Prep Log crash recovery — replay committed writes, discard uncommitted/corrupt tail"
```

---

## Task 6: Wire into `Pot` — recovery on open, autocommit writes through the log

**Files:**
- Modify: `src/stove/pot.rs`
- Create: `tests/prep_log_recovery.rs`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: `PrepLog` from `prep_log`; `recovery::recover` from `prep_log::recovery`
- Produces: `Pot` now recovers from a prior crash on `Pot::open` and durably
  logs every heap insert/delete before it reaches the `.pot` file.

- [ ] **Step 1: Add the `prep_log` field, `prep_log_path` helper, and rework `Pot::open`**

In `src/stove/pot.rs`, add to the imports:

```rust
use crate::prep_log::{recovery, PrepLog};
```

Add the `prep_log` field to the `Pot` struct:

```rust
pub struct Pot {
    pot_file: PotFile,
    tray: WarmingTray,
    catalog: Catalog,
    next_key: i64,
    prep_log: PrepLog,
}
```

Add this free function near the top of the file (below `encode_and_validate`):

```rust
fn prep_log_path(pot_path: &str) -> String {
    match pot_path.strip_suffix(".pot") {
        Some(stripped) => format!("{stripped}.prep"),
        None => format!("{pot_path}.prep"),
    }
}
```

Replace `Pot::open` with:

```rust
    pub fn open(path: &str) -> Result<Self, Scottatura> {
        let pre_existing = std::path::Path::new(path).exists();
        let prep_path = prep_log_path(path);

        // Recovery must run against a cache-free PotFile, before the
        // Catalog (or anything else) populates the WarmingTray — otherwise
        // a replayed page write would be shadowed by a stale cache entry.
        let (mut pot_file, next_lsn) = if pre_existing {
            let mut f = PotFile::open(path)?;
            let next_lsn = recovery::recover(&mut f, &prep_path)?;
            (f, next_lsn)
        } else {
            (PotFile::create(path)?, 1)
        };

        let mut tray = WarmingTray::new(256);
        let catalog = if pre_existing {
            Catalog::load(&mut tray, &mut pot_file)?
        } else {
            Catalog::init(&mut tray, &mut pot_file)?
        };

        let prep_log = PrepLog::init(&prep_path, next_lsn)?;

        Ok(Self { pot_file, tray, catalog, next_key: 1, prep_log })
    }
```

- [ ] **Step 2: Run existing tests to confirm nothing broke yet**

Run: `cargo test stove::pot`
Expected: PASS (existing `pot.rs` tests don't yet exercise the Prep Log wiring beyond construction).

- [ ] **Step 3: Route `heap_chain_insert`/`heap_chain_delete` through the log**

Replace `heap_chain_insert` and `heap_chain_delete`, and add `log_and_flush`:

```rust
    fn heap_chain_insert(&mut self, root_page: u64, row_bytes: &[u8]) -> Result<(), Scottatura> {
        let mut page_num = root_page;
        loop {
            let inserted_image = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                if heap_insert(page, row_bytes).is_some() {
                    Some(page.data.to_vec())
                } else {
                    None
                }
            };
            if let Some(after_image) = inserted_image {
                self.log_and_flush(page_num, after_image)?;
                return Ok(());
            }

            let next_page_num = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                page.next_page()
            };
            if next_page_num != 0 {
                page_num = next_page_num;
                continue;
            }

            let new_page = self.pot_file.allocate_page(PageType::Heap)?;
            let new_page_num = new_page.number;
            let linked_image = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                page.set_next_page(new_page_num);
                page.data.to_vec()
            };
            self.log_and_flush(page_num, linked_image)?;
            page_num = new_page_num;
        }
    }

    fn heap_chain_scan(&mut self, root_page: u64, cols: &[ColumnDef]) -> Result<Vec<(i64, Vec<Value>)>, Scottatura> {
        let mut rows = Vec::new();
        let mut page_num = root_page;
        loop {
            let (page_rows, next) = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                (heap_scan(page, cols), page.next_page())
            };
            rows.extend(page_rows);
            if next == 0 { break; }
            page_num = next;
        }
        Ok(rows)
    }

    fn heap_chain_delete(&mut self, root_page: u64, key: i64, cols: &[ColumnDef]) -> Result<bool, Scottatura> {
        let mut page_num = root_page;
        loop {
            let (deleted_image, next) = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                let deleted = heap_delete(page, key, cols);
                let image = if deleted { Some(page.data.to_vec()) } else { None };
                (image, page.next_page())
            };
            if let Some(after_image) = deleted_image {
                self.log_and_flush(page_num, after_image)?;
                return Ok(true);
            }
            if next == 0 { return Ok(false); }
            page_num = next;
        }
    }

    fn log_and_flush(&mut self, page_num: u64, after_image: Vec<u8>) -> Result<(), Scottatura> {
        self.prep_log.append_write(0, page_num, 0, &after_image)?;
        self.prep_log.append_commit(0)?;
        self.tray.mark_dirty(page_num, 0);
        self.tray.flush_simmer(0, &mut self.pot_file)?;
        if self.prep_log.should_checkpoint() {
            self.prep_log.checkpoint(&mut self.tray, &mut self.pot_file)?;
        }
        Ok(())
    }
```

`heap_chain_scan` is included above unchanged (reads never touch the log) —
copy it in as-is if your editor's diff view doesn't show it; it wasn't
modified, only reprinted here for continuity between the two functions that
were changed around it.

- [ ] **Step 4: Run existing tests**

Run: `cargo test`
Expected: All existing tests still pass — `test_insert_beyond_one_page_spans_multiple_pages` and `test_delete_and_reinsert_across_chain` in `pot.rs`, plus `tests/executor.rs` and `tests/foundation.rs`.

- [ ] **Step 5: Create `tests/prep_log_recovery.rs`**

```rust
use marinaradb::stove::page::{Page, PageType};
use marinaradb::stove::pot_file::PotFile;
use marinaradb::{ColType, ColumnDef, Pot, Value};

fn temp_path() -> String {
    let f = tempfile::NamedTempFile::new().unwrap();
    let p = f.path().to_str().unwrap().to_string();
    drop(f);
    p
}

fn nums_col() -> Vec<ColumnDef> {
    vec![ColumnDef { name: "n".into(), col_type: ColType::Integer, nullable: false, strainer_root: None }]
}

#[test]
fn test_committed_write_survives_simulated_pot_write_loss() {
    let path = temp_path();
    let prep_path = format!("{path}.prep");
    {
        let mut pot = Pot::open(&path).unwrap();
        pot.create_table("nums", nums_col()).unwrap();
        pot.insert_strand("nums", vec![Value::Integer(42)]).unwrap();
        // A fresh single-table database always gets catalog root = page 1,
        // heap root = page 2 (PotFile::create leaves page_count at 1 for the
        // header page; Catalog::init claims page 1, create_table claims page 2).
    }

    // Simulate a crash where the .pot write was lost but the Prep Log's
    // commit record made it to disk: blank out the heap page directly.
    {
        let mut raw = PotFile::open(&path).unwrap();
        raw.write_page(&Page::new(2, PageType::Heap)).unwrap();
    }

    let mut pot = Pot::open(&path).unwrap();
    let rows = pot.scan_noodle("nums").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1[0], Value::Integer(42));

    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&prep_path).ok();
}

#[test]
fn test_torn_tail_discarded_without_losing_prior_committed_write() {
    let path = temp_path();
    let prep_path = format!("{path}.prep");
    {
        let mut pot = Pot::open(&path).unwrap();
        pot.create_table("nums", nums_col()).unwrap();
        pot.insert_strand("nums", vec![Value::Integer(7)]).unwrap();
    }

    // Simulate the .pot write being lost...
    {
        let mut raw = PotFile::open(&path).unwrap();
        raw.write_page(&Page::new(2, PageType::Heap)).unwrap();
    }

    // ...and a torn/partial record dangling at the end of the prep log, as
    // if a crash interrupted mid-write of the next statement's record.
    {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new().append(true).open(&prep_path).unwrap();
        file.write_all(&[0xAB; 20]).unwrap(); // shorter than the 36-byte header
    }

    let mut pot = Pot::open(&path).unwrap();
    let rows = pot.scan_noodle("nums").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1[0], Value::Integer(7));

    // Prove the reopened Pot is still fully writable after a torn-tail recovery.
    pot.insert_strand("nums", vec![Value::Integer(8)]).unwrap();
    let rows = pot.scan_noodle("nums").unwrap();
    assert_eq!(rows.len(), 2);

    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&prep_path).ok();
}

#[test]
fn test_checkpoint_shrinks_prep_log_file() {
    let path = temp_path();
    let prep_path = format!("{path}.prep");
    let mut pot = Pot::open(&path).unwrap();
    pot.create_table("nums", nums_col()).unwrap();

    for i in 0..520 {
        pot.insert_strand("nums", vec![Value::Integer(i)]).unwrap();
    }

    let size_after = std::fs::metadata(&prep_path).unwrap().len();
    // Each insert logs one full 4096-byte page image plus a commit record
    // (~4176 bytes/insert). 520 inserts uncheckpointed would be ~2.1MB; a
    // checkpoint fires automatically at record 1000 (insert 500), truncating
    // the log back to just the checkpoint record plus the ~20 records from
    // inserts 501-520 (~84KB) — well under a tenth of the pre-checkpoint size.
    assert!(size_after < 500_000, "expected checkpoint to shrink the log, got {size_after} bytes");

    drop(pot);
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&prep_path).ok();
}
```

- [ ] **Step 6: Run the new integration tests**

Run: `cargo test --test prep_log_recovery`
Expected: All 3 tests pass.

- [ ] **Step 7: Run full suite and clippy**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: Clean.

- [ ] **Step 8: Update `CHANGELOG.md`**

Add these lines under `## [Unreleased]` / `### Added` (after the existing
`BUNDLED BY`/aggregate entries, before the "Scope note" lines already there):

```markdown
- Prep Log (write-ahead log) — on-disk record format (`src/prep_log/record.rs`), append-only writer with fsync-ordered commit durability, checkpoint-and-truncate, and crash recovery run on every `Pot::open`; heap insert/delete now write through the log under the autocommit Simmer ID `0` before touching the `.pot` file
- `STOVE_003` (Prep Log CRC32 mismatch), `STOVE_005` (I/O error on prep log) Scottatura error codes
- Scope note: the `.prep` file is fully reinitialized on every `Pot::open` after recovery replay completes. Safe today because autocommit statements under Simmer ID `0` never overlap within a session; revisit once concurrent/long-lived Simmers exist (Simmer transactions — a later phase).
```

- [ ] **Step 9: Commit**

```bash
git add src/stove/pot.rs tests/prep_log_recovery.rs CHANGELOG.md
git commit -m "feat: wire Prep Log into Pot — crash recovery on open, autocommit heap writes through the log"
```

---

## Self-Review Notes

- **Spec coverage:** every bullet in `docs/wal-format.md` (record layout,
  write ordering, checkpointing, crash recovery, LSN semantics) has a
  corresponding task and test. `docs/error-taxonomy.md`'s `STOVE_003`/
  `STOVE_005` are implemented in Task 1.
- **Type consistency:** `PrepLog::append_write` returns the LSN it used
  (`u64`) in Task 3, matched by `recovery::recover`'s return type
  (`Result<u64, Scottatura>`, "next LSN to resume from") in Task 5 and
  consumed identically in Task 6's `Pot::open`. `LogRecord`'s field names
  (`lsn`, `simmer_id`, `record_type`, `page_num`, `offset`, `after_image`)
  are used identically across `record.rs`, `mod.rs`, and `recovery.rs`.
- **No placeholders:** every step shows real code; the only `todo!()` bodies
  are the intentional Red step of each TDD cycle, immediately followed by
  the Green implementation step in the same task.
