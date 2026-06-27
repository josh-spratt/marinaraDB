# MarinaraDB Phase 1: Storage Foundation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the storage layer that all other MarinaraDB components build on: Value types, page I/O, LRU page cache, heap row operations, catalog (noodle definitions), and a public Rust API to create tables, insert rows, and scan them back.

**Architecture:** Eight tasks, each adding one focused layer. Each layer is independently unit-tested before the next is written. By the end, `cargo test` passes and `Pot::open / create_table / insert_strand / scan_noodle` work against a real `.pot` file on disk.

**Tech Stack:** Rust 2021 edition, `bincode 1`, `thiserror 1`, `crc32fast 1`, `serde 1` (derive), `tempfile 3` (dev)

**This is Plan 1 of 4.** Plans 2–4 (SQL Engine, Indexing, Transactions + WAL) each get their own plan file when reached.

## Global Constraints

- Rust edition 2021; no unsafe except where `memmap2` requires it (not used until Phase 4)
- Page size: exactly 4096 bytes — hardcoded constant, never configurable
- All multi-byte integers are little-endian on disk
- Types: `INTEGER` (`i64`), `REAL` (`f64`), `TEXT` (`String`, UTF-8), `BLOB` (`Vec<u8>`), NULL
- Inline overflow threshold: 512 bytes for TEXT/BLOB (overflow pages: Phase 2)
- All public APIs return `Result<T, Scottatura>`
- Identifier normalization: lowercase at creation, max 128 chars
- No mocking of storage I/O — tests that touch disk use `tempfile`
- Al Dente Key auto-increment starts at 1, increments by 1, gaps never backfilled
- `serde` derives on `ColumnDef`, `ColType`, `NoodleEntry` for `bincode` serialization

---

## File Map

| File | Responsibility |
|---|---|
| `Cargo.toml` | dependencies |
| `src/lib.rs` | `Value`, `ColType`, `ColumnDef`, `Recipe`, `ResultSet`, `Scottatura`, module declarations, `pub use stove::pot::Pot` |
| `src/main.rs` | CLI entry point stub |
| `src/stove/mod.rs` | re-exports sub-modules |
| `src/stove/encoding.rs` | `encode_value`, `decode_value`, `is_fixed_width`, `encoded_fixed_size` |
| `src/stove/page.rs` | `Page`, `PageType`, page header accessors, slot directory accessors |
| `src/stove/pot_file.rs` | `PotFile`: raw file I/O, page read/write, page allocation, file header |
| `src/stove/heap.rs` | `encode_row`, `decode_row`, `heap_insert`, `heap_scan`, `heap_delete` |
| `src/stove/catalog.rs` | `Catalog`, `NoodleEntry`: create/drop/persist noodle definitions |
| `src/stove/pot.rs` | `Pot`: public API wiring everything together |
| `src/warming_tray/mod.rs` | `WarmingTray`: LRU page cache, dirty tracking, simmer-owned eviction |
| `src/strainer/mod.rs` | stub |
| `src/prep_log/mod.rs` | stub |
| `src/parser/mod.rs` | stub |
| `src/sous_chef/mod.rs` | stub |
| `tests/foundation.rs` | integration tests for `Pot` public API |

---

## Task 1: Project Scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: `src/stove/mod.rs`
- Create: `src/warming_tray/mod.rs`
- Create: `src/strainer/mod.rs`
- Create: `src/prep_log/mod.rs`
- Create: `src/parser/mod.rs`
- Create: `src/sous_chef/mod.rs`

**Interfaces:**
- Produces: compilable Rust project with `Value`, `ColType`, `ColumnDef`, `Recipe`, `ResultSet`, `Scottatura` types and empty module stubs

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "marinaradb"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "marinara"
path = "src/main.rs"

[lib]
name = "marinaradb"
path = "src/lib.rs"

[dependencies]
sqlparser = "0.54"
clap = { version = "4", features = ["derive"] }
rustyline = "14"
serde = { version = "1", features = ["derive"] }
bincode = "1"
thiserror = "1"
anyhow = "1"
crc32fast = "1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Create src/main.rs**

```rust
fn main() {
    println!("MarinaraDB v0.1.0 — Buon appetito!");
}
```

- [ ] **Step 3: Create src/lib.rs**

```rust
pub mod parser;
pub mod prep_log;
pub mod sous_chef;
pub mod stove;
pub mod strainer;
pub mod warming_tray;

pub use stove::pot::Pot;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
    Null,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ColType {
    Integer,
    Real,
    Text,
    Blob,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub col_type: ColType,
    pub nullable: bool,
    pub strainer_root: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Recipe {
    pub noodle_name: String,
    pub columns: Vec<ColumnDef>,
}

#[derive(Debug, Default)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub affected_rows: u64,
    pub last_insert_key: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum Scottatura {
    #[error("Scottatura [STOVE_002]: Magic number invalid — `{path}` is not a pot file.")]
    NotAPotFile { path: String },

    #[error("Scottatura [STOVE_001]: Page checksum mismatch on plate {page} — pot may be corrupt.")]
    PageChecksumMismatch { page: u64 },

    #[error("Scottatura [STOVE_004]: I/O error on pot file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Scottatura [RICETTA_001]: Noodle `{name}` does not exist.")]
    NoodleNotFound { name: String },

    #[error("Scottatura [RICETTA_002]: Noodle `{name}` already exists.")]
    NoodleAlreadyExists { name: String },

    #[error("Scottatura [RICETTA_003]: Ingredient `{col}` not found in noodle `{name}`.")]
    ColumnNotFound { col: String, name: String },

    #[error("Scottatura [TIPO_001]: Cannot store {got} value in ingredient `{col}` (expected {expected}).")]
    TypeMismatch { got: String, col: String, expected: String },

    #[error("Scottatura [VINCOLO_002]: Vuoto not allowed in ingredient `{col}` (declared NOT NULL).")]
    NullConstraintViolation { col: String },
}
```

- [ ] **Step 4: Create stub module files**

`src/stove/mod.rs`:
```rust
pub mod pot;
```

`src/warming_tray/mod.rs`:
```rust
// Page cache (buffer pool) — implemented in Task 5
```

`src/strainer/mod.rs`:
```rust
// B-tree index — Phase 3
```

`src/prep_log/mod.rs`:
```rust
// Write-ahead log — Phase 4
```

`src/parser/mod.rs`:
```rust
// sqlparser-rs wrapper — Phase 2
```

`src/sous_chef/mod.rs`:
```rust
// Query planner — Phase 2
```

`src/stove/pot.rs`:
```rust
// Public Pot API — implemented in Task 8
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compiles. Warnings about dead code are fine; errors are not.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/
git commit -m "feat: project scaffolding — module stubs, Value/ColType/Scottatura types"
```

---

## Task 2: Value Encoding

**Files:**
- Create: `src/stove/encoding.rs`
- Modify: `src/stove/mod.rs`

**Interfaces:**
- Consumes: `Value`, `ColType`, `Scottatura` from `lib.rs`
- Produces:
  - `pub fn encode_value(val: &Value, col_type: &ColType) -> Result<Vec<u8>, Scottatura>`
    - `INTEGER` → 8-byte LE i64
    - `REAL` → 8-byte IEEE 754 LE f64
    - `TEXT` → 4-byte LE u32 length + UTF-8 bytes
    - `BLOB` → 4-byte LE u32 length + raw bytes
    - `Null` → 8 zero bytes (fixed-width) or `0xFFFFFFFF` length sentinel (variable)
    - `INTEGER` value into `REAL` column → promoted (only implicit coercion)
    - Type mismatch → `Scottatura::TypeMismatch`
  - `pub fn decode_value(bytes: &[u8], col_type: &ColType, is_null: bool) -> Value`
    - If `is_null`, returns `Value::Null` without reading bytes
  - `pub fn is_fixed_width(col_type: &ColType) -> bool` — true for INTEGER, REAL
  - `pub fn encoded_fixed_size(col_type: &ColType) -> usize` — 8 for INTEGER/REAL, 0 for TEXT/BLOB

- [ ] **Step 1: Create src/stove/encoding.rs with failing tests**

```rust
use crate::{ColType, Scottatura, Value};

pub fn is_fixed_width(col_type: &ColType) -> bool {
    todo!()
}

pub fn encoded_fixed_size(col_type: &ColType) -> usize {
    todo!()
}

pub fn encode_value(val: &Value, col_type: &ColType) -> Result<Vec<u8>, Scottatura> {
    todo!()
}

pub fn decode_value(bytes: &[u8], col_type: &ColType, is_null: bool) -> Value {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_integer() {
        let val = Value::Integer(42);
        let encoded = encode_value(&val, &ColType::Integer).unwrap();
        assert_eq!(encoded.len(), 8);
        let decoded = decode_value(&encoded, &ColType::Integer, false);
        assert_eq!(decoded, Value::Integer(42));
    }

    #[test]
    fn test_encode_decode_negative_integer() {
        let val = Value::Integer(-999);
        let encoded = encode_value(&val, &ColType::Integer).unwrap();
        let decoded = decode_value(&encoded, &ColType::Integer, false);
        assert_eq!(decoded, Value::Integer(-999));
    }

    #[test]
    fn test_encode_decode_real() {
        let val = Value::Real(3.14);
        let encoded = encode_value(&val, &ColType::Real).unwrap();
        assert_eq!(encoded.len(), 8);
        let decoded = decode_value(&encoded, &ColType::Real, false);
        assert_eq!(decoded, Value::Real(3.14));
    }

    #[test]
    fn test_encode_decode_text() {
        let val = Value::Text("hello".to_string());
        let encoded = encode_value(&val, &ColType::Text).unwrap();
        assert_eq!(encoded.len(), 4 + 5);
        let decoded = decode_value(&encoded, &ColType::Text, false);
        assert_eq!(decoded, Value::Text("hello".to_string()));
    }

    #[test]
    fn test_encode_decode_empty_text() {
        let val = Value::Text(String::new());
        let encoded = encode_value(&val, &ColType::Text).unwrap();
        assert_eq!(encoded.len(), 4);
        let decoded = decode_value(&encoded, &ColType::Text, false);
        assert_eq!(decoded, Value::Text(String::new()));
    }

    #[test]
    fn test_encode_decode_blob() {
        let val = Value::Blob(vec![0xde, 0xad, 0xbe, 0xef]);
        let encoded = encode_value(&val, &ColType::Blob).unwrap();
        assert_eq!(encoded.len(), 4 + 4);
        let decoded = decode_value(&encoded, &ColType::Blob, false);
        assert_eq!(decoded, Value::Blob(vec![0xde, 0xad, 0xbe, 0xef]));
    }

    #[test]
    fn test_null_returns_null_regardless_of_bytes() {
        let bytes = 42i64.to_le_bytes().to_vec();
        let decoded = decode_value(&bytes, &ColType::Integer, true);
        assert_eq!(decoded, Value::Null);
    }

    #[test]
    fn test_integer_promoted_to_real() {
        let val = Value::Integer(5);
        let encoded = encode_value(&val, &ColType::Real).unwrap();
        let decoded = decode_value(&encoded, &ColType::Real, false);
        assert_eq!(decoded, Value::Real(5.0));
    }

    #[test]
    fn test_type_mismatch_errors() {
        let val = Value::Text("hello".to_string());
        let result = encode_value(&val, &ColType::Integer);
        assert!(matches!(result, Err(Scottatura::TypeMismatch { .. })));
    }

    #[test]
    fn test_fixed_width_classification() {
        assert!(is_fixed_width(&ColType::Integer));
        assert!(is_fixed_width(&ColType::Real));
        assert!(!is_fixed_width(&ColType::Text));
        assert!(!is_fixed_width(&ColType::Blob));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test stove::encoding`
Expected: FAIL with "not yet implemented" panics.

- [ ] **Step 3: Implement the functions**

Replace the `todo!()` stubs in `src/stove/encoding.rs`:

```rust
use crate::{ColType, Scottatura, Value};

pub fn is_fixed_width(col_type: &ColType) -> bool {
    matches!(col_type, ColType::Integer | ColType::Real)
}

pub fn encoded_fixed_size(col_type: &ColType) -> usize {
    match col_type {
        ColType::Integer | ColType::Real => 8,
        ColType::Text | ColType::Blob => 0,
    }
}

pub fn encode_value(val: &Value, col_type: &ColType) -> Result<Vec<u8>, Scottatura> {
    match (val, col_type) {
        (Value::Null, ColType::Integer | ColType::Real) => Ok(vec![0u8; 8]),
        (Value::Null, ColType::Text | ColType::Blob) => {
            Ok(0xFFFF_FFFFu32.to_le_bytes().to_vec())
        }
        (Value::Integer(n), ColType::Integer) => Ok(n.to_le_bytes().to_vec()),
        (Value::Integer(n), ColType::Real) => Ok((*n as f64).to_le_bytes().to_vec()),
        (Value::Real(f), ColType::Real) => Ok(f.to_le_bytes().to_vec()),
        (Value::Text(s), ColType::Text) => {
            let b = s.as_bytes();
            let mut out = (b.len() as u32).to_le_bytes().to_vec();
            out.extend_from_slice(b);
            Ok(out)
        }
        (Value::Blob(b), ColType::Blob) => {
            let mut out = (b.len() as u32).to_le_bytes().to_vec();
            out.extend_from_slice(b);
            Ok(out)
        }
        (got, expected) => Err(Scottatura::TypeMismatch {
            got: format!("{got:?}"),
            col: String::new(),
            expected: format!("{expected:?}"),
        }),
    }
}

pub fn decode_value(bytes: &[u8], col_type: &ColType, is_null: bool) -> Value {
    if is_null {
        return Value::Null;
    }
    match col_type {
        ColType::Integer => Value::Integer(i64::from_le_bytes(bytes[..8].try_into().unwrap())),
        ColType::Real => Value::Real(f64::from_le_bytes(bytes[..8].try_into().unwrap())),
        ColType::Text => {
            let len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
            if len == 0xFFFF_FFFF {
                return Value::Null;
            }
            Value::Text(String::from_utf8(bytes[4..4 + len].to_vec()).unwrap_or_default())
        }
        ColType::Blob => {
            let len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
            if len == 0xFFFF_FFFF {
                return Value::Null;
            }
            Value::Blob(bytes[4..4 + len].to_vec())
        }
    }
}
```

- [ ] **Step 4: Add module to stove/mod.rs**

```rust
pub mod encoding;
pub mod pot;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test stove::encoding`
Expected: All 10 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/stove/encoding.rs src/stove/mod.rs
git commit -m "feat: Value encode/decode for all four column types plus null and promotion"
```

---

## Task 3: Page Structure

**Files:**
- Create: `src/stove/page.rs`
- Modify: `src/stove/mod.rs`

**Interfaces:**
- Consumes: nothing from other tasks
- Produces:
  - `pub const PAGE_SIZE: usize = 4096`
  - `pub const PAGE_HEADER_SIZE: usize = 16`
  - `pub const SLOT_SIZE: usize = 8`
  - `pub const TOMBSTONE: u32 = 0xFFFF_FFFF`
  - `pub enum PageType { Heap = 0x01, BtreeInternal = 0x02, BtreeLeaf = 0x03, Overflow = 0x04, Free = 0x05 }`
  - `pub struct Page { pub number: u64, pub data: [u8; PAGE_SIZE] }`
  - `Page::new(number: u64, page_type: PageType) -> Page` — zeroed data, header initialized
  - `Page::page_type(&self) -> PageType`
  - `Page::free_space_offset(&self) -> u16`
  - `Page::set_free_space_offset(&mut self, offset: u16)`
  - `Page::slot_count(&self) -> u32`
  - `Page::set_slot_count(&mut self, count: u32)`
  - `Page::next_page(&self) -> u64`
  - `Page::set_next_page(&mut self, page: u64)`
  - `Page::slot(&self, idx: u32) -> (u32, u32)` — returns (byte_offset_in_page, byte_length)
  - `Page::set_slot(&mut self, idx: u32, offset: u32, length: u32)`

Page header layout (bytes 0–15):
- `[0]` page type u8
- `[1]` flags u8 (reserved, always 0)
- `[2..4]` free_space_offset u16 LE — starts at 16 (right after header)
- `[4..8]` slot_count u32 LE
- `[8..16]` next_page u64 LE

Slot entries start at byte 16, each 8 bytes: `offset: u32 LE` then `length: u32 LE`.

- [ ] **Step 1: Create src/stove/page.rs with failing tests**

```rust
pub const PAGE_SIZE: usize = 4096;
pub const PAGE_HEADER_SIZE: usize = 16;
pub const SLOT_SIZE: usize = 8;
pub const TOMBSTONE: u32 = 0xFFFF_FFFF;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageType {
    Heap = 0x01,
    BtreeInternal = 0x02,
    BtreeLeaf = 0x03,
    Overflow = 0x04,
    Free = 0x05,
}

impl PageType {
    pub fn from_u8(b: u8) -> Option<Self> {
        todo!()
    }
}

pub struct Page {
    pub number: u64,
    pub data: [u8; PAGE_SIZE],
}

impl Page {
    pub fn new(number: u64, page_type: PageType) -> Self { todo!() }
    pub fn page_type(&self) -> PageType { todo!() }
    pub fn free_space_offset(&self) -> u16 { todo!() }
    pub fn set_free_space_offset(&mut self, offset: u16) { todo!() }
    pub fn slot_count(&self) -> u32 { todo!() }
    pub fn set_slot_count(&mut self, count: u32) { todo!() }
    pub fn next_page(&self) -> u64 { todo!() }
    pub fn set_next_page(&mut self, page: u64) { todo!() }
    pub fn slot(&self, idx: u32) -> (u32, u32) { todo!() }
    pub fn set_slot(&mut self, idx: u32, offset: u32, length: u32) { todo!() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_heap_page_header() {
        let page = Page::new(1, PageType::Heap);
        assert_eq!(page.page_type(), PageType::Heap);
        assert_eq!(page.free_space_offset(), PAGE_HEADER_SIZE as u16);
        assert_eq!(page.slot_count(), 0);
        assert_eq!(page.next_page(), 0);
    }

    #[test]
    fn test_set_and_read_slot() {
        let mut page = Page::new(1, PageType::Heap);
        page.set_slot(0, 4000, 80);
        let (off, len) = page.slot(0);
        assert_eq!(off, 4000);
        assert_eq!(len, 80);
    }

    #[test]
    fn test_tombstone_constant() {
        let mut page = Page::new(1, PageType::Heap);
        page.set_slot(0, 100, TOMBSTONE);
        let (_, len) = page.slot(0);
        assert_eq!(len, TOMBSTONE);
    }

    #[test]
    fn test_free_space_offset_roundtrip() {
        let mut page = Page::new(1, PageType::Heap);
        page.set_free_space_offset(256);
        assert_eq!(page.free_space_offset(), 256);
    }

    #[test]
    fn test_next_page_roundtrip() {
        let mut page = Page::new(5, PageType::Heap);
        page.set_next_page(42);
        assert_eq!(page.next_page(), 42);
    }

    #[test]
    fn test_slot_count_roundtrip() {
        let mut page = Page::new(1, PageType::Heap);
        page.set_slot_count(17);
        assert_eq!(page.slot_count(), 17);
    }

    #[test]
    fn test_multiple_slots_independent() {
        let mut page = Page::new(1, PageType::Heap);
        page.set_slot(0, 4000, 50);
        page.set_slot(1, 3940, 60);
        assert_eq!(page.slot(0), (4000, 50));
        assert_eq!(page.slot(1), (3940, 60));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test stove::page`
Expected: FAIL with todo panics.

- [ ] **Step 3: Implement all methods**

Replace the `todo!()` bodies:

```rust
impl PageType {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::Heap),
            0x02 => Some(Self::BtreeInternal),
            0x03 => Some(Self::BtreeLeaf),
            0x04 => Some(Self::Overflow),
            0x05 => Some(Self::Free),
            _ => None,
        }
    }
}

impl Page {
    pub fn new(number: u64, page_type: PageType) -> Self {
        let mut data = [0u8; PAGE_SIZE];
        data[0] = page_type as u8;
        data[2..4].copy_from_slice(&(PAGE_HEADER_SIZE as u16).to_le_bytes());
        Self { number, data }
    }

    pub fn page_type(&self) -> PageType {
        PageType::from_u8(self.data[0]).unwrap_or(PageType::Heap)
    }

    pub fn free_space_offset(&self) -> u16 {
        u16::from_le_bytes(self.data[2..4].try_into().unwrap())
    }

    pub fn set_free_space_offset(&mut self, offset: u16) {
        self.data[2..4].copy_from_slice(&offset.to_le_bytes());
    }

    pub fn slot_count(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    pub fn set_slot_count(&mut self, count: u32) {
        self.data[4..8].copy_from_slice(&count.to_le_bytes());
    }

    pub fn next_page(&self) -> u64 {
        u64::from_le_bytes(self.data[8..16].try_into().unwrap())
    }

    pub fn set_next_page(&mut self, page: u64) {
        self.data[8..16].copy_from_slice(&page.to_le_bytes());
    }

    pub fn slot(&self, idx: u32) -> (u32, u32) {
        let start = PAGE_HEADER_SIZE + (idx as usize) * SLOT_SIZE;
        let offset = u32::from_le_bytes(self.data[start..start + 4].try_into().unwrap());
        let length = u32::from_le_bytes(self.data[start + 4..start + 8].try_into().unwrap());
        (offset, length)
    }

    pub fn set_slot(&mut self, idx: u32, offset: u32, length: u32) {
        let start = PAGE_HEADER_SIZE + (idx as usize) * SLOT_SIZE;
        self.data[start..start + 4].copy_from_slice(&offset.to_le_bytes());
        self.data[start + 4..start + 8].copy_from_slice(&length.to_le_bytes());
    }
}
```

- [ ] **Step 4: Add to stove/mod.rs**

```rust
pub mod encoding;
pub mod page;
pub mod pot;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test stove::page`
Expected: All 7 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/stove/page.rs src/stove/mod.rs
git commit -m "feat: Page struct with 16-byte header, slot directory, PageType enum"
```

---

## Task 4: PotFile (Raw File I/O + Page Allocation)

**Files:**
- Create: `src/stove/pot_file.rs`
- Modify: `src/stove/mod.rs`

**Interfaces:**
- Consumes: `Page`, `PageType`, `PAGE_SIZE` from `stove::page`; `Scottatura` from `lib`
- Produces:
  - `pub struct PotFile`
  - `PotFile::create(path: &str) -> Result<Self, Scottatura>` — creates new `.pot` file, writes file header on page 0
  - `PotFile::open(path: &str) -> Result<Self, Scottatura>` — opens existing file, validates magic bytes
  - `PotFile::read_page(&mut self, page_num: u64) -> Result<Page, Scottatura>`
  - `PotFile::write_page(&mut self, page: &Page) -> Result<(), Scottatura>`
  - `PotFile::allocate_page(&mut self, page_type: PageType) -> Result<Page, Scottatura>` — extends file by one page, updates header
  - `PotFile::page_count(&self) -> u64`
  - `PotFile::root_catalog_page(&self) -> u64`
  - `PotFile::set_root_catalog_page(&mut self, page_num: u64) -> Result<(), Scottatura>`

File header (page 0, bytes 0–35):
- `[0..4]` magic `0x4D524442` u32 LE
- `[4..8]` version `1` u32 LE
- `[8..12]` page_size `4096` u32 LE
- `[12..16]` page_count u32 LE (includes the header page; starts at 1)
- `[16..24]` root_catalog_page u64 LE (0 until set)
- `[24..32]` first_free_page u64 LE (0 for now)
- `[32..36]` CRC32 of bytes 0–31

- [ ] **Step 1: Create src/stove/pot_file.rs with failing tests**

```rust
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use crate::Scottatura;
use crate::stove::page::{Page, PageType, PAGE_SIZE};

const MAGIC: u32 = 0x4D52_4442;
const VERSION: u32 = 1;

pub struct PotFile {
    file: File,
    page_count: u64,
    root_catalog_page: u64,
}

impl PotFile {
    pub fn create(path: &str) -> Result<Self, Scottatura> { todo!() }
    pub fn open(path: &str) -> Result<Self, Scottatura> { todo!() }
    pub fn page_count(&self) -> u64 { todo!() }
    pub fn root_catalog_page(&self) -> u64 { todo!() }
    pub fn set_root_catalog_page(&mut self, page_num: u64) -> Result<(), Scottatura> { todo!() }
    pub fn read_page(&mut self, page_num: u64) -> Result<Page, Scottatura> { todo!() }
    pub fn write_page(&mut self, page: &Page) -> Result<(), Scottatura> { todo!() }
    pub fn allocate_page(&mut self, page_type: PageType) -> Result<Page, Scottatura> { todo!() }
    fn flush_header(&mut self) -> Result<(), Scottatura> { todo!() }
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
    fn test_create_and_reopen() {
        let path = fresh_path();
        {
            let pot = PotFile::create(&path).unwrap();
            assert_eq!(pot.page_count(), 1);
        }
        let pot2 = PotFile::open(&path).unwrap();
        assert_eq!(pot2.page_count(), 1);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_open_bad_magic_errors() {
        let f = NamedTempFile::new().unwrap();
        std::fs::write(f.path(), b"not a pot").unwrap();
        let result = PotFile::open(f.path().to_str().unwrap());
        assert!(matches!(result, Err(crate::Scottatura::NotAPotFile { .. })));
    }

    #[test]
    fn test_allocate_and_read_write_page() {
        let path = fresh_path();
        let mut pot = PotFile::create(&path).unwrap();
        let page = pot.allocate_page(PageType::Heap).unwrap();
        assert_eq!(page.number, 1);
        assert_eq!(pot.page_count(), 2);

        let mut p = Page::new(1, PageType::Heap);
        p.set_next_page(77);
        pot.write_page(&p).unwrap();

        let p2 = pot.read_page(1).unwrap();
        assert_eq!(p2.next_page(), 77);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_root_catalog_page_persists() {
        let path = fresh_path();
        {
            let mut pot = PotFile::create(&path).unwrap();
            pot.set_root_catalog_page(5).unwrap();
        }
        let pot2 = PotFile::open(&path).unwrap();
        assert_eq!(pot2.root_catalog_page(), 5);
        std::fs::remove_file(&path).ok();
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test stove::pot_file`
Expected: FAIL with todo panics.

- [ ] **Step 3: Implement all methods**

```rust
impl PotFile {
    pub fn create(path: &str) -> Result<Self, Scottatura> {
        let mut file = OpenOptions::new()
            .read(true).write(true).create_new(true)
            .open(path)?;

        let mut header = [0u8; PAGE_SIZE];
        header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        header[4..8].copy_from_slice(&VERSION.to_le_bytes());
        header[8..12].copy_from_slice(&(PAGE_SIZE as u32).to_le_bytes());
        header[12..16].copy_from_slice(&1u32.to_le_bytes()); // page_count = 1

        let crc = crc32fast::hash(&header[0..32]);
        header[32..36].copy_from_slice(&crc.to_le_bytes());

        file.write_all(&header)?;
        file.flush()?;
        Ok(Self { file, page_count: 1, root_catalog_page: 0 })
    }

    pub fn open(path: &str) -> Result<Self, Scottatura> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let mut header = [0u8; PAGE_SIZE];
        file.read_exact(&mut header)?;

        let magic = u32::from_le_bytes(header[0..4].try_into().unwrap());
        if magic != MAGIC {
            return Err(Scottatura::NotAPotFile { path: path.to_string() });
        }

        let page_count = u32::from_le_bytes(header[12..16].try_into().unwrap()) as u64;
        let root_catalog_page = u64::from_le_bytes(header[16..24].try_into().unwrap());
        Ok(Self { file, page_count, root_catalog_page })
    }

    pub fn page_count(&self) -> u64 { self.page_count }
    pub fn root_catalog_page(&self) -> u64 { self.root_catalog_page }

    pub fn set_root_catalog_page(&mut self, page_num: u64) -> Result<(), Scottatura> {
        self.root_catalog_page = page_num;
        self.flush_header()
    }

    pub fn read_page(&mut self, page_num: u64) -> Result<Page, Scottatura> {
        self.file.seek(SeekFrom::Start(page_num * PAGE_SIZE as u64))?;
        let mut data = [0u8; PAGE_SIZE];
        self.file.read_exact(&mut data)?;
        Ok(Page { number: page_num, data })
    }

    pub fn write_page(&mut self, page: &Page) -> Result<(), Scottatura> {
        self.file.seek(SeekFrom::Start(page.number * PAGE_SIZE as u64))?;
        self.file.write_all(&page.data)?;
        Ok(())
    }

    pub fn allocate_page(&mut self, page_type: PageType) -> Result<Page, Scottatura> {
        let page_num = self.page_count;
        self.page_count += 1;
        self.flush_header()?;
        let page = Page::new(page_num, page_type);
        self.write_page(&page)?;
        Ok(page)
    }

    fn flush_header(&mut self) -> Result<(), Scottatura> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; PAGE_SIZE];
        self.file.read_exact(&mut header)?;

        header[12..16].copy_from_slice(&(self.page_count as u32).to_le_bytes());
        header[16..24].copy_from_slice(&self.root_catalog_page.to_le_bytes());

        let crc = crc32fast::hash(&header[0..32]);
        header[32..36].copy_from_slice(&crc.to_le_bytes());

        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header)?;
        self.file.flush()?;
        Ok(())
    }
}
```

- [ ] **Step 4: Add to stove/mod.rs**

```rust
pub mod encoding;
pub mod page;
pub mod pot;
pub mod pot_file;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test stove::pot_file`
Expected: All 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/stove/pot_file.rs src/stove/mod.rs
git commit -m "feat: PotFile — .pot file I/O, page allocation, file header with CRC32"
```

---

## Task 5: Warming Tray (LRU Page Cache)

**Files:**
- Modify: `src/warming_tray/mod.rs` (replace stub with implementation)

**Interfaces:**
- Consumes: `Page` from `stove::page`; `PotFile` from `stove::pot_file`; `Scottatura` from `lib`
- Produces:
  - `pub struct CacheStats { pub hits: u64, pub misses: u64 }`
  - `pub struct WarmingTray`
  - `WarmingTray::new(capacity_pages: usize) -> Self`
  - `WarmingTray::fetch(&mut self, page_num: u64, file: &mut PotFile) -> Result<&mut Page, Scottatura>` — load on miss, update LRU tick on hit
  - `WarmingTray::mark_dirty(&mut self, page_num: u64, simmer_id: u64)` — tag page as dirty and owned by simmer
  - `WarmingTray::flush_simmer(&mut self, simmer_id: u64, file: &mut PotFile) -> Result<(), Scottatura>` — write all dirty pages for this simmer to disk, clear dirty flag
  - `WarmingTray::evict_simmer(&mut self, simmer_id: u64)` — drop all pages owned by this simmer without flushing (used on ROLLBACK)
  - `WarmingTray::stats(&self) -> CacheStats` — returns a copy

Eviction: when at capacity, evict the LRU clean page. Never evict a dirty page (that would lose in-progress writes).

- [ ] **Step 1: Write the implementation with failing tests inline**

```rust
use std::collections::HashMap;
use crate::Scottatura;
use crate::stove::{page::Page, pot_file::PotFile};

#[derive(Clone, Copy, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
}

struct CachedPage {
    page: Page,
    dirty: bool,
    simmer_id: Option<u64>,
    lru_tick: u64,
}

pub struct WarmingTray {
    capacity: usize,
    cache: HashMap<u64, CachedPage>,
    tick: u64,
    stats: CacheStats,
}

impl WarmingTray {
    pub fn new(capacity_pages: usize) -> Self {
        Self { capacity: capacity_pages, cache: HashMap::new(), tick: 0, stats: CacheStats::default() }
    }

    pub fn fetch(&mut self, page_num: u64, file: &mut PotFile) -> Result<&mut Page, Scottatura> {
        self.tick += 1;
        let tick = self.tick;
        if !self.cache.contains_key(&page_num) {
            self.stats.misses += 1;
            if self.cache.len() >= self.capacity {
                self.evict_one_clean();
            }
            let page = file.read_page(page_num)?;
            self.cache.insert(page_num, CachedPage { page, dirty: false, simmer_id: None, lru_tick: tick });
        } else {
            self.stats.hits += 1;
        }
        let entry = self.cache.get_mut(&page_num).unwrap();
        entry.lru_tick = tick;
        Ok(&mut entry.page)
    }

    pub fn mark_dirty(&mut self, page_num: u64, simmer_id: u64) {
        if let Some(entry) = self.cache.get_mut(&page_num) {
            entry.dirty = true;
            entry.simmer_id = Some(simmer_id);
        }
    }

    pub fn flush_simmer(&mut self, simmer_id: u64, file: &mut PotFile) -> Result<(), Scottatura> {
        for entry in self.cache.values_mut() {
            if entry.simmer_id == Some(simmer_id) && entry.dirty {
                file.write_page(&entry.page)?;
                entry.dirty = false;
                entry.simmer_id = None;
            }
        }
        Ok(())
    }

    pub fn evict_simmer(&mut self, simmer_id: u64) {
        self.cache.retain(|_, e| e.simmer_id != Some(simmer_id));
    }

    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    fn evict_one_clean(&mut self) {
        let to_evict = self.cache.iter()
            .filter(|(_, e)| !e.dirty)
            .min_by_key(|(_, e)| e.lru_tick)
            .map(|(k, _)| *k);
        if let Some(k) = to_evict {
            self.cache.remove(&k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use crate::stove::{page::PageType, pot_file::PotFile};

    fn make_pot_with_page() -> (PotFile, String) {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_string();
        drop(f);
        let mut pot = PotFile::create(&path).unwrap();
        pot.allocate_page(PageType::Heap).unwrap(); // page 1
        (pot, path)
    }

    #[test]
    fn test_miss_then_hit() {
        let (mut pot, path) = make_pot_with_page();
        let mut tray = WarmingTray::new(16);
        tray.fetch(1, &mut pot).unwrap();
        assert_eq!(tray.stats().misses, 1);
        assert_eq!(tray.stats().hits, 0);
        tray.fetch(1, &mut pot).unwrap();
        assert_eq!(tray.stats().hits, 1);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_mark_dirty_and_flush_writes_to_disk() {
        let (mut pot, path) = make_pot_with_page();
        let mut tray = WarmingTray::new(16);
        let page = tray.fetch(1, &mut pot).unwrap();
        page.set_next_page(77);
        tray.mark_dirty(1, 42);
        tray.flush_simmer(42, &mut pot).unwrap();
        let on_disk = pot.read_page(1).unwrap();
        assert_eq!(on_disk.next_page(), 77);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_evict_simmer_discards_without_flush() {
        let (mut pot, path) = make_pot_with_page();
        let mut tray = WarmingTray::new(16);
        let page = tray.fetch(1, &mut pot).unwrap();
        page.set_next_page(99);
        tray.mark_dirty(1, 55);
        tray.evict_simmer(55);
        // Re-fetch from disk (was never flushed)
        let page = tray.fetch(1, &mut pot).unwrap();
        assert_eq!(page.next_page(), 0);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_lru_evicts_clean_pages_at_capacity() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_string();
        drop(f);
        let mut pot = PotFile::create(&path).unwrap();
        for _ in 0..5 {
            pot.allocate_page(PageType::Heap).unwrap();
        }
        let mut tray = WarmingTray::new(3); // capacity of 3
        tray.fetch(1, &mut pot).unwrap();
        tray.fetch(2, &mut pot).unwrap();
        tray.fetch(3, &mut pot).unwrap();
        // Page 1 is LRU; fetching page 4 should evict it
        tray.fetch(4, &mut pot).unwrap();
        assert_eq!(tray.stats().misses, 4);
        // Page 1 is now a miss again
        tray.fetch(1, &mut pot).unwrap();
        assert_eq!(tray.stats().misses, 5);
        std::fs::remove_file(&path).ok();
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test warming_tray`
Expected: FAIL — the old stub file has no implementation.

- [ ] **Step 3: Replace the stub content with the implementation above**

The file content is the entire block from Step 1 (implementation + tests together).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test warming_tray`
Expected: All 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/warming_tray/mod.rs
git commit -m "feat: WarmingTray — LRU page cache with dirty tracking and simmer-owned eviction"
```

---

## Task 6: Heap Page Operations

**Files:**
- Create: `src/stove/heap.rs`
- Modify: `src/stove/mod.rs`

**Interfaces:**
- Consumes: `Page`, `PAGE_SIZE`, `PAGE_HEADER_SIZE`, `SLOT_SIZE`, `TOMBSTONE` from `stove::page`; `Value`, `ColumnDef`, `ColType`, `Scottatura` from `lib`; `encode_value`, `decode_value` from `stove::encoding`
- Produces:
  - `pub fn encode_row(al_dente_key: i64, values: &[Value], cols: &[ColumnDef]) -> Result<Vec<u8>, Scottatura>`
  - `pub fn decode_row(data: &[u8], cols: &[ColumnDef]) -> (i64, Vec<Value>)`
  - `pub fn heap_insert(page: &mut Page, row_bytes: &[u8]) -> Option<u32>` — returns slot index, or None if full
  - `pub fn heap_scan(page: &Page, cols: &[ColumnDef]) -> Vec<(i64, Vec<Value>)>` — skips tombstones
  - `pub fn heap_delete(page: &mut Page, al_dente_key: i64, cols: &[ColumnDef]) -> bool` — tombstones matching row, returns true if found

Row layout:
```
[al_dente_key: i64, 8 bytes LE]
[null_bitmap: ceil(col_count / 8) bytes, bit i (LSB-first) = 1 if column i is NULL]
[col0_data][col1_data]...[colN_data]
```

Variable-length null column takes exactly 4 bytes on disk (the 0xFFFF_FFFF sentinel from `encode_value`).

Heap insert: row data grows backward from byte 4095. Slot entries grow forward from byte 16. Page is full when `free_space_offset + SLOT_SIZE + row_len > lowest_row_start`.

- [ ] **Step 1: Create src/stove/heap.rs with failing tests**

```rust
use crate::{ColType, ColumnDef, Scottatura, Value};
use crate::stove::encoding::{decode_value, encode_value};
use crate::stove::page::{Page, PAGE_HEADER_SIZE, PAGE_SIZE, SLOT_SIZE, TOMBSTONE};

pub fn encode_row(al_dente_key: i64, values: &[Value], cols: &[ColumnDef]) -> Result<Vec<u8>, Scottatura> {
    todo!()
}

pub fn decode_row(data: &[u8], cols: &[ColumnDef]) -> (i64, Vec<Value>) {
    todo!()
}

pub fn heap_insert(page: &mut Page, row_bytes: &[u8]) -> Option<u32> {
    todo!()
}

pub fn heap_scan(page: &Page, cols: &[ColumnDef]) -> Vec<(i64, Vec<Value>)> {
    todo!()
}

pub fn heap_delete(page: &mut Page, al_dente_key: i64, cols: &[ColumnDef]) -> bool {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColType, ColumnDef, Value};
    use crate::stove::page::{Page, PageType};

    fn two_cols() -> Vec<ColumnDef> {
        vec![
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
            ColumnDef { name: "age".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
        ]
    }

    #[test]
    fn test_encode_decode_row_roundtrip() {
        let cols = two_cols();
        let vals = vec![Value::Text("Tony".into()), Value::Integer(38)];
        let bytes = encode_row(1, &vals, &cols).unwrap();
        let (key, decoded) = decode_row(&bytes, &cols);
        assert_eq!(key, 1);
        assert_eq!(decoded, vals);
    }

    #[test]
    fn test_encode_decode_row_with_null() {
        let cols = two_cols();
        let vals = vec![Value::Null, Value::Integer(25)];
        let bytes = encode_row(2, &vals, &cols).unwrap();
        let (key, decoded) = decode_row(&bytes, &cols);
        assert_eq!(key, 2);
        assert_eq!(decoded[0], Value::Null);
        assert_eq!(decoded[1], Value::Integer(25));
    }

    #[test]
    fn test_heap_insert_and_scan() {
        let cols = two_cols();
        let mut page = Page::new(1, PageType::Heap);
        let r1 = encode_row(1, &[Value::Text("Tony".into()), Value::Integer(38)], &cols).unwrap();
        let r2 = encode_row(2, &[Value::Text("Maria".into()), Value::Integer(29)], &cols).unwrap();
        heap_insert(&mut page, &r1).unwrap();
        heap_insert(&mut page, &r2).unwrap();
        let rows = heap_scan(&page, &cols);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, 1);
        assert_eq!(rows[1].0, 2);
        assert_eq!(rows[0].1[0], Value::Text("Tony".into()));
    }

    #[test]
    fn test_heap_delete_tombstones_correct_row() {
        let cols = two_cols();
        let mut page = Page::new(1, PageType::Heap);
        heap_insert(&mut page, &encode_row(1, &[Value::Text("Tony".into()), Value::Integer(38)], &cols).unwrap()).unwrap();
        heap_insert(&mut page, &encode_row(2, &[Value::Text("Maria".into()), Value::Integer(29)], &cols).unwrap()).unwrap();
        assert!(heap_delete(&mut page, 1, &cols));
        let rows = heap_scan(&page, &cols);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, 2);
    }

    #[test]
    fn test_heap_delete_nonexistent_returns_false() {
        let cols = two_cols();
        let mut page = Page::new(1, PageType::Heap);
        heap_insert(&mut page, &encode_row(1, &[Value::Text("Tony".into()), Value::Integer(38)], &cols).unwrap()).unwrap();
        assert!(!heap_delete(&mut page, 99, &cols));
    }

    #[test]
    fn test_heap_returns_none_when_full() {
        // A large row that fills the page quickly
        let cols = vec![ColumnDef { name: "data".into(), col_type: ColType::Blob, nullable: false, strainer_root: None }];
        let big = Value::Blob(vec![0u8; 512]);
        let mut page = Page::new(1, PageType::Heap);
        let row = encode_row(1, &[big], &cols).unwrap();
        // Fill the page
        let mut inserted = 0;
        while heap_insert(&mut page, &row).is_some() {
            inserted += 1;
            if inserted > 100 { break; } // safety valve
        }
        // One more insert should return None
        assert!(heap_insert(&mut page, &row).is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test stove::heap`
Expected: FAIL with todo panics.

- [ ] **Step 3: Implement all five functions**

```rust
pub fn encode_row(al_dente_key: i64, values: &[Value], cols: &[ColumnDef]) -> Result<Vec<u8>, Scottatura> {
    let mut out = Vec::new();
    out.extend_from_slice(&al_dente_key.to_le_bytes());

    let bitmap_bytes = (cols.len() + 7) / 8;
    let mut bitmap = vec![0u8; bitmap_bytes];
    for (i, val) in values.iter().enumerate() {
        if matches!(val, Value::Null) {
            bitmap[i / 8] |= 1 << (i % 8);
        }
    }
    out.extend_from_slice(&bitmap);

    for (val, col) in values.iter().zip(cols.iter()) {
        out.extend(encode_value(val, &col.col_type)?);
    }
    Ok(out)
}

pub fn decode_row(data: &[u8], cols: &[ColumnDef]) -> (i64, Vec<Value>) {
    let key = i64::from_le_bytes(data[0..8].try_into().unwrap());
    let bitmap_bytes = (cols.len() + 7) / 8;
    let bitmap = &data[8..8 + bitmap_bytes];
    let mut cursor = 8 + bitmap_bytes;

    let mut values = Vec::with_capacity(cols.len());
    for (i, col) in cols.iter().enumerate() {
        let is_null = (bitmap[i / 8] >> (i % 8)) & 1 == 1;
        let val = decode_value(&data[cursor..], &col.col_type, is_null);
        cursor += match col.col_type {
            ColType::Integer | ColType::Real => 8,
            ColType::Text | ColType::Blob => {
                if is_null {
                    4 // just the sentinel
                } else {
                    4 + u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize
                }
            }
        };
        values.push(val);
    }
    (key, values)
}

pub fn heap_insert(page: &mut Page, row_bytes: &[u8]) -> Option<u32> {
    let slot_count = page.slot_count();
    let free_off = page.free_space_offset() as usize;
    let row_end = if slot_count == 0 {
        PAGE_SIZE
    } else {
        page.slot(slot_count - 1).0 as usize
    };
    if free_off + SLOT_SIZE + row_bytes.len() > row_end {
        return None;
    }
    let row_start = row_end - row_bytes.len();
    page.data[row_start..row_end].copy_from_slice(row_bytes);
    page.set_slot(slot_count, row_start as u32, row_bytes.len() as u32);
    page.set_slot_count(slot_count + 1);
    page.set_free_space_offset((free_off + SLOT_SIZE) as u16);
    Some(slot_count)
}

pub fn heap_scan(page: &Page, cols: &[ColumnDef]) -> Vec<(i64, Vec<Value>)> {
    (0..page.slot_count())
        .filter_map(|i| {
            let (off, len) = page.slot(i);
            if len == TOMBSTONE { return None; }
            Some(decode_row(&page.data[off as usize..(off + len) as usize], cols))
        })
        .collect()
}

pub fn heap_delete(page: &mut Page, al_dente_key: i64, cols: &[ColumnDef]) -> bool {
    for i in 0..page.slot_count() {
        let (off, len) = page.slot(i);
        if len == TOMBSTONE { continue; }
        let (key, _) = decode_row(&page.data[off as usize..(off + len) as usize], cols);
        if key == al_dente_key {
            page.set_slot(i, off, TOMBSTONE);
            return true;
        }
    }
    false
}
```

- [ ] **Step 4: Add to stove/mod.rs**

```rust
pub mod encoding;
pub mod heap;
pub mod page;
pub mod pot;
pub mod pot_file;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test stove::heap`
Expected: All 6 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/stove/heap.rs src/stove/mod.rs
git commit -m "feat: heap page ops — row encode/decode with null bitmap, insert, scan, delete"
```

---

## Task 7: Catalog

**Files:**
- Create: `src/stove/catalog.rs`
- Modify: `src/stove/mod.rs`

**Interfaces:**
- Consumes: `Page`, `PageType`, `PAGE_HEADER_SIZE` from `stove::page`; `PotFile` from `stove::pot_file`; `WarmingTray` from `warming_tray`; `ColumnDef`, `Scottatura` from `lib`; `bincode`
- Produces:
  - `pub struct NoodleEntry { pub name: String, pub root_heap_page: u64, pub root_strainer_page: u64, pub cols: Vec<ColumnDef> }` (derives `Serialize`, `Deserialize`, `Clone`)
  - `pub struct Catalog`
  - `Catalog::init(tray: &mut WarmingTray, pot: &mut PotFile) -> Result<Self, Scottatura>` — allocates the catalog page, sets root_catalog_page in file header
  - `Catalog::load(tray: &mut WarmingTray, pot: &mut PotFile) -> Result<Self, Scottatura>` — reads from root_catalog_page
  - `Catalog::list_noodles(&self) -> Vec<String>`
  - `Catalog::get_recipe(&self, name: &str) -> Option<&NoodleEntry>`
  - `Catalog::get_recipe_mut(&mut self, name: &str) -> Option<&mut NoodleEntry>`
  - `Catalog::create_noodle(&mut self, name: &str, cols: Vec<ColumnDef>, tray: &mut WarmingTray, pot: &mut PotFile) -> Result<u64, Scottatura>` — allocates the noodle's first heap page, returns its page number
  - `Catalog::drop_noodle(&mut self, name: &str, tray: &mut WarmingTray, pot: &mut PotFile) -> Result<(), Scottatura>`

Catalog persistence: `entries` are `bincode::serialize`d and stored in the catalog page starting at byte `PAGE_HEADER_SIZE`. The first 4 bytes at `PAGE_HEADER_SIZE` are the serialized length (u32 LE). Max catalog size per page: `4096 - 16 - 4 = 4076` bytes (sufficient for dozens of noodles in v1). Catalog writes use `simmer_id = 0` (auto-flush).

- [ ] **Step 1: Create src/stove/catalog.rs with failing tests**

```rust
use serde::{Deserialize, Serialize};
use crate::{ColumnDef, Scottatura};
use crate::stove::page::{Page, PageType, PAGE_HEADER_SIZE};
use crate::stove::pot_file::PotFile;
use crate::warming_tray::WarmingTray;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoodleEntry {
    pub name: String,
    pub root_heap_page: u64,
    pub root_strainer_page: u64,
    pub cols: Vec<ColumnDef>,
}

pub struct Catalog {
    entries: Vec<NoodleEntry>,
    catalog_page_num: u64,
}

impl Catalog {
    pub fn init(tray: &mut WarmingTray, pot: &mut PotFile) -> Result<Self, Scottatura> { todo!() }
    pub fn load(tray: &mut WarmingTray, pot: &mut PotFile) -> Result<Self, Scottatura> { todo!() }
    pub fn list_noodles(&self) -> Vec<String> { todo!() }
    pub fn get_recipe(&self, name: &str) -> Option<&NoodleEntry> { todo!() }
    pub fn get_recipe_mut(&mut self, name: &str) -> Option<&mut NoodleEntry> { todo!() }
    pub fn create_noodle(&mut self, name: &str, cols: Vec<ColumnDef>, tray: &mut WarmingTray, pot: &mut PotFile) -> Result<u64, Scottatura> { todo!() }
    pub fn drop_noodle(&mut self, name: &str, tray: &mut WarmingTray, pot: &mut PotFile) -> Result<(), Scottatura> { todo!() }
    fn persist(&mut self, tray: &mut WarmingTray, pot: &mut PotFile) -> Result<(), Scottatura> { todo!() }
    fn deserialize_entries(page: &Page) -> Vec<NoodleEntry> { todo!() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use crate::{ColType, ColumnDef};
    use crate::stove::pot_file::PotFile;
    use crate::warming_tray::WarmingTray;

    fn setup() -> (PotFile, WarmingTray, String) {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_string();
        drop(f);
        (PotFile::create(&path).unwrap(), WarmingTray::new(64), path)
    }

    fn user_cols() -> Vec<ColumnDef> {
        vec![
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
            ColumnDef { name: "age".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
        ]
    }

    #[test]
    fn test_create_and_list() {
        let (mut pot, mut tray, path) = setup();
        let mut cat = Catalog::init(&mut tray, &mut pot).unwrap();
        cat.create_noodle("users", user_cols(), &mut tray, &mut pot).unwrap();
        assert_eq!(cat.list_noodles(), vec!["users"]);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_get_recipe_returns_correct_columns() {
        let (mut pot, mut tray, path) = setup();
        let mut cat = Catalog::init(&mut tray, &mut pot).unwrap();
        cat.create_noodle("users", user_cols(), &mut tray, &mut pot).unwrap();
        let entry = cat.get_recipe("users").unwrap();
        assert_eq!(entry.cols[0].name, "name");
        assert_eq!(entry.cols[1].name, "age");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_duplicate_name_errors() {
        let (mut pot, mut tray, path) = setup();
        let mut cat = Catalog::init(&mut tray, &mut pot).unwrap();
        cat.create_noodle("users", user_cols(), &mut tray, &mut pot).unwrap();
        assert!(matches!(
            cat.create_noodle("users", user_cols(), &mut tray, &mut pot),
            Err(crate::Scottatura::NoodleAlreadyExists { .. })
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_drop_removes_entry() {
        let (mut pot, mut tray, path) = setup();
        let mut cat = Catalog::init(&mut tray, &mut pot).unwrap();
        cat.create_noodle("users", user_cols(), &mut tray, &mut pot).unwrap();
        cat.drop_noodle("users", &mut tray, &mut pot).unwrap();
        assert!(cat.list_noodles().is_empty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_persists_across_reopen() {
        let (mut pot, mut tray, path) = setup();
        {
            let mut cat = Catalog::init(&mut tray, &mut pot).unwrap();
            cat.create_noodle("orders", user_cols(), &mut tray, &mut pot).unwrap();
        }
        drop(pot);
        let mut pot2 = PotFile::open(&path).unwrap();
        let mut tray2 = WarmingTray::new(64);
        let cat2 = Catalog::load(&mut tray2, &mut pot2).unwrap();
        assert_eq!(cat2.list_noodles(), vec!["orders"]);
        std::fs::remove_file(&path).ok();
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test stove::catalog`
Expected: FAIL with todo panics.

- [ ] **Step 3: Implement all methods**

```rust
impl Catalog {
    pub fn init(tray: &mut WarmingTray, pot: &mut PotFile) -> Result<Self, Scottatura> {
        let page = pot.allocate_page(PageType::Heap)?;
        let page_num = page.number;
        pot.set_root_catalog_page(page_num)?;
        tray.fetch(page_num, pot)?;
        Ok(Self { entries: Vec::new(), catalog_page_num: page_num })
    }

    pub fn load(tray: &mut WarmingTray, pot: &mut PotFile) -> Result<Self, Scottatura> {
        let page_num = pot.root_catalog_page();
        let page = tray.fetch(page_num, pot)?;
        let entries = Self::deserialize_entries(page);
        Ok(Self { entries, catalog_page_num: page_num })
    }

    pub fn list_noodles(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.name.clone()).collect()
    }

    pub fn get_recipe(&self, name: &str) -> Option<&NoodleEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    pub fn get_recipe_mut(&mut self, name: &str) -> Option<&mut NoodleEntry> {
        self.entries.iter_mut().find(|e| e.name == name)
    }

    pub fn create_noodle(
        &mut self, name: &str, cols: Vec<ColumnDef>,
        tray: &mut WarmingTray, pot: &mut PotFile,
    ) -> Result<u64, Scottatura> {
        if self.entries.iter().any(|e| e.name == name) {
            return Err(Scottatura::NoodleAlreadyExists { name: name.to_string() });
        }
        let heap_page = pot.allocate_page(PageType::Heap)?;
        self.entries.push(NoodleEntry {
            name: name.to_string(),
            root_heap_page: heap_page.number,
            root_strainer_page: 0,
            cols,
        });
        self.persist(tray, pot)?;
        Ok(heap_page.number)
    }

    pub fn drop_noodle(
        &mut self, name: &str,
        tray: &mut WarmingTray, pot: &mut PotFile,
    ) -> Result<(), Scottatura> {
        let pos = self.entries.iter().position(|e| e.name == name)
            .ok_or_else(|| Scottatura::NoodleNotFound { name: name.to_string() })?;
        self.entries.remove(pos);
        self.persist(tray, pot)
    }

    fn persist(&mut self, tray: &mut WarmingTray, pot: &mut PotFile) -> Result<(), Scottatura> {
        let encoded = bincode::serialize(&self.entries)
            .map_err(|e| Scottatura::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        let page = tray.fetch(self.catalog_page_num, pot)?;
        let len = encoded.len() as u32;
        page.data[PAGE_HEADER_SIZE..PAGE_HEADER_SIZE + 4].copy_from_slice(&len.to_le_bytes());
        page.data[PAGE_HEADER_SIZE + 4..PAGE_HEADER_SIZE + 4 + encoded.len()]
            .copy_from_slice(&encoded);
        tray.mark_dirty(self.catalog_page_num, 0);
        tray.flush_simmer(0, pot)
    }

    fn deserialize_entries(page: &Page) -> Vec<NoodleEntry> {
        let len = u32::from_le_bytes(
            page.data[PAGE_HEADER_SIZE..PAGE_HEADER_SIZE + 4].try_into().unwrap()
        ) as usize;
        if len == 0 { return Vec::new(); }
        let data = &page.data[PAGE_HEADER_SIZE + 4..PAGE_HEADER_SIZE + 4 + len];
        bincode::deserialize(data).unwrap_or_default()
    }
}
```

- [ ] **Step 4: Add to stove/mod.rs**

```rust
pub mod catalog;
pub mod encoding;
pub mod heap;
pub mod page;
pub mod pot;
pub mod pot_file;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test stove::catalog`
Expected: All 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/stove/catalog.rs src/stove/mod.rs
git commit -m "feat: Catalog — noodle definitions with create/drop, persists via bincode"
```

---

## Task 8: Pot API + Integration Tests

**Files:**
- Modify: `src/stove/pot.rs` (replace stub)
- Modify: `src/lib.rs` (add `pub use`)
- Create: `tests/foundation.rs`

**Interfaces:**
- Consumes: all previous tasks
- Produces the public API from `lib.rs`:
  - `Pot::open(path: &str) -> Result<Self, Scottatura>` — creates if absent, opens if exists
  - `Pot::noodles(&self) -> Vec<String>`
  - `Pot::recipe(&self, noodle: &str) -> Result<crate::Recipe, Scottatura>`
  - `Pot::create_table(&mut self, name: &str, cols: Vec<ColumnDef>) -> Result<(), Scottatura>`
  - `Pot::drop_table(&mut self, name: &str) -> Result<(), Scottatura>`
  - `Pot::insert_strand(&mut self, noodle: &str, values: Vec<Value>) -> Result<i64, Scottatura>` — auto-increments Al Dente Key from 1
  - `Pot::scan_noodle(&mut self, noodle: &str) -> Result<Vec<(i64, Vec<Value>)>, Scottatura>`

Note: Multi-page heap traversal is **not** implemented in this phase. A single heap page holds approximately 40–60 typical rows. The executor in Phase 2 will add heap-chain traversal (following `next_page` pointers to continue to the next page when a heap page is full).

- [ ] **Step 1: Write the integration tests first**

Create `tests/foundation.rs`:

```rust
use marinaradb::{ColType, ColumnDef, Value};

fn tmp_path(tag: &str) -> String {
    format!("/tmp/marinara_test_{tag}_{}.pot",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap()
            .subsec_nanos())
}

fn user_cols() -> Vec<ColumnDef> {
    vec![
        ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
        ColumnDef { name: "age".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
    ]
}

#[test]
fn test_create_table_and_list_noodles() {
    let path = tmp_path("list");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("users", user_cols()).unwrap();
    assert!(pot.noodles().contains(&"users".to_string()));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_recipe_returns_correct_columns() {
    let path = tmp_path("recipe");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("users", user_cols()).unwrap();
    let recipe = pot.recipe("users").unwrap();
    assert_eq!(recipe.noodle_name, "users");
    assert_eq!(recipe.columns[0].name, "name");
    assert_eq!(recipe.columns[1].name, "age");
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_insert_returns_incrementing_keys() {
    let path = tmp_path("keys");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("users", user_cols()).unwrap();
    let k1 = pot.insert_strand("users", vec![Value::Text("Tony".into()), Value::Integer(38)]).unwrap();
    let k2 = pot.insert_strand("users", vec![Value::Text("Maria".into()), Value::Integer(29)]).unwrap();
    assert_eq!(k1, 1);
    assert_eq!(k2, 2);
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_scan_returns_inserted_rows_in_order() {
    let path = tmp_path("scan");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("users", user_cols()).unwrap();
    pot.insert_strand("users", vec![Value::Text("Tony".into()), Value::Integer(38)]).unwrap();
    pot.insert_strand("users", vec![Value::Text("Maria".into()), Value::Integer(29)]).unwrap();
    let rows = pot.scan_noodle("users").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].1[0], Value::Text("Tony".into()));
    assert_eq!(rows[1].1[1], Value::Integer(29));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_not_null_violation_errors() {
    let path = tmp_path("notnull");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("users", user_cols()).unwrap();
    // "age" is NOT NULL
    let result = pot.insert_strand("users", vec![Value::Text("Tony".into()), Value::Null]);
    assert!(matches!(result, Err(marinaradb::Scottatura::NullConstraintViolation { .. })));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_data_persists_across_reopen() {
    let path = tmp_path("persist");
    {
        let mut pot = marinaradb::Pot::open(&path).unwrap();
        pot.create_table("nums", vec![
            ColumnDef { name: "x".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
        ]).unwrap();
        pot.insert_strand("nums", vec![Value::Integer(99)]).unwrap();
    }
    let mut pot2 = marinaradb::Pot::open(&path).unwrap();
    let rows = pot2.scan_noodle("nums").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1[0], Value::Integer(99));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_drop_table_removes_noodle() {
    let path = tmp_path("drop");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("tmp", user_cols()).unwrap();
    pot.drop_table("tmp").unwrap();
    assert!(!pot.noodles().contains(&"tmp".to_string()));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_missing_noodle_errors() {
    let path = tmp_path("missing");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    let result = pot.scan_noodle("ghosts");
    assert!(matches!(result, Err(marinaradb::Scottatura::NoodleNotFound { .. })));
    std::fs::remove_file(&path).ok();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test foundation`
Expected: FAIL — `Pot` has no methods.

- [ ] **Step 3: Implement src/stove/pot.rs**

```rust
use crate::{ColumnDef, Recipe, Scottatura, Value};
use crate::stove::catalog::Catalog;
use crate::stove::heap::{encode_row, heap_insert, heap_scan};
use crate::stove::page::PageType;
use crate::stove::pot_file::PotFile;
use crate::warming_tray::WarmingTray;

pub struct Pot {
    pot_file: PotFile,
    tray: WarmingTray,
    catalog: Catalog,
    next_key: i64,
}

impl Pot {
    pub fn open(path: &str) -> Result<Self, Scottatura> {
        let (mut pot_file, mut tray, catalog) = if std::path::Path::new(path).exists() {
            let mut f = PotFile::open(path)?;
            let mut t = WarmingTray::new(256);
            let c = Catalog::load(&mut t, &mut f)?;
            (f, t, c)
        } else {
            let mut f = PotFile::create(path)?;
            let mut t = WarmingTray::new(256);
            let c = Catalog::init(&mut t, &mut f)?;
            (f, t, c)
        };
        Ok(Self { pot_file, tray, catalog, next_key: 1 })
    }

    pub fn noodles(&self) -> Vec<String> {
        self.catalog.list_noodles()
    }

    pub fn recipe(&self, noodle: &str) -> Result<Recipe, Scottatura> {
        let e = self.catalog.get_recipe(noodle)
            .ok_or_else(|| Scottatura::NoodleNotFound { name: noodle.to_string() })?;
        Ok(Recipe { noodle_name: e.name.clone(), columns: e.cols.clone() })
    }

    pub fn create_table(&mut self, name: &str, cols: Vec<ColumnDef>) -> Result<(), Scottatura> {
        self.catalog.create_noodle(name, cols, &mut self.tray, &mut self.pot_file)?;
        Ok(())
    }

    pub fn drop_table(&mut self, name: &str) -> Result<(), Scottatura> {
        self.catalog.drop_noodle(name, &mut self.tray, &mut self.pot_file)
    }

    pub fn insert_strand(&mut self, noodle: &str, values: Vec<Value>) -> Result<i64, Scottatura> {
        let entry = self.catalog.get_recipe(noodle)
            .ok_or_else(|| Scottatura::NoodleNotFound { name: noodle.to_string() })?
            .clone();

        for (val, col) in values.iter().zip(entry.cols.iter()) {
            if matches!(val, Value::Null) && !col.nullable {
                return Err(Scottatura::NullConstraintViolation { col: col.name.clone() });
            }
        }

        let key = self.next_key;
        self.next_key += 1;
        let row_bytes = encode_row(key, &values, &entry.cols)?;

        let page_num = entry.root_heap_page;
        let page = self.tray.fetch(page_num, &mut self.pot_file)?;
        if heap_insert(page, &row_bytes).is_none() {
            return Err(Scottatura::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "heap page full — multi-page heap implemented in Phase 2",
            )));
        }
        self.tray.mark_dirty(page_num, 0);
        self.tray.flush_simmer(0, &mut self.pot_file)?;
        Ok(key)
    }

    pub fn scan_noodle(&mut self, noodle: &str) -> Result<Vec<(i64, Vec<Value>)>, Scottatura> {
        let entry = self.catalog.get_recipe(noodle)
            .ok_or_else(|| Scottatura::NoodleNotFound { name: noodle.to_string() })?
            .clone();
        let page = self.tray.fetch(entry.root_heap_page, &mut self.pot_file)?;
        Ok(heap_scan(page, &entry.cols))
    }
}
```

- [ ] **Step 4: Update src/lib.rs to export Pot**

The `pub use stove::pot::Pot;` line was already added in Task 1 Step 3. Verify it's there; add it if missing:

```rust
pub use stove::pot::Pot;
```

- [ ] **Step 5: Run integration tests**

Run: `cargo test --test foundation`
Expected: All 8 tests pass.

- [ ] **Step 6: Run the full test suite**

Run: `cargo test`
Expected: All tests pass (unit + integration).

- [ ] **Step 7: Commit**

```bash
git add src/stove/pot.rs src/lib.rs tests/foundation.rs
git commit -m "feat: Pot public API — create/drop table, insert strands, scan, persists on disk"
```

---

## Self-Review Checklist (run before calling Phase 1 complete)

- [ ] `cargo test` passes with zero failures
- [ ] `cargo build` emits no errors
- [ ] All test files use real `tempfile`/disk paths — no mocks for I/O
- [ ] Every `todo!()` from Step 1 stubs is replaced in Step 3

---

## What Phase 2 Adds

Phase 2 (SQL Engine) will add:
- Full `Scottatura` error set (all codes from `docs/error-taxonomy.md`)
- `sqlparser-rs` wrapper in `src/parser/`
- Sous Chef query planner (`Plan` enum, index selection)
- Stove executor (walks `Plan`, handles SELECT/INSERT/UPDATE/DELETE + WHERE/JOIN/ORDER BY/LIMIT)
- Multi-page heap traversal (following `next_page` pointers)
- `Pot::execute(sql: &str) -> Result<ResultSet, Scottatura>`
- `marinara` REPL (`clap` + `rustyline`)

Deliverable: A working REPL where you run SQL against a `.pot` file.
