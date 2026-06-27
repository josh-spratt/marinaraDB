# MarinaraDB — Full Design Document

**Date:** 2026-06-27
**Status:** Approved

---

## Summary

MarinaraDB is a fully functional toy RDBMS written in Rust (2021 edition). It speaks standard SQL but names every internal component after pasta and Italian food. This document is the production-quality design reference a second engineer can use to implement the system independently.

The design is hub-and-spoke: this document is the index; each subsystem has its own companion doc under `docs/`.

---

## Decisions Made

| Topic | Decision | Rationale |
|---|---|---|
| Data types | `INTEGER`, `TEXT`, `REAL`, `BLOB` | SQLite-style minimal set; sufficient for a toy DB |
| Page size | 4096 bytes | Standard; matches OS page size; same as SQLite default |
| Isolation level | Serializable | Strongest ACID guarantee; v1 uses single write lock |
| WAL style | Redo-only with CRC32 checksums | SQLite-style; simpler than undo/redo; safe with no-dirty-flush rule |
| Testing | Unit tests per module + SQL integration tests | Two-layer coverage without overengineering |

---

## Architecture

```
┌──────────────────────────────────────┐
│           marinara CLI / REPL        │
└─────────────────┬────────────────────┘
                  │ SQL string
┌─────────────────▼────────────────────┐
│            SQL Parser                │  ← sqlparser-rs; no changes to SQL syntax
└─────────────────┬────────────────────┘
                  │ AST
┌─────────────────▼────────────────────┐
│      Sous Chef  (Query Planner)      │  ← rule-based plan generation
└─────────────────┬────────────────────┘
                  │ execution plan
┌─────────────────▼────────────────────┐
│         The Stove  (Executor)        │  ← walks + executes the plan
└─────────────────┬────────────────────┘
          ┌───────┴────────┐
┌─────────▼──────┐  ┌──────▼──────────────┐
│  Warming Tray  │  │  Strainer  (B-tree)  │
│  (Page Cache)  │  │  Index               │
└─────────┬──────┘  └─────────────────────┘
          │
┌─────────▼──────────────────────────┐
│    The Stove — Storage Layer       │
│    .pot files, Plate (page) I/O    │
│    Prep Log (WAL)                  │
└────────────────────────────────────┘
```

---

## Companion Documents

| Document | What it covers |
|---|---|
| [type-system.md](../../type-system.md) | `INTEGER`, `TEXT`, `REAL`, `BLOB` — storage encoding, NULL (Vuoto) semantics, Al Dente Key |
| [storage-format.md](../../storage-format.md) | `.pot` file layout, 4KB page header, heap slotted-page format, overflow pages, catalog |
| [wal-format.md](../../wal-format.md) | Prep Log record layout, CRC32, checkpointing, crash recovery algorithm |
| [btree-format.md](../../btree-format.md) | Strainer B-tree node layout, insert/lookup/range-scan/delete operations, stats |
| [sql-semantics.md](../../sql-semantics.md) | Supported statements, NULL semantics, WHERE operators, JOIN strategy, aggregates |
| [transaction-model.md](../../transaction-model.md) | Simmer lifecycle, serializable isolation, locking protocol, dirty page ownership |
| [error-taxonomy.md](../../error-taxonomy.md) | Scottatura codes by category, REPL display format, Rust `thiserror` enum structure |
| [testing-strategy.md](../../testing-strategy.md) | Unit test coverage per module, SQL integration test harness, crash recovery test |

---

## Module Public APIs (Rust)

### `lib.rs` — Programmatic Client

```rust
pub struct Pot { ... }

impl Pot {
    pub fn open(path: &str) -> Result<Self, Scottatura>;
    pub fn execute(&self, sql: &str) -> Result<ResultSet, Scottatura>;
    pub fn noodles(&self) -> Vec<String>;
    pub fn recipe(&self, noodle: &str) -> Result<Recipe, Scottatura>;
    pub fn strainer_stats(&self) -> Vec<StrainerStats>;
    pub fn simmer(&mut self) -> Result<Simmer, Scottatura>;
}

pub struct Simmer<'a> { ... }

impl<'a> Simmer<'a> {
    pub fn execute(&mut self, sql: &str) -> Result<ResultSet, Scottatura>;
    pub fn finish(self) -> Result<(), Scottatura>;   // COMMIT
    pub fn abandon(self) -> Result<(), Scottatura>;  // ROLLBACK
}

pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub affected_rows: u64,
    pub last_insert_key: Option<i64>,  // set after INSERT; None for all other statements
}

pub enum Value {
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
    Null,
}
```

### `sous_chef/` — Query Planner

```rust
pub fn plan(ast: Statement, catalog: &Catalog) -> Result<Plan, Scottatura>;

pub enum Plan {
    SeqScan { noodle: String, filter: Option<Expr> },
    IndexScan { noodle: String, index: String, key: Value, filter: Option<Expr> },
    HashJoin { outer: Box<Plan>, inner: Box<Plan>, on: (String, String) },
    IndexNestedLoopJoin { outer: Box<Plan>, inner_index: String, on: (String, String) },
    Insert { noodle: String, cols: Vec<String>, values: Vec<Vec<Expr>> },
    Update { noodle: String, assignments: Vec<(String, Expr)>, filter: Expr },
    Delete { noodle: String, filter: Expr },
    CreateTable { ... },
    DropTable { ... },
    CreateIndex { ... },
    DropIndex { ... },
}
```

### `warming_tray/` — Page Cache

```rust
pub struct WarmingTray { ... }

impl WarmingTray {
    pub fn new(capacity_pages: usize) -> Self;
    pub fn fetch(&mut self, page_num: u64, file: &mut File) -> Result<&mut Page, Scottatura>;
    pub fn mark_dirty(&mut self, page_num: u64, simmer_id: u64);
    pub fn flush_simmer(&mut self, simmer_id: u64, file: &mut File) -> Result<(), Scottatura>;
    pub fn evict_simmer(&mut self, simmer_id: u64);
    pub fn stats(&self) -> CacheStats;
}
```

---

## Repo Structure

```
marinaradb/
├── src/
│   ├── main.rs             # CLI entry point / REPL (clap + rustyline)
│   ├── lib.rs              # Pot, Simmer, ResultSet, Value, Scottatura
│   ├── parser/             # sqlparser-rs wrapper + AST normalization
│   ├── sous_chef/          # query planner (plan() function + Plan enum)
│   ├── stove/              # executor (walks Plan) + storage engine (page I/O)
│   ├── warming_tray/       # page cache / buffer pool
│   ├── strainer/           # B-tree index
│   └── prep_log/           # write-ahead log
├── docs/
│   ├── type-system.md
│   ├── storage-format.md
│   ├── wal-format.md
│   ├── btree-format.md
│   ├── sql-semantics.md
│   ├── transaction-model.md
│   ├── error-taxonomy.md
│   └── testing-strategy.md
├── tests/
│   ├── harness.rs
│   └── *.sql / *.expected
├── examples/               # sample .sql scripts
└── Cargo.toml
```

---

## v1 Feature Scope

**In scope:**
- `CREATE TABLE` / `DROP TABLE` / `CREATE INDEX` / `DROP INDEX`
- `SELECT`, `INSERT`, `UPDATE`, `DELETE`
- `WHERE` with: `=`, `!=`, `<`, `>`, `<=`, `>=`, `AND`, `OR`, `NOT`, `IS NULL`, `IS NOT NULL`, `BETWEEN`, `LIKE`, `IN (...)`
- `INNER JOIN` (single equi-join per query)
- `ORDER BY` (multi-column), `LIMIT`, `OFFSET`
- `COUNT`, `SUM`, `MIN`, `MAX`, `AVG` with optional `GROUP BY`
- Auto-increment Al Dente Keys
- Single-column Strainer indexes (B-tree)
- `BEGIN` / `COMMIT` / `ROLLBACK` with Serializable isolation
- REPL + `.sql` script file execution
- Programmatic Rust client API

**Out of scope for v1:**
- Subqueries, CTEs, window functions
- Multi-column indexes
- `LEFT` / `RIGHT` / `FULL` joins
- `HAVING`
- Views, triggers, stored procedures
- Networked / multi-client access
- `VACUUM`

---

## Stack

| Concern | Choice |
|---|---|
| Language | Rust 2021 edition |
| SQL parsing | `sqlparser-rs` |
| CLI / REPL | `clap` + `rustyline` |
| Binary I/O | `std::fs`, `memmap2` |
| Serialization | `bincode` (for catalog ColumnDef blobs) |
| Error handling | `thiserror` + `anyhow` |
| Testing | `cargo test` + `tempfile` |
| Packaging | `cargo` + `Cargo.toml` |
