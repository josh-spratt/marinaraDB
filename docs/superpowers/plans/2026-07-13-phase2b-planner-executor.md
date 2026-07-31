# Phase 2b: Sous Chef Planner + Stove Executor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `Pot::execute(sauql: &str) -> Result<ResultSet, Scottatura>`, wiring the Phase 2a parser through a new Sous Chef planner and Stove executor so single-table `SERVE`, `LADLE INTO`, `SEASON`, and `DISCARD FROM` (with `STRAINED BY`) run end-to-end against a real `.pot` file. Once that's in place, replace the `main.rs` stub with an interactive REPL (Task 9) so the database is usable outside of Rust — a `psql`/`sqlite3`-style terminal session, not just a library API.

**Architecture:** `parser::parse_statement` (existing) produces an `ast::Statement`; `sous_chef::plan()` translates it into a `Plan` (rejecting clauses deferred to Phase 2c/3); `stove::executor::execute()` walks the `Plan` against a `&mut Pot`, using a small three-valued (`Tri`) boolean evaluator for `STRAINED BY` and a scalar evaluator for computed values. The heap gains real multi-page traversal (`next_page` chaining) since a table's rows are no longer guaranteed to fit one page.

**Tech Stack:** Rust, existing crates only (`thiserror`, `serde`, `bincode`, `crc32fast`, `tempfile` for tests). No new dependencies.

## Global Constraints

- Follow the existing pattern: every fallible operation returns `Result<T, Scottatura>`.
- Every new `Scottatura` variant must have a code from `docs/error-taxonomy.md`; add new codes there in the same task that introduces the variant.
- `cargo build` and `cargo clippy --all-targets -- -D warnings` must stay clean after every task.
- No `todo!()`/`unimplemented!()`. Deferred features fail with a specific, documented `Scottatura` variant, never a panic or silent no-op.
- Identifiers are already normalized to lowercase by the parser; executor code does not need to re-normalize case.

---

### Task 1: New Scottatura error variants for execution-time gaps

**Files:**
- Modify: `src/lib.rs` (the `Scottatura` enum, currently ends at `StatementNotSupported`)
- Modify: `docs/error-taxonomy.md`

**Interfaces:**
- Produces: `Scottatura::AlDenteKeyNotUpdatable`, `Scottatura::ClauseNotYetSupported { clause: String }`, `Scottatura::TransactionNotSupported`, `Scottatura::StrainerNotSupported` — used by Tasks 3 and 5.

- [ ] **Step 1: Add the four variants to the `Scottatura` enum**

In `src/lib.rs`, add after the final `StatementNotSupported` variant:

```rust
    #[error("Scottatura [RICETTA_005]: Cannot update the Al Dente Key (`id`) column.")]
    AlDenteKeyNotUpdatable,

    #[error("Scottatura [EXEC_001]: `{clause}` is not supported yet — planned for a later phase.")]
    ClauseNotYetSupported { clause: String },

    #[error("Scottatura [EXEC_002]: Simmer transactions (BEGIN/FINISH/SPILL) are not executable yet; only single-statement autocommit is supported.")]
    TransactionNotSupported,

    #[error("Scottatura [EXEC_003]: Strainer indexes (STRAIN / REMOVE STRAINER) are not supported yet.")]
    StrainerNotSupported,
```

- [ ] **Step 2: Document the new codes in the taxonomy**

In `docs/error-taxonomy.md`, add a row to the `RICETTA_` table:

```markdown
| `RICETTA_005` | Cannot update the Al Dente Key (`id`) column. |
```

(This row already exists in the table — confirm it matches the message above exactly; it does, so no change needed there.)

Add a new category section after `## Lock Errors (LOCK_)`:

```markdown
## Execution Errors (EXEC_)

| Code | Message |
|---|---|
| `EXEC_001` | `{clause}` is not supported yet — planned for a later phase. |
| `EXEC_002` | Simmer transactions (BEGIN/FINISH/SPILL) are not executable yet; only single-statement autocommit is supported. |
| `EXEC_003` | Strainer indexes (STRAIN / REMOVE STRAINER) are not supported yet. |
```

Also add `EXEC_` to the `## Error Categories` table near the top:

```markdown
| Execution | `EXEC_` | Statement is parsed correctly but not yet executable (deferred feature) |
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: no errors (new enum variants are unused so far, which is fine — no warnings for unused enum variants in Rust).

- [ ] **Step 4: Commit**

```bash
git add src/lib.rs docs/error-taxonomy.md
git commit -m "feat: add EXEC_ error variants for deferred Phase 2b features"
```

---

### Task 2: Multi-page heap chain traversal in Pot

**Files:**
- Modify: `src/stove/pot.rs`
- Test: `tests/heap_chain.rs` (new)

**Interfaces:**
- Consumes: `stove::heap::{encode_row, heap_insert, heap_scan, heap_delete}` (existing, unchanged), `stove::page::PageType` (existing), `stove::catalog::NoodleEntry` (existing, has `root_heap_page: u64`, `cols: Vec<ColumnDef>`).
- Produces (new `Pot` methods, all `pub(crate)` except where noted):
  - `pub fn insert_strand(&mut self, noodle: &str, values: Vec<Value>) -> Result<i64, Scottatura>` (existing signature, reimplemented to chain across pages)
  - `pub fn scan_noodle(&mut self, noodle: &str) -> Result<Vec<(i64, Vec<Value>)>, Scottatura>` (existing signature, reimplemented to chain across pages)
  - `pub(crate) fn insert_with_key(&mut self, noodle: &str, key: i64, values: &[Value]) -> Result<(), Scottatura>` — new, used by Task 5's `SEASON` handling to re-insert a row under its existing Al Dente Key.
  - `pub(crate) fn delete_by_key(&mut self, noodle: &str, key: i64) -> Result<bool, Scottatura>` — new, used by Task 5's `SEASON`/`DISCARD` handling.

- [ ] **Step 1: Write the failing multi-page test**

Create `tests/heap_chain.rs`:

```rust
use marinaradb::{ColType, ColumnDef, Pot};

fn temp_path() -> String {
    let f = tempfile::NamedTempFile::new().unwrap();
    let p = f.path().to_str().unwrap().to_string();
    drop(f);
    p
}

#[test]
fn test_insert_beyond_one_page_spans_multiple_pages() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.create_table("nums", vec![
        ColumnDef { name: "n".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
    ]).unwrap();

    // Each row is ~17 bytes + an 8-byte slot; a 4KB page holds well under 300.
    // 500 rows guarantees at least one page split.
    for i in 0..500 {
        pot.insert_strand("nums", vec![marinaradb::Value::Integer(i)]).unwrap();
    }

    let rows = pot.scan_noodle("nums").unwrap();
    assert_eq!(rows.len(), 500);
    assert_eq!(rows[0].0, 1);
    assert_eq!(rows[499].0, 500);
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_delete_and_reinsert_across_chain() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.create_table("nums", vec![
        ColumnDef { name: "n".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
    ]).unwrap();
    for i in 0..500 {
        pot.insert_strand("nums", vec![marinaradb::Value::Integer(i)]).unwrap();
    }
    // Key 500 (the 500th insert, i.e. last_insert_key of the loop's last call) lives on a later page.
    assert!(pot.delete_by_key("nums", 500).unwrap());
    assert!(!pot.delete_by_key("nums", 500).unwrap()); // already gone

    pot.insert_with_key("nums", 500, &[marinaradb::Value::Integer(999)]).unwrap();
    let rows = pot.scan_noodle("nums").unwrap();
    let reinserted = rows.iter().find(|(k, _)| *k == 500).unwrap();
    assert_eq!(reinserted.1[0], marinaradb::Value::Integer(999));
    std::fs::remove_file(&path).ok();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test heap_chain`
Expected: FAIL — `insert_strand` currently returns an `IoError` once the first page fills (see `src/stove/pot.rs:74-79`), and `insert_with_key`/`delete_by_key` don't exist yet (compile error). Confirm the compile error names both missing methods.

- [ ] **Step 3: Add chain-walking helpers and rewrite `insert_strand`/`scan_noodle`**

Replace the body of `src/stove/pot.rs` from the `use` block through the end of `scan_noodle` with:

```rust
use crate::{ColumnDef, Recipe, Scottatura, Value};
use crate::stove::catalog::{Catalog, NoodleEntry};
use crate::stove::heap::{encode_row, heap_delete, heap_insert, heap_scan};
use crate::stove::page::PageType;
use crate::stove::pot_file::PotFile;
use crate::warming_tray::WarmingTray;

pub struct Pot {
    pot_file: PotFile,
    tray: WarmingTray,
    catalog: Catalog,
    next_key: i64,
}

fn encode_and_validate(entry: &NoodleEntry, key: i64, values: &[Value]) -> Result<Vec<u8>, Scottatura> {
    for (val, col) in values.iter().zip(entry.cols.iter()) {
        if matches!(val, Value::Null) && !col.nullable {
            return Err(Scottatura::NullConstraintViolation { col: col.name.clone() });
        }
    }
    encode_row(key, values, &entry.cols)
}

impl Pot {
    pub fn open(path: &str) -> Result<Self, Scottatura> {
        let (pot_file, tray, catalog) = if std::path::Path::new(path).exists() {
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

    fn entry(&self, noodle: &str) -> Result<NoodleEntry, Scottatura> {
        self.catalog.get_recipe(noodle)
            .cloned()
            .ok_or_else(|| Scottatura::NoodleNotFound { name: noodle.to_string() })
    }

    pub fn insert_strand(&mut self, noodle: &str, values: Vec<Value>) -> Result<i64, Scottatura> {
        let entry = self.entry(noodle)?;

        if values.len() != entry.cols.len() {
            return Err(Scottatura::ValueCountMismatch { got: values.len(), expected: entry.cols.len() });
        }

        let key = self.next_key;
        let row_bytes = encode_and_validate(&entry, key, &values)?;
        self.heap_chain_insert(entry.root_heap_page, &row_bytes)?;
        self.next_key += 1;
        Ok(key)
    }

    pub(crate) fn insert_with_key(&mut self, noodle: &str, key: i64, values: &[Value]) -> Result<(), Scottatura> {
        let entry = self.entry(noodle)?;
        let row_bytes = encode_and_validate(&entry, key, values)?;
        self.heap_chain_insert(entry.root_heap_page, &row_bytes)
    }

    pub fn scan_noodle(&mut self, noodle: &str) -> Result<Vec<(i64, Vec<Value>)>, Scottatura> {
        let entry = self.entry(noodle)?;
        self.heap_chain_scan(entry.root_heap_page, &entry.cols)
    }

    pub(crate) fn delete_by_key(&mut self, noodle: &str, key: i64) -> Result<bool, Scottatura> {
        let entry = self.entry(noodle)?;
        self.heap_chain_delete(entry.root_heap_page, key, &entry.cols)
    }

    fn heap_chain_insert(&mut self, root_page: u64, row_bytes: &[u8]) -> Result<(), Scottatura> {
        let mut page_num = root_page;
        loop {
            let next_page_num = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                if heap_insert(page, row_bytes).is_some() {
                    self.tray.mark_dirty(page_num, 0);
                    self.tray.flush_simmer(0, &mut self.pot_file)?;
                    return Ok(());
                }
                page.next_page()
            };
            if next_page_num != 0 {
                page_num = next_page_num;
                continue;
            }
            let new_page = self.pot_file.allocate_page(PageType::Heap)?;
            let new_page_num = new_page.number;
            let page = self.tray.fetch(page_num, &mut self.pot_file)?;
            page.set_next_page(new_page_num);
            self.tray.mark_dirty(page_num, 0);
            self.tray.flush_simmer(0, &mut self.pot_file)?;
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
            let (deleted, next) = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                let deleted = heap_delete(page, key, cols);
                (deleted, page.next_page())
            };
            if deleted {
                self.tray.mark_dirty(page_num, 0);
                self.tray.flush_simmer(0, &mut self.pot_file)?;
                return Ok(true);
            }
            if next == 0 { return Ok(false); }
            page_num = next;
        }
    }
}
```

Note: `insert_strand`'s length check now uses `Scottatura::ValueCountMismatch` (the correct RICETTA_009 variant) instead of the old `TypeMismatch` misuse — this is a pre-existing bug fix that's a one-line side effect of the rewrite, not a separate task.

- [ ] **Step 4: Run the new test**

Run: `cargo test --test heap_chain`
Expected: both tests PASS.

- [ ] **Step 5: Run the full suite**

Run: `cargo test`
Expected: all existing tests (`tests/foundation.rs`, `tests/parser.rs`, unit tests) still PASS — `insert_strand`/`scan_noodle` keep their exact signatures and behavior for single-page cases.

- [ ] **Step 6: `cargo build` and `cargo clippy`**

Run: `cargo build && cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/stove/pot.rs tests/heap_chain.rs
git commit -m "feat: multi-page heap chain traversal; insert_with_key, delete_by_key"
```

---

### Task 3: Sous Chef Plan type and planner

**Files:**
- Create: `src/sous_chef/mod.rs` (currently a one-line placeholder comment)
- Test: inline `#[cfg(test)]` module in the same file

**Interfaces:**
- Consumes: `parser::ast::{Statement, SelectStatement, SelectItem, Expr, ColumnSpec}` (existing), `Scottatura::{ClauseNotYetSupported, TransactionNotSupported, StrainerNotSupported}` (Task 1).
- Produces: `pub enum Plan { .. }`, `pub fn plan(stmt: Statement) -> Result<Plan, Scottatura>` — consumed by Task 5.

- [ ] **Step 1: Write the `Plan` enum and `plan()` function with its rejection tests**

Replace the contents of `src/sous_chef/mod.rs` with:

```rust
use crate::parser::ast::{ColumnSpec, Expr, SelectItem, Statement};
use crate::Scottatura;

#[derive(Debug, Clone)]
pub enum Plan {
    SeqScan { table: String, columns: Vec<SelectItem>, filter: Option<Expr> },
    Insert { table: String, columns: Vec<String>, rows: Vec<Vec<Expr>> },
    Update { table: String, assignments: Vec<(String, Expr)>, filter: Expr },
    Delete { table: String, filter: Expr },
    CreateTable { name: String, columns: Vec<ColumnSpec> },
    DropTable { name: String },
}

pub fn plan(stmt: Statement) -> Result<Plan, Scottatura> {
    match stmt {
        Statement::Select(sel) => {
            if sel.join.is_some() {
                return Err(Scottatura::ClauseNotYetSupported { clause: "TOSS (join)".to_string() });
            }
            if !sel.order_by.is_empty() {
                return Err(Scottatura::ClauseNotYetSupported { clause: "ARRANGED BY".to_string() });
            }
            if sel.limit.is_some() || sel.offset.is_some() {
                return Err(Scottatura::ClauseNotYetSupported { clause: "ONLY / SKIP".to_string() });
            }
            if !sel.group_by.is_empty() {
                return Err(Scottatura::ClauseNotYetSupported { clause: "BUNDLED BY".to_string() });
            }
            Ok(Plan::SeqScan { table: sel.from.name, columns: sel.items, filter: sel.filter })
        }
        Statement::Insert { table, columns, rows } => Ok(Plan::Insert { table, columns, rows }),
        Statement::Update { table, assignments, filter } => Ok(Plan::Update { table, assignments, filter }),
        Statement::Delete { table, filter } => Ok(Plan::Delete { table, filter }),
        Statement::CreateTable { name, columns } => Ok(Plan::CreateTable { name, columns }),
        Statement::DropTable { name } => Ok(Plan::DropTable { name }),
        Statement::CreateIndex { .. } | Statement::DropIndex { .. } => Err(Scottatura::StrainerNotSupported),
        Statement::BeginSimmer | Statement::FinishSimmer | Statement::SpillSimmer => {
            Err(Scottatura::TransactionNotSupported)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_statement;

    #[test]
    fn test_select_becomes_seqscan() {
        let stmt = parse_statement("SERVE * FROM users STRAINED BY age > 18;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::SeqScan { table, .. } if table == "users"));
    }

    #[test]
    fn test_select_without_filter() {
        let stmt = parse_statement("SERVE * FROM users;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::SeqScan { filter: None, .. }));
    }

    #[test]
    fn test_join_rejected() {
        let stmt = parse_statement("SERVE * FROM users u TOSS orders o ON u.id = o.user_id;").unwrap();
        let err = plan(stmt).unwrap_err();
        assert!(matches!(err, Scottatura::ClauseNotYetSupported { clause } if clause == "TOSS (join)"));
    }

    #[test]
    fn test_order_by_rejected() {
        let stmt = parse_statement("SERVE * FROM users ARRANGED BY age;").unwrap();
        let err = plan(stmt).unwrap_err();
        assert!(matches!(err, Scottatura::ClauseNotYetSupported { clause } if clause == "ARRANGED BY"));
    }

    #[test]
    fn test_limit_rejected() {
        let stmt = parse_statement("SERVE * FROM users ONLY 10;").unwrap();
        let err = plan(stmt).unwrap_err();
        assert!(matches!(err, Scottatura::ClauseNotYetSupported { clause } if clause == "ONLY / SKIP"));
    }

    #[test]
    fn test_insert_passthrough() {
        let stmt = parse_statement("LADLE INTO users (name) PORTIONS ('Tony');").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Insert { table, columns, .. } if table == "users" && columns == vec!["name".to_string()]));
    }

    #[test]
    fn test_update_passthrough() {
        let stmt = parse_statement("SEASON users SET age = 39 STRAINED BY name = 'Tony';").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Update { table, .. } if table == "users"));
    }

    #[test]
    fn test_delete_passthrough() {
        let stmt = parse_statement("DISCARD FROM users STRAINED BY age < 18;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Delete { table, .. } if table == "users"));
    }

    #[test]
    fn test_create_table_passthrough() {
        let stmt = parse_statement("PLATE noodle t (name TEXT);").unwrap();
        assert!(matches!(plan(stmt).unwrap(), Plan::CreateTable { name, .. } if name == "t"));
    }

    #[test]
    fn test_drop_table_passthrough() {
        let stmt = parse_statement("TOSS OUT noodle t;").unwrap();
        assert!(matches!(plan(stmt).unwrap(), Plan::DropTable { name } if name == "t"));
    }

    #[test]
    fn test_create_index_rejected() {
        let stmt = parse_statement("STRAIN idx1 ON users (age);").unwrap();
        assert!(matches!(plan(stmt).unwrap_err(), Scottatura::StrainerNotSupported));
    }

    #[test]
    fn test_drop_index_rejected() {
        let stmt = parse_statement("REMOVE STRAINER idx1;").unwrap();
        assert!(matches!(plan(stmt).unwrap_err(), Scottatura::StrainerNotSupported));
    }

    #[test]
    fn test_begin_simmer_rejected() {
        let stmt = parse_statement("BEGIN SIMMER;").unwrap();
        assert!(matches!(plan(stmt).unwrap_err(), Scottatura::TransactionNotSupported));
    }
}
```

- [ ] **Step 2: Run test to verify it fails first, then passes**

Run: `cargo test --lib sous_chef:: -- --nocapture`
Expected: since this is the first real content in the module, there's no "before" state to fail against — just confirm all tests PASS after this step. If any fail, check the `ast::Statement`/`SelectStatement` field names against `src/parser/ast.rs` (they must match exactly: `sel.from.name`, `sel.items`, `sel.filter`, `sel.join`, `sel.order_by`, `sel.limit`, `sel.offset`, `sel.group_by`).

- [ ] **Step 3: Run `cargo build` and `cargo clippy`**

Run: `cargo build && cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/sous_chef/mod.rs
git commit -m "feat: Sous Chef Plan type and planner with deferred-clause rejection"
```

---

### Task 4: Scalar and three-valued boolean expression evaluator

**Files:**
- Create: `src/stove/executor.rs`
- Modify: `src/stove/mod.rs` (add `pub mod executor;`)

**Interfaces:**
- Consumes: `parser::ast::{Expr, BinOp}` (existing), `ColumnDef`, `Value`, `Scottatura` (existing).
- Produces (consumed by Task 5):
  - `pub(crate) fn full_schema(cols: &[ColumnDef]) -> Vec<ColumnDef>`
  - `pub(crate) fn full_row(key: i64, values: &[Value]) -> Vec<Value>`
  - `pub(crate) fn eval_scalar(expr: &Expr, row: &[Value], schema: &[ColumnDef]) -> Result<Value, Scottatura>`
  - `pub(crate) enum Tri { True, False, Unknown }` with `pub(crate) fn is_true(&self) -> bool`
  - `pub(crate) fn eval_tri(expr: &Expr, row: &[Value], schema: &[ColumnDef]) -> Result<Tri, Scottatura>`

- [ ] **Step 1: Write the evaluator with unit tests**

Create `src/stove/executor.rs`:

```rust
use std::cmp::Ordering;
use crate::parser::ast::{BinOp, Expr};
use crate::{ColType, ColumnDef, Scottatura, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tri {
    True,
    False,
    Unknown,
}

impl Tri {
    pub(crate) fn is_true(&self) -> bool {
        matches!(self, Tri::True)
    }

    fn from_bool(b: bool) -> Tri {
        if b { Tri::True } else { Tri::False }
    }

    fn not(&self) -> Tri {
        match self {
            Tri::True => Tri::False,
            Tri::False => Tri::True,
            Tri::Unknown => Tri::Unknown,
        }
    }

    fn and(&self, other: Tri) -> Tri {
        match (self, other) {
            (Tri::False, _) | (_, Tri::False) => Tri::False,
            (Tri::True, Tri::True) => Tri::True,
            _ => Tri::Unknown,
        }
    }

    fn or(&self, other: Tri) -> Tri {
        match (self, other) {
            (Tri::True, _) | (_, Tri::True) => Tri::True,
            (Tri::False, Tri::False) => Tri::False,
            _ => Tri::Unknown,
        }
    }
}

pub(crate) fn full_schema(cols: &[ColumnDef]) -> Vec<ColumnDef> {
    let mut out = vec![ColumnDef {
        name: "id".to_string(),
        col_type: ColType::Integer,
        nullable: false,
        strainer_root: None,
    }];
    out.extend(cols.iter().cloned());
    out
}

pub(crate) fn full_row(key: i64, values: &[Value]) -> Vec<Value> {
    let mut out = vec![Value::Integer(key)];
    out.extend(values.iter().cloned());
    out
}

fn column_value(name: &str, row: &[Value], schema: &[ColumnDef]) -> Result<Value, Scottatura> {
    schema.iter().position(|c| c.name == name)
        .map(|i| row[i].clone())
        .ok_or_else(|| Scottatura::ColumnNotFound { col: name.to_string(), name: "?".to_string() })
}

fn values_equal(l: &Value, r: &Value) -> Result<bool, Scottatura> {
    match (l, r) {
        (Value::Integer(a), Value::Integer(b)) => Ok(a == b),
        (Value::Real(a), Value::Real(b)) => Ok(a == b),
        (Value::Integer(a), Value::Real(b)) => Ok(*a as f64 == *b),
        (Value::Real(a), Value::Integer(b)) => Ok(*a == *b as f64),
        (Value::Text(a), Value::Text(b)) => Ok(a == b),
        (Value::Blob(a), Value::Blob(b)) => Ok(a == b),
        (l, r) => Err(Scottatura::TypeMismatch {
            got: format!("{r:?}"), col: "<expr>".to_string(), expected: format!("{l:?}"),
        }),
    }
}

fn compare_values(l: &Value, r: &Value) -> Result<Ordering, Scottatura> {
    match (l, r) {
        (Value::Integer(a), Value::Integer(b)) => Ok(a.cmp(b)),
        (Value::Real(a), Value::Real(b)) => a.partial_cmp(b).ok_or_else(|| Scottatura::TypeMismatch {
            got: "NaN".to_string(), col: "<expr>".to_string(), expected: "orderable REAL".to_string(),
        }),
        (Value::Integer(a), Value::Real(b)) => (*a as f64).partial_cmp(b).ok_or_else(|| Scottatura::TypeMismatch {
            got: "NaN".to_string(), col: "<expr>".to_string(), expected: "orderable REAL".to_string(),
        }),
        (Value::Real(a), Value::Integer(b)) => a.partial_cmp(&(*b as f64)).ok_or_else(|| Scottatura::TypeMismatch {
            got: "NaN".to_string(), col: "<expr>".to_string(), expected: "orderable REAL".to_string(),
        }),
        (Value::Text(a), Value::Text(b)) => Ok(a.cmp(b)),
        (l, r) => Err(Scottatura::TypeMismatch {
            got: format!("{r:?}"), col: "<expr>".to_string(), expected: format!("orderable value matching {l:?}"),
        }),
    }
}

fn like_match(text: &str, pattern: &str) -> bool {
    fn rec(t: &[u8], p: &[u8]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some(b'%') => rec(t, &p[1..]) || (!t.is_empty() && rec(&t[1..], p)),
            Some(b'_') => !t.is_empty() && rec(&t[1..], &p[1..]),
            Some(c) => !t.is_empty() && t[0] == *c && rec(&t[1..], &p[1..]),
        }
    }
    rec(text.as_bytes(), pattern.as_bytes())
}

fn eval_arith(op: &BinOp, l: Value, r: Value) -> Result<Value, Scottatura> {
    if matches!(l, Value::Null) || matches!(r, Value::Null) {
        return Ok(Value::Null);
    }
    let (a, b): (f64, f64) = match (&l, &r) {
        (Value::Integer(a), Value::Integer(b)) => {
            let (a, b) = (*a, *b);
            return Ok(match op {
                BinOp::Add => Value::Integer(a + b),
                BinOp::Sub => Value::Integer(a - b),
                BinOp::Mul => Value::Integer(a * b),
                BinOp::Div => {
                    if b == 0 { Value::Null } else { Value::Integer(a / b) }
                }
                _ => unreachable!("eval_arith only called with arithmetic ops"),
            });
        }
        (Value::Integer(a), Value::Real(b)) => (*a as f64, *b),
        (Value::Real(a), Value::Integer(b)) => (*a, *b as f64),
        (Value::Real(a), Value::Real(b)) => (*a, *b),
        _ => return Err(Scottatura::TypeMismatch {
            got: format!("{r:?}"), col: "<expr>".to_string(), expected: "INTEGER or REAL".to_string(),
        }),
    };
    Ok(match op {
        BinOp::Add => Value::Real(a + b),
        BinOp::Sub => Value::Real(a - b),
        BinOp::Mul => Value::Real(a * b),
        BinOp::Div => if b == 0.0 { Value::Null } else { Value::Real(a / b) },
        _ => unreachable!("eval_arith only called with arithmetic ops"),
    })
}

pub(crate) fn eval_scalar(expr: &Expr, row: &[Value], schema: &[ColumnDef]) -> Result<Value, Scottatura> {
    match expr {
        Expr::Literal(v) => Ok(v.clone()),
        Expr::Column { name, .. } => column_value(name, row, schema),
        Expr::BinaryOp { op: op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div), left, right } => {
            let l = eval_scalar(left, row, schema)?;
            let r = eval_scalar(right, row, schema)?;
            eval_arith(op, l, r)
        }
        Expr::FunctionCall { .. } => Err(Scottatura::ClauseNotYetSupported { clause: "aggregate functions".to_string() }),
        _ => Err(Scottatura::ClauseNotYetSupported { clause: "boolean expression in value position".to_string() }),
    }
}

pub(crate) fn eval_tri(expr: &Expr, row: &[Value], schema: &[ColumnDef]) -> Result<Tri, Scottatura> {
    match expr {
        Expr::BinaryOp { op: BinOp::And, left, right } => {
            Ok(eval_tri(left, row, schema)?.and(eval_tri(right, row, schema)?))
        }
        Expr::BinaryOp { op: BinOp::Or, left, right } => {
            Ok(eval_tri(left, row, schema)?.or(eval_tri(right, row, schema)?))
        }
        Expr::BinaryOp { op: cmp_op @ (BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte), left, right } => {
            let l = eval_scalar(left, row, schema)?;
            let r = eval_scalar(right, row, schema)?;
            if matches!(l, Value::Null) || matches!(r, Value::Null) {
                return Ok(Tri::Unknown);
            }
            let result = match cmp_op {
                BinOp::Eq => values_equal(&l, &r)?,
                BinOp::Neq => !values_equal(&l, &r)?,
                BinOp::Lt => compare_values(&l, &r)? == Ordering::Less,
                BinOp::Gt => compare_values(&l, &r)? == Ordering::Greater,
                BinOp::Lte => compare_values(&l, &r)? != Ordering::Greater,
                BinOp::Gte => compare_values(&l, &r)? != Ordering::Less,
                _ => unreachable!(),
            };
            Ok(Tri::from_bool(result))
        }
        Expr::UnaryNot(inner) => Ok(eval_tri(inner, row, schema)?.not()),
        Expr::IsNull { expr, negated } => {
            let is_null = matches!(eval_scalar(expr, row, schema)?, Value::Null);
            Ok(Tri::from_bool(is_null != *negated))
        }
        Expr::Between { expr, low, high } => {
            let v = eval_scalar(expr, row, schema)?;
            let lo = eval_scalar(low, row, schema)?;
            let hi = eval_scalar(high, row, schema)?;
            if matches!(v, Value::Null) || matches!(lo, Value::Null) || matches!(hi, Value::Null) {
                return Ok(Tri::Unknown);
            }
            let ge_lo = compare_values(&v, &lo)? != Ordering::Less;
            let le_hi = compare_values(&v, &hi)? != Ordering::Greater;
            Ok(Tri::from_bool(ge_lo && le_hi))
        }
        Expr::Like { expr, pattern } => {
            let v = eval_scalar(expr, row, schema)?;
            let p = eval_scalar(pattern, row, schema)?;
            if matches!(v, Value::Null) || matches!(p, Value::Null) {
                return Ok(Tri::Unknown);
            }
            match (v, p) {
                (Value::Text(t), Value::Text(pat)) => Ok(Tri::from_bool(like_match(&t, &pat))),
                (got, _) => Err(Scottatura::TypeMismatch {
                    got: format!("{got:?}"), col: "<expr>".to_string(), expected: "TEXT".to_string(),
                }),
            }
        }
        Expr::In { expr, list } => {
            let v = eval_scalar(expr, row, schema)?;
            if matches!(v, Value::Null) {
                return Ok(Tri::Unknown);
            }
            for item in list {
                let item_val = eval_scalar(item, row, schema)?;
                if !matches!(item_val, Value::Null) && values_equal(&v, &item_val)? {
                    return Ok(Tri::True);
                }
            }
            Ok(Tri::False)
        }
        _ => Err(Scottatura::ClauseNotYetSupported { clause: "non-boolean expression in filter position".to_string() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_statement;
    use crate::parser::ast::Statement;

    fn schema() -> Vec<ColumnDef> {
        vec![
            ColumnDef { name: "id".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
            ColumnDef { name: "age".into(), col_type: ColType::Integer, nullable: true, strainer_root: None },
        ]
    }

    fn filter_of(sql: &str) -> Expr {
        match parse_statement(sql).unwrap() {
            Statement::Select(s) => s.filter.unwrap(),
            _ => panic!("expected SERVE"),
        }
    }

    #[test]
    fn test_eq_true() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY age = 38;");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_vuoto_equality_is_unknown() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Null];
        let f = filter_of("SERVE * FROM t STRAINED BY age = 38;");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::Unknown);
    }

    #[test]
    fn test_is_vuoto() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Null];
        let f = filter_of("SERVE * FROM t STRAINED BY age IS VUOTO;");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_and_short_circuits_to_false() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Null];
        let f = filter_of("SERVE * FROM t STRAINED BY age = 38 AND name = 'nobody';");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::False);
    }

    #[test]
    fn test_between_inclusive() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY age BETWEEN 38 AND 40;");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_like_wildcards() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY name LIKE 'T_n%';");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_in_list() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY age IN (1, 2, 38);");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_integer_promotes_to_real_in_comparison() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY age > 37.5;");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_arithmetic_with_vuoto_is_vuoto() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Null];
        let expr = Expr::BinaryOp {
            op: BinOp::Add,
            left: Box::new(Expr::Column { qualifier: None, name: "age".to_string() }),
            right: Box::new(Expr::Literal(Value::Integer(1))),
        };
        assert_eq!(eval_scalar(&expr, &row, &schema()).unwrap(), Value::Null);
    }

    #[test]
    fn test_text_ordering_is_lexicographic() {
        let row = vec![Value::Integer(1), Value::Text("banana".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY name > 'apple';");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }
}
```

- [ ] **Step 2: Register the module**

In `src/stove/mod.rs`, add:

```rust
pub mod executor;
```

so the file reads:

```rust
pub mod catalog;
pub mod encoding;
pub mod executor;
pub mod heap;
pub mod page;
pub mod pot;
pub mod pot_file;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --lib executor:: -- --nocapture`
Expected: all tests PASS.

- [ ] **Step 4: `cargo build` and `cargo clippy`**

Run: `cargo build && cargo clippy --all-targets -- -D warnings`
Expected: clean. `pub(crate)` items with no external callers yet will not warn as dead code because the test module uses them; if clippy complains about anything unused before Task 5 wires it up, that's expected to resolve once Task 5 lands — re-run clippy after Task 5 instead of fighting it here.

- [ ] **Step 5: Commit**

```bash
git add src/stove/executor.rs src/stove/mod.rs
git commit -m "feat: scalar and three-valued boolean expression evaluator"
```

---

### Task 5: Statement executor

**Files:**
- Modify: `src/stove/executor.rs` (append to the file created in Task 4)

**Interfaces:**
- Consumes: `sous_chef::Plan` (Task 3), `Pot::{recipe, create_table, drop_table, insert_strand, scan_noodle, insert_with_key, delete_by_key}` (Task 2 + existing), `eval_scalar`/`eval_tri`/`full_schema`/`full_row` (Task 4).
- Produces: `pub(crate) fn execute(plan: Plan, pot: &mut Pot) -> Result<ResultSet, Scottatura>` — consumed by Task 6.

- [ ] **Step 1: Write `execute()` with its unit tests**

Append to `src/stove/executor.rs` (after the existing `eval_tri` function, before the `#[cfg(test)]` module — merge these new helpers into the file, then extend the existing test module):

```rust
use crate::parser::ast::SelectItem;
use crate::sous_chef::Plan;
use crate::stove::pot::Pot;
use crate::ResultSet;

fn project(items: &[SelectItem], schema: &[ColumnDef], row: &[Value]) -> Result<(Vec<String>, Vec<Value>), Scottatura> {
    let mut names = Vec::new();
    let mut vals = Vec::new();
    for (i, item) in items.iter().enumerate() {
        match item {
            SelectItem::Star => {
                for col in schema {
                    names.push(col.name.clone());
                    vals.push(column_value(&col.name, row, schema)?);
                }
            }
            SelectItem::Expr { expr, alias } => {
                let name = alias.clone().unwrap_or_else(|| match expr {
                    Expr::Column { name, .. } => name.clone(),
                    _ => format!("expr{}", i + 1),
                });
                names.push(name);
                vals.push(eval_scalar(expr, row, schema)?);
            }
        }
    }
    Ok((names, vals))
}

fn resolve_insert_values(table: &str, table_cols: &[ColumnDef], columns: &[String], row_exprs: &[Expr]) -> Result<Vec<Value>, Scottatura> {
    for name in columns {
        if !table_cols.iter().any(|c| &c.name == name) {
            return Err(Scottatura::ColumnNotFound { col: name.clone(), name: table.to_string() });
        }
    }
    let mut by_name: std::collections::HashMap<&str, Value> = std::collections::HashMap::new();
    for (name, expr) in columns.iter().zip(row_exprs.iter()) {
        by_name.insert(name.as_str(), eval_scalar(expr, &[], &[])?);
    }
    Ok(table_cols.iter().map(|col| by_name.remove(col.name.as_str()).unwrap_or(Value::Null)).collect())
}

pub(crate) fn execute(plan: Plan, pot: &mut Pot) -> Result<ResultSet, Scottatura> {
    match plan {
        Plan::CreateTable { name, columns } => {
            let cols = columns.into_iter().map(|c| ColumnDef {
                name: c.name, col_type: c.col_type, nullable: !c.not_null, strainer_root: None,
            }).collect();
            pot.create_table(&name, cols)?;
            Ok(ResultSet::default())
        }
        Plan::DropTable { name } => {
            pot.drop_table(&name)?;
            Ok(ResultSet::default())
        }
        Plan::Insert { table, columns, rows } => {
            let recipe = pot.recipe(&table)?;
            let mut last_key = None;
            let n = rows.len();
            for row_exprs in &rows {
                let values = resolve_insert_values(&table, &recipe.columns, &columns, row_exprs)?;
                last_key = Some(pot.insert_strand(&table, values)?);
            }
            Ok(ResultSet { columns: vec![], rows: vec![], affected_rows: n as u64, last_insert_key: last_key })
        }
        Plan::SeqScan { table, columns, filter } => {
            let recipe = pot.recipe(&table)?;
            let schema = full_schema(&recipe.columns);
            let raw_rows = pot.scan_noodle(&table)?;
            let mut out_names = Vec::new();
            let mut out_rows = Vec::new();
            for (key, values) in raw_rows {
                let row = full_row(key, &values);
                if let Some(f) = &filter {
                    if !eval_tri(f, &row, &schema)?.is_true() {
                        continue;
                    }
                }
                let (names, vals) = project(&columns, &schema, &row)?;
                out_names = names;
                out_rows.push(vals);
            }
            let n = out_rows.len() as u64;
            Ok(ResultSet { columns: out_names, rows: out_rows, affected_rows: n, last_insert_key: None })
        }
        Plan::Update { table, assignments, filter } => {
            let recipe = pot.recipe(&table)?;
            let schema = full_schema(&recipe.columns);
            for (name, _) in &assignments {
                if name == "id" {
                    return Err(Scottatura::AlDenteKeyNotUpdatable);
                }
            }
            let raw_rows = pot.scan_noodle(&table)?;
            let mut affected = 0u64;
            for (key, values) in raw_rows {
                let row = full_row(key, &values);
                if !eval_tri(&filter, &row, &schema)?.is_true() {
                    continue;
                }
                let mut new_values = values.clone();
                for (name, expr) in &assignments {
                    let idx = recipe.columns.iter().position(|c| &c.name == name)
                        .ok_or_else(|| Scottatura::ColumnNotFound { col: name.clone(), name: table.clone() })?;
                    new_values[idx] = eval_scalar(expr, &row, &schema)?;
                }
                pot.delete_by_key(&table, key)?;
                pot.insert_with_key(&table, key, &new_values)?;
                affected += 1;
            }
            Ok(ResultSet { columns: vec![], rows: vec![], affected_rows: affected, last_insert_key: None })
        }
        Plan::Delete { table, filter } => {
            let recipe = pot.recipe(&table)?;
            let schema = full_schema(&recipe.columns);
            let raw_rows = pot.scan_noodle(&table)?;
            let mut affected = 0u64;
            for (key, values) in raw_rows {
                let row = full_row(key, &values);
                if !eval_tri(&filter, &row, &schema)?.is_true() {
                    continue;
                }
                pot.delete_by_key(&table, key)?;
                affected += 1;
            }
            Ok(ResultSet { columns: vec![], rows: vec![], affected_rows: affected, last_insert_key: None })
        }
    }
}

#[cfg(test)]
mod execute_tests {
    use super::*;
    use crate::{ColType, ColumnDef};
    use crate::sous_chef::plan;
    use crate::parser::parse_statement;

    fn temp_pot() -> Pot {
        let f = tempfile::NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_string();
        drop(f);
        std::fs::remove_file(&path).ok();
        Pot::open(&path).unwrap()
    }

    fn run(pot: &mut Pot, sql: &str) -> Result<ResultSet, Scottatura> {
        execute(plan(parse_statement(sql).unwrap()).unwrap(), pot)
    }

    #[test]
    fn test_create_insert_serve() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT, age INT);").unwrap();
        let r = run(&mut pot, "LADLE INTO users (name, age) PORTIONS ('Tony', 38);").unwrap();
        assert_eq!(r.last_insert_key, Some(1));
        let r = run(&mut pot, "SERVE * FROM users STRAINED BY age = 38;").unwrap();
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.columns, vec!["id", "name", "age"]);
    }

    #[test]
    fn test_insert_with_partial_columns_defaults_to_vuoto() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT, age INT);").unwrap();
        run(&mut pot, "LADLE INTO users (name) PORTIONS ('Tony');").unwrap();
        let r = run(&mut pot, "SERVE * FROM users;").unwrap();
        assert_eq!(r.rows[0][2], Value::Null);
    }

    #[test]
    fn test_update_moves_row_keeping_key() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT, age INT);").unwrap();
        run(&mut pot, "LADLE INTO users (name, age) PORTIONS ('Tony', 38);").unwrap();
        run(&mut pot, "SEASON users SET age = 39 STRAINED BY name = 'Tony';").unwrap();
        let r = run(&mut pot, "SERVE * FROM users;").unwrap();
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0][0], Value::Integer(1)); // same id
        assert_eq!(r.rows[0][2], Value::Integer(39));
    }

    #[test]
    fn test_update_id_rejected() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT, age INT);").unwrap();
        run(&mut pot, "LADLE INTO users (name, age) PORTIONS ('Tony', 38);").unwrap();
        let err = run(&mut pot, "SEASON users SET id = 5 STRAINED BY name = 'Tony';").unwrap_err();
        assert!(matches!(err, Scottatura::AlDenteKeyNotUpdatable));
    }

    #[test]
    fn test_delete_removes_matching_row() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT, age INT);").unwrap();
        run(&mut pot, "LADLE INTO users (name, age) PORTIONS ('Tony', 38), ('Maria', 12);").unwrap();
        let r = run(&mut pot, "DISCARD FROM users STRAINED BY age < 18;").unwrap();
        assert_eq!(r.affected_rows, 1);
        let r = run(&mut pot, "SERVE * FROM users;").unwrap();
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0][1], Value::Text("Tony".to_string()));
    }

    #[test]
    fn test_drop_table() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT);").unwrap();
        run(&mut pot, "TOSS OUT noodle users;").unwrap();
        assert!(pot.noodles().is_empty());
    }
}
```

`ColType` import: `src/stove/executor.rs` already imports `crate::{ColType, ColumnDef, Scottatura, Value}` from Task 4 — no duplicate import needed for the main body; the `execute_tests` module re-imports `ColType, ColumnDef` for clarity, which is harmless (shadowing an already-in-scope `use` in a nested module is fine in Rust).

- [ ] **Step 2: Make `stove::pot` and `sous_chef` reachable from the executor module**

`src/stove/executor.rs` uses `crate::stove::pot::Pot` and `crate::sous_chef::Plan`. Confirm `src/stove/mod.rs` has `pub mod pot;` (it already does) and `src/lib.rs` has `pub mod sous_chef;` (it already does). No changes needed — this step is a verification, not an edit.

- [ ] **Step 3: Run the tests**

Run: `cargo test --lib executor:: -- --nocapture`
Expected: all tests in both `tests` and `execute_tests` modules PASS.

- [ ] **Step 4: Run the full suite, `cargo build`, `cargo clippy`**

Run: `cargo test && cargo build && cargo clippy --all-targets -- -D warnings`
Expected: everything clean, all tests passing.

- [ ] **Step 5: Commit**

```bash
git add src/stove/executor.rs
git commit -m "feat: Stove statement executor — SERVE/LADLE/SEASON/DISCARD/DDL"
```

---

### Task 6: `Pot::execute` public entry point

**Files:**
- Modify: `src/stove/pot.rs`

**Interfaces:**
- Produces: `pub fn execute(&mut self, sauql: &str) -> Result<ResultSet, Scottatura>` on `Pot` — the top-level API this whole phase exists to deliver.

- [ ] **Step 1: Add the method**

In `src/stove/pot.rs`, add to the `impl Pot` block (after `delete_by_key`, or anywhere inside the block):

```rust
    pub fn execute(&mut self, sauql: &str) -> Result<crate::ResultSet, Scottatura> {
        let stmt = crate::parser::parse_statement(sauql)?;
        let plan = crate::sous_chef::plan(stmt)?;
        crate::stove::executor::execute(plan, self)
    }
```

Note `crate::stove::executor::execute` is `pub(crate)`, which is sufficient here since `Pot::execute` (this new method) is itself defined inside the `crate::stove` tree and is the only caller.

- [ ] **Step 2: Write the failing test first**

Create the start of `tests/executor.rs` (this file is completed in Task 7; for now just the smoke test):

```rust
use marinaradb::Pot;

fn temp_path() -> String {
    let f = tempfile::NamedTempFile::new().unwrap();
    let p = f.path().to_str().unwrap().to_string();
    drop(f);
    p
}

#[test]
fn test_pot_execute_smoke() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT NOT VUOTO, age INT);").unwrap();
    let r = pot.execute("LADLE INTO users (name, age) PORTIONS ('Tony', 38);").unwrap();
    assert_eq!(r.last_insert_key, Some(1));
    let r = pot.execute("SERVE * FROM users STRAINED BY age = 38;").unwrap();
    assert_eq!(r.rows.len(), 1);
    std::fs::remove_file(&path).ok();
}
```

- [ ] **Step 3: Run test to verify it fails, then passes**

Run: `cargo test --test executor`
Expected: FAILs to compile before Step 1's edit lands (`execute` not found on `Pot`); PASSes after.

- [ ] **Step 4: `cargo build` and `cargo clippy`**

Run: `cargo build && cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/stove/pot.rs tests/executor.rs
git commit -m "feat: Pot::execute — parse, plan, and run a SaucQL statement end-to-end"
```

---

### Task 7: Full integration test suite

**Files:**
- Modify: `tests/executor.rs` (extend the file from Task 6)

**Interfaces:**
- Consumes: `Pot::execute` (Task 6), `Scottatura` variants from Task 1.

- [ ] **Step 1: Add the remaining integration tests**

Append to `tests/executor.rs`:

```rust
use marinaradb::{Scottatura, Value};

#[test]
fn test_full_crud_lifecycle_through_execute() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT NOT VUOTO, age INT);").unwrap();
    pot.execute("LADLE INTO users (name, age) PORTIONS ('Tony', 38), ('Maria', 29);").unwrap();
    pot.execute("SEASON users SET age = 39 STRAINED BY name = 'Tony';").unwrap();
    let r = pot.execute("SERVE name, age FROM users STRAINED BY age > 30;").unwrap();
    assert_eq!(r.rows.len(), 2);
    pot.execute("DISCARD FROM users STRAINED BY age < 35;").unwrap();
    let r = pot.execute("SERVE * FROM users;").unwrap();
    assert_eq!(r.rows.len(), 1);
    pot.execute("TOSS OUT noodle users;").unwrap();
    assert!(pot.noodles().is_empty());
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_multi_page_insert_via_execute() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle nums (n INT);").unwrap();
    for i in 0..500 {
        pot.execute(&format!("LADLE INTO nums (n) PORTIONS ({i});")).unwrap();
    }
    let r = pot.execute("SERVE * FROM nums;").unwrap();
    assert_eq!(r.rows.len(), 500);
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_join_returns_clause_not_yet_supported() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT);").unwrap();
    pot.execute("PLATE noodle orders (user_id INT);").unwrap();
    let err = pot.execute("SERVE * FROM users u TOSS orders o ON u.id = o.user_id;").unwrap_err();
    assert!(matches!(err, Scottatura::ClauseNotYetSupported { .. }));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_order_by_limit_bundled_by_all_return_clause_not_yet_supported() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (age INT);").unwrap();
    assert!(matches!(
        pot.execute("SERVE * FROM users ARRANGED BY age;").unwrap_err(),
        Scottatura::ClauseNotYetSupported { .. }
    ));
    assert!(matches!(
        pot.execute("SERVE * FROM users ONLY 10;").unwrap_err(),
        Scottatura::ClauseNotYetSupported { .. }
    ));
    assert!(matches!(
        pot.execute("SERVE age FROM users BUNDLED BY age;").unwrap_err(),
        Scottatura::ClauseNotYetSupported { .. }
    ));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_transactions_return_transaction_not_supported() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    assert!(matches!(pot.execute("BEGIN SIMMER;").unwrap_err(), Scottatura::TransactionNotSupported));
    assert!(matches!(pot.execute("FINISH SIMMER;").unwrap_err(), Scottatura::TransactionNotSupported));
    assert!(matches!(pot.execute("SPILL SIMMER;").unwrap_err(), Scottatura::TransactionNotSupported));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_strain_and_remove_strainer_return_strainer_not_supported() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (age INT);").unwrap();
    assert!(matches!(
        pot.execute("STRAIN idx1 ON users (age);").unwrap_err(),
        Scottatura::StrainerNotSupported
    ));
    assert!(matches!(
        pot.execute("REMOVE STRAINER idx1;").unwrap_err(),
        Scottatura::StrainerNotSupported
    ));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_vuoto_three_valued_logic_excludes_unknown_rows() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT, age INT);").unwrap();
    pot.execute("LADLE INTO users (name) PORTIONS ('NoAge');").unwrap(); // age is VUOTO
    pot.execute("LADLE INTO users (name, age) PORTIONS ('HasAge', 20);").unwrap();
    let r = pot.execute("SERVE * FROM users STRAINED BY age = 20;").unwrap();
    assert_eq!(r.rows.len(), 1);
    let r = pot.execute("SERVE * FROM users STRAINED BY age IS VUOTO;").unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0][1], Value::Text("NoAge".to_string()));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_not_null_violation_through_execute() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT NOT VUOTO);").unwrap();
    let err = pot.execute("LADLE INTO users () PORTIONS ();").unwrap_err();
    assert!(matches!(err, Scottatura::NullConstraintViolation { .. }) || matches!(err, Scottatura::UnexpectedToken { .. }));
    std::fs::remove_file(&path).ok();
}
```

Note on the last test: `LADLE INTO users () PORTIONS ();` may not parse (empty column list) — if `pot.execute` returns `Scottatura::UnexpectedToken` instead of reaching the executor, that's still a correct rejection of the malformed statement, hence the `||` in the assertion. Run it and see which branch actually fires; either is an acceptable, already-covered outcome.

- [ ] **Step 2: Run the full test file**

Run: `cargo test --test executor -- --nocapture`
Expected: all tests PASS.

- [ ] **Step 3: Run the entire workspace test suite**

Run: `cargo test`
Expected: every test across `src/` unit tests and all `tests/*.rs` integration files PASSES — no regressions in `tests/foundation.rs`, `tests/parser.rs`, or `tests/heap_chain.rs`.

- [ ] **Step 4: `cargo build` and `cargo clippy`**

Run: `cargo build && cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add tests/executor.rs
git commit -m "test: Phase 2b integration tests — full CRUD, multi-page, deferred-clause errors"
```

---

### Task 8: Documentation refresh and final self-review

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

**Interfaces:** None — documentation only.

- [ ] **Step 1: Update `README.md`'s Current Status section**

Replace the `## Current Status` section (currently describes Phase 1 only) with:

```markdown
## Current Status

**Phase 2b — Sous Chef Planner + Stove Executor** (unreleased, building on `v0.1.0`)

The storage layer, SaucQL parser, and single-table executor are implemented and tested. Joins, `ARRANGED BY`/`ONLY`/`SKIP`, `BUNDLED BY`, aggregates, transactions, and Strainer indexes are parsed but not yet executable.

What works:
- Open/create a `.pot` database file; data persists across process restarts
- Create and drop noodles (tables) with typed ingredients (columns)
- `Pot::execute(sauql)` — run a SaucQL string end-to-end:
  - `PLATE noodle` / `TOSS OUT noodle` (CREATE/DROP TABLE)
  - `LADLE INTO ... PORTIONS ...` (INSERT), including partial column lists and multi-row inserts
  - `SERVE ... FROM ... STRAINED BY ...` (SELECT with WHERE) — full expression support: comparisons, `AND`/`OR`/`NOT`, `IS VUOTO`, `BETWEEN`, `LIKE`, `IN`, three-valued VUOTO logic, `INTEGER`→`REAL` coercion
  - `SEASON ... SET ... STRAINED BY ...` (UPDATE)
  - `DISCARD FROM ... STRAINED BY ...` (DELETE)
- Multi-page heap storage — tables are no longer capped at one page of rows
- Auto-increment Al Dente Keys (primary keys)
- `NOT VUOTO` (NOT NULL) constraint enforcement

What's stubbed (not yet executable — parses but returns a clear error):
- Joins (`TOSS`), `ARRANGED BY`, `ONLY`/`SKIP`, `BUNDLED BY`, aggregate functions
- Transactions (`BEGIN`/`FINISH`/`SPILL SIMMER`) — awaiting the Prep Log (WAL)
- `STRAIN`/`REMOVE STRAINER` (B-tree indexes) — Phase 3
- WAL-based crash recovery
```

- [ ] **Step 2: Add a CHANGELOG entry**

In `CHANGELOG.md`, replace the empty `## [Unreleased]` section with:

```markdown
## [Unreleased]

### Added

- `Pot::execute(sauql: &str) -> Result<ResultSet, Scottatura>` — parses, plans, and executes a SaucQL statement end-to-end
- Sous Chef planner (`sous_chef::plan`) — translates parsed statements into an executable `Plan`, rejecting joins/`ARRANGED BY`/`ONLY`/`SKIP`/`BUNDLED BY`/transactions/Strainer DDL with clear `EXEC_*` errors
- Stove executor — single-table `SERVE`/`LADLE INTO`/`SEASON`/`DISCARD FROM` with full `STRAINED BY` expression support (comparisons, `AND`/`OR`/`NOT`, `IS VUOTO`, `BETWEEN`, `LIKE`, `IN`, three-valued VUOTO logic, `INTEGER`→`REAL` coercion)
- Multi-page heap traversal — heap storage now chains across pages via `next_page` instead of capping a table at one page
- `RICETTA_005`, `EXEC_001`–`EXEC_003` Scottatura error codes
```

- [ ] **Step 3: Run the full verification suite one more time**

Run: `cargo test && cargo build && cargo clippy --all-targets -- -D warnings`
Expected: all clean — this is the final gate before considering Phase 2b done.

- [ ] **Step 4: Commit**

```bash
git add README.md CHANGELOG.md
git commit -m "docs: update README and CHANGELOG for Phase 2b"
```

---

### Task 9: Interactive REPL binary

**Files:**
- Modify: `src/main.rs` (currently a two-line stub that prints a banner and exits)
- Create: `src/repl/mod.rs`
- Modify: `src/lib.rs` (add `pub mod repl;` if the formatting/loop helpers need to be unit-testable; otherwise keep `repl` binary-only under `src/bin` — see Step 1 for the decision)
- Modify: `README.md` (`## Run the Binary` section)
- Modify: `CHANGELOG.md`

**Rationale:** Everything through Task 8 only exposes MarinaraDB as a Rust library (`Pot::execute` callable from Rust code). This task makes it usable the way `psql`/`sqlite3` are usable: point a terminal at a `.pot` file and type SaucQL.

**Interfaces:**
- Consumes: `Pot::open`, `Pot::execute`, `ResultSet`, `Scottatura` (all existing, from Task 6).
- Produces: `pub(crate) fn run(pot: &mut Pot, input: impl std::io::BufRead, output: &mut impl std::io::Write) -> std::io::Result<()>` in `src/repl/mod.rs` — takes the I/O streams as parameters (not raw stdin/stdout) specifically so the REPL loop is unit-testable without a real terminal.
- Produces: `pub(crate) fn format_result(rs: &ResultSet) -> String` — used by `run` and directly unit-tested.

**No new dependencies for this task.** Line editing/history (e.g. `rustyline`) is an explicit non-goal here — track it as a fast-follow once the plain-stdin loop works, since it pulls in a dependency the rest of this plan deliberately avoided.

- [ ] **Step 1: Write `format_result` with unit tests**

Create `src/repl/mod.rs`. `format_result` renders a `ResultSet` two ways:
- If `rs.columns` is non-empty (a `SERVE`/SELECT result): a header row, a `-|-` separator, then each data row, columns aligned to the widest value per column (compute width via `Value`'s `Display`/`Debug`-free formatting — reuse whatever string conversion `Value` already exposes; if none exists, add a small local `fn value_to_cell(v: &Value) -> String` in this file rather than touching `Value` itself).
- If `rs.columns` is empty (INSERT/UPDATE/DELETE/DDL): a single summary line, `"{n} row(s) affected"` pluralized correctly for `n == 1`, plus `" (id {k})"` appended when `last_insert_key` is `Some(k)`.

Write `#[cfg(test)] mod tests` covering: empty SELECT result (header only, "0 rows"), a multi-row multi-column SELECT with mixed `Value` types including `Value::Null` (rendered as an empty cell or literal `VUOTO` — pick one and test it), a single-row-affected DML result, a multi-row-affected DML result (pluralization), and an INSERT result with `last_insert_key`.

- [ ] **Step 2: Write the REPL loop with unit tests against in-memory I/O**

Add `run(pot, input, output)` to the same file:
- Print the prompt `"marinara> "` to `output` before each read (no trailing newline).
- Read lines from `input` and accumulate them into a buffer until the buffer contains a `;` — statement boundaries are naive semicolon-splitting (matches the parser's own statement terminator; the parser is the authority on whether the accumulated text is valid, this loop just decides when to hand it off). While accumulating a not-yet-terminated buffer, print the continuation prompt `"     -> "` instead of `"marinara> "`.
- Recognize dot-commands *before* semicolon accumulation, matched on the trimmed line: `.tables` (print `pot.noodles()`, one per line, or `"(no tables)"` if empty), `.help` (print a short command summary), `.quit` / `.exit` (return `Ok(())` immediately). Dot-commands don't require a trailing `;`.
- For a completed SaucQL buffer, call `pot.execute(&buffer)`; on `Ok(rs)` print `format_result(&rs)`; on `Err(e)` print `"Error: {e}"` to `output` (not a hard failure — clear the buffer and continue the loop, matching how `psql`/`sqlite3` recover from a bad statement).
- On EOF from `input` (a `read_line` returning `Ok(0)`), exit the loop and return `Ok(())`.

Write `#[cfg(test)] mod tests` (or extend the Step 1 module) using `std::io::Cursor`/`Vec<u8>` as the reader/writer: a full session — create table, insert, select, a `.tables` command, a deliberately malformed statement (assert the error message appears in output *and* the loop continues to the next statement), and `.quit` ending the session before EOF.

- [ ] **Step 3: Wire it into `main.rs`**

Replace `src/main.rs`:

```rust
use std::io::{BufReader, Write};

fn main() -> std::io::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "marinara.pot".to_string());
    let mut pot = match marinaradb::Pot::open(&path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Could not open {path}: {e}");
            std::process::exit(1);
        }
    };
    println!("MarinaraDB v0.1.0 — Buon appetito!");
    println!("Connected to {path}. Type .help for commands, .quit to exit.");
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    marinaradb::repl::run(&mut pot, BufReader::new(stdin.lock()), &mut stdout)?;
    stdout.flush()
}
```

Add `pub mod repl;` to `src/lib.rs` so `main.rs` (a separate crate target) can reach it; mark the module and its contents `pub` rather than `pub(crate)` for that reason — adjust the **Interfaces** signatures above from `pub(crate)` to `pub` accordingly when implementing.

- [ ] **Step 4: Manual smoke test**

Run: `cargo run` (defaults to `marinara.pot` in the cwd), then interactively:
```
marinara> PLATE noodle users (name TEXT, age INT);
marinara> LADLE INTO users (name, age) PORTIONS ('Tony', 38);
marinara> SERVE * FROM users;
marinara> .tables
marinara> .quit
```
Expected: table renders with aligned columns, `.tables` lists `users`, `.quit` exits cleanly. Delete the generated `marinara.pot` file afterward — it's a manual-test artifact, not something to commit.

- [ ] **Step 5: Run the full suite, `cargo build`, `cargo clippy`**

Run: `cargo test && cargo build && cargo clippy --all-targets -- -D warnings`
Expected: clean, including the new `src/repl/mod.rs` unit tests.

- [ ] **Step 6: Update `README.md`'s `## Run the Binary` section**

Replace the "stub" description with the REPL usage shown in Step 4, plus a note that `cargo run -- path/to/db.pot` opens a specific file.

- [ ] **Step 7: Add a CHANGELOG entry**

Append to the `### Added` list under `## [Unreleased]`:

```markdown
- Interactive REPL (`cargo run`) — a `psql`/`sqlite3`-style terminal session over `Pot::execute`, with `.tables`/`.help`/`.quit` dot-commands
```

- [ ] **Step 8: Commit**

```bash
git add src/main.rs src/repl/mod.rs src/lib.rs README.md CHANGELOG.md
git commit -m "feat: interactive REPL — MarinaraDB usable as a standalone terminal session"
```

---

## Self-Review Checklist (run before calling Phase 2b complete)

- [ ] `cargo test` passes with zero failures across unit tests and all `tests/*.rs` files.
- [ ] `cargo build` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every statement in `docs/sql-semantics.md` § "Supported Statements" executes via `Pot::execute` for the single-table case: `PLATE noodle` ✓, `TOSS OUT noodle` ✓, `SERVE` ✓, `LADLE INTO` ✓, `SEASON` ✓, `DISCARD FROM` ✓. `STRAIN`/`REMOVE STRAINER`/`BEGIN SIMMER`/`FINISH SIMMER`/`SPILL SIMMER` correctly return their deferred-feature errors.
- [ ] Every `STRAINED BY` operator in `docs/sql-semantics.md` is implemented and tested: `=`, `!=`, `<`, `>`, `<=`, `>=`, `AND`, `OR`, `NOT`, `IS VUOTO`/`IS NOT VUOTO`, `BETWEEN`, `LIKE`, `IN`.
- [ ] VUOTO three-valued logic and `INTEGER`→`REAL` coercion match `docs/sql-semantics.md` exactly.
- [ ] No `todo!()`, `unimplemented!()`, or placeholder strings remain in `src/sous_chef/` or `src/stove/executor.rs`.
- [ ] `README.md` and `CHANGELOG.md` accurately describe what works vs. what's stubbed.
- [ ] `cargo run` opens a REPL that can create a table, insert, select, and exit via `.quit` — MarinaraDB is usable without writing any Rust.
