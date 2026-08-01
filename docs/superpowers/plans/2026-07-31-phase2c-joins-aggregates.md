# Phase 2c: Joins, Ordering, Limiting, Grouping & Aggregates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `TOSS` (inner equi-join), `ARRANGED BY` (ORDER BY), `ONLY`/`SKIP` (LIMIT/OFFSET), `BUNDLED BY` (GROUP BY), and aggregate functions (`COUNT`/`SUM`/`AVG`/`MIN`/`MAX`) into the Sous Chef planner and Stove executor, which today reject all of them with `ClauseNotYetSupported`.

**Architecture:** `Plan::SeqScan` is replaced by a richer `Plan::Select` carrying the whole query. The executor runs a fixed pipeline: join → filter → group/aggregate → project → order → limit/offset. Column resolution becomes qualifier-aware (a new `Schema`/`SchemaColumn` type replacing bare `Vec<ColumnDef>`) so `u.name` vs `name` can be resolved across a joined pair of tables, with ambiguity detection for unqualified references.

**Tech Stack:** Rust (2021 edition), existing crate structure (`src/parser`, `src/sous_chef`, `src/stove`), `cargo test` / `cargo clippy`.

## Global Constraints

- `cargo build`, `cargo test`, and `cargo clippy --all-targets -- -D warnings` must stay clean after every task (this is the project's established convention from prior phases).
- No new external dependencies.
- Follow the spec exactly: `docs/superpowers/specs/2026-07-31-phase2c-joins-aggregates-design.md`. In particular: VUOTO always sorts first regardless of ASC/DESC; aggregate-context queries reject `SELECT *` and reject non-aggregate columns not in `BUNDLED BY`; unqualified ambiguous join columns are a hard error, not silently resolved.
- Every new `Scottatura` variant needs a `#[error(...)]` message in the existing Italian-flavoured style, consistent with `src/lib.rs`.

---

### Task 1: Qualifier-aware `Schema` type (pure refactor, no new behavior)

**Files:**
- Modify: `src/stove/executor.rs` (schema type, `full_schema`, `full_row`, `column_value`, `eval_scalar`, `eval_tri`, `project_names`, `project_values`, and every call site in `execute()`)

**Interfaces:**
- Produces: `pub(crate) struct SchemaColumn { pub owner: String, pub col: ColumnDef }`, `pub(crate) type Schema = Vec<SchemaColumn>;`, `fn full_schema(owner: &str, cols: &[ColumnDef]) -> Schema`, `fn column_value(qualifier: Option<&str>, name: &str, row: &[Value], schema: &Schema) -> Result<Value, Scottatura>`, `pub(crate) fn eval_scalar(expr: &Expr, row: &[Value], schema: &Schema) -> Result<Value, Scottatura>`, `pub(crate) fn eval_tri(expr: &Expr, row: &[Value], schema: &Schema) -> Result<Tri, Scottatura>`.
- Consumes: nothing new from other tasks — this is prerequisite plumbing for Tasks 3–6.

This task changes internal signatures only; single-table query behavior must be identical before and after (verified by the full existing test suite staying green), plus one new unit test proving qualifier resolution and ambiguity detection work.

- [ ] **Step 1: Write the failing unit test for qualifier-aware `column_value`**

Add to the `#[cfg(test)] mod tests` block at the bottom of `src/stove/executor.rs` (near the existing `fn schema()` helper — rename that helper's usages to build a `Schema` instead of `Vec<ColumnDef>`, see Step 3):

```rust
    fn two_table_schema() -> Schema {
        let mut s = full_schema("users", &[
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
        ]);
        s.extend(full_schema("orders", &[
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
        ]));
        s
    }

    #[test]
    fn test_qualified_column_resolves_correct_side() {
        // schema: [users.id, users.name, orders.id, orders.name]
        let row = vec![
            Value::Integer(1), Value::Text("Tony".into()),
            Value::Integer(9), Value::Text("Pizza".into()),
        ];
        let schema = two_table_schema();
        let v = column_value(Some("orders"), "name", &row, &schema).unwrap();
        assert_eq!(v, Value::Text("Pizza".into()));
    }

    #[test]
    fn test_unqualified_ambiguous_column_errors() {
        let row = vec![
            Value::Integer(1), Value::Text("Tony".into()),
            Value::Integer(9), Value::Text("Pizza".into()),
        ];
        let schema = two_table_schema();
        let err = column_value(None, "name", &row, &schema).unwrap_err();
        assert!(matches!(err, Scottatura::AmbiguousColumn { name } if name == "name"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail to compile**

Run: `cargo test --lib stove::executor::tests::test_qualified_column_resolves_correct_side stove::executor::tests::test_unqualified_ambiguous_column_errors`
Expected: compile error — `full_schema` takes one argument today, `Scottatura::AmbiguousColumn` doesn't exist yet, `column_value` doesn't take a qualifier.

- [ ] **Step 3: Add the `AmbiguousColumn` Scottatura variant**

In `src/lib.rs`, add to `pub enum Scottatura` (near `ColumnNotFound`):

```rust
    #[error("Scottatura [RICETTA_010]: Ingredient `{name}` is ambiguous — it exists in more than one noodle in this TOSS. Qualify it (e.g. `table.{name}`).")]
    AmbiguousColumn { name: String },
```

- [ ] **Step 4: Implement the `Schema`/`SchemaColumn` type and refactor `full_schema`/`full_row`/`column_value`**

In `src/stove/executor.rs`, replace:

```rust
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
```

with:

```rust
#[derive(Debug, Clone)]
pub(crate) struct SchemaColumn {
    pub owner: String,
    pub col: ColumnDef,
}

pub(crate) type Schema = Vec<SchemaColumn>;

pub(crate) fn full_schema(owner: &str, cols: &[ColumnDef]) -> Schema {
    let mut out = vec![SchemaColumn {
        owner: owner.to_string(),
        col: ColumnDef {
            name: "id".to_string(),
            col_type: ColType::Integer,
            nullable: false,
            strainer_root: None,
        },
    }];
    out.extend(cols.iter().cloned().map(|col| SchemaColumn { owner: owner.to_string(), col }));
    out
}
```

`full_row` is unchanged (it builds a plain `Vec<Value>`, independent of schema shape).

Replace `column_value`:

```rust
fn column_value(name: &str, row: &[Value], schema: &[ColumnDef]) -> Result<Value, Scottatura> {
    schema.iter().position(|c| c.name == name)
        .map(|i| row[i].clone())
        .ok_or_else(|| Scottatura::ColumnNotFound { col: name.to_string(), name: "?".to_string() })
}
```

with:

```rust
fn column_value(qualifier: Option<&str>, name: &str, row: &[Value], schema: &Schema) -> Result<Value, Scottatura> {
    let matches: Vec<usize> = schema.iter().enumerate()
        .filter(|(_, sc)| sc.col.name == name && qualifier.is_none_or(|q| sc.owner == q))
        .map(|(i, _)| i)
        .collect();
    match matches.as_slice() {
        [] => Err(Scottatura::ColumnNotFound { col: name.to_string(), name: qualifier.unwrap_or("?").to_string() }),
        [i] => Ok(row[*i].clone()),
        _ => Err(Scottatura::AmbiguousColumn { name: name.to_string() }),
    }
}
```

(`Option::is_none_or` is stable since Rust 1.82; check `rustc --version` — if older, use `qualifier.map_or(true, |q| sc.owner == q)` instead.)

- [ ] **Step 5: Thread `Schema` and the qualifier through `eval_scalar`/`eval_tri`**

In `eval_scalar`, change the signature to `pub(crate) fn eval_scalar(expr: &Expr, row: &[Value], schema: &Schema) -> Result<Value, Scottatura>` and update the `Expr::Column` arm:

```rust
        Expr::Column { qualifier, name } => column_value(qualifier.as_deref(), name, row, schema),
```

In `eval_tri`, change the signature to `pub(crate) fn eval_tri(expr: &Expr, row: &[Value], schema: &Schema) -> Result<Tri, Scottatura>` (body unchanged — it only forwards `schema` to `eval_scalar`/nested `eval_tri` calls, so the type change propagates without further edits).

- [ ] **Step 6: Update `project_names`/`project_values` and all call sites in `execute()`**

`project_names` and `project_values` take `schema: &[ColumnDef]` today — change both to `schema: &Schema`. Their `SelectItem::Star` arms currently do `for col in schema { ... col.name ... }` / call `column_value(&col.name, row, schema)`; simplify to iterate positionally instead of by name (avoids re-resolving through `column_value`, and works once duplicate names appear under joins in Task 5):

```rust
fn project_names(items: &[SelectItem], schema: &Schema) -> Vec<String> {
    let mut names = Vec::new();
    for (i, item) in items.iter().enumerate() {
        match item {
            SelectItem::Star => {
                for sc in schema {
                    names.push(sc.col.name.clone());
                }
            }
            SelectItem::Expr { expr, alias } => {
                let name = alias.clone().unwrap_or_else(|| match expr {
                    Expr::Column { name, .. } => name.clone(),
                    _ => format!("expr{}", i + 1),
                });
                names.push(name);
            }
        }
    }
    names
}

fn project_values(items: &[SelectItem], schema: &Schema, row: &[Value]) -> Result<Vec<Value>, Scottatura> {
    let mut vals = Vec::new();
    for item in items {
        match item {
            SelectItem::Star => {
                for i in 0..schema.len() {
                    vals.push(row[i].clone());
                }
            }
            SelectItem::Expr { expr, .. } => {
                vals.push(eval_scalar(expr, row, schema)?);
            }
        }
    }
    Ok(vals)
}
```

In `execute()`, update every call site that builds a schema or calls these functions:
- `Plan::SeqScan { table, columns, filter } => { let recipe = pot.recipe(&table)?; let schema = full_schema(&table, &recipe.columns); ... }` (add `&table` as the owner argument).
- `Plan::Update { table, .. }` and `Plan::Delete { table, .. }`: same — `full_schema(&table, &recipe.columns)`.

`resolve_insert_values` calls `eval_scalar(expr, &[], &[])`; change the empty schema literal to a typed empty `Schema`:

```rust
    let empty_schema: Schema = Vec::new();
    for (name, expr) in columns.iter().zip(row_exprs.iter()) {
        by_name.insert(name.as_str(), eval_scalar(expr, &[], &empty_schema)?);
    }
```

- [ ] **Step 7: Update existing test helpers that build a bare `Vec<ColumnDef>`**

In the `#[cfg(test)] mod tests` block, the `fn schema() -> Vec<ColumnDef>` helper (used by the filter-evaluation tests) must become `fn schema() -> Schema` using `full_schema("t", &[...])` with the `name`/`age` columns (drop the manual `id` entry — `full_schema` adds it):

```rust
    fn schema() -> Schema {
        full_schema("t", &[
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
            ColumnDef { name: "age".into(), col_type: ColType::Integer, nullable: true, strainer_root: None },
        ])
    }
```

- [ ] **Step 8: Run the full test suite**

Run: `cargo test`
Expected: all tests pass, including the two new ones from Step 1.

- [ ] **Step 9: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 10: Commit**

```bash
git add src/lib.rs src/stove/executor.rs
git commit -m "refactor: qualifier-aware Schema type in executor, prep for joins"
```

---

### Task 2: `Plan::Select` structural change (rename, no new behavior yet)

**Files:**
- Modify: `src/sous_chef/mod.rs`
- Modify: `src/stove/executor.rs` (only the `execute()` match arm name/fields)

**Interfaces:**
- Produces: `pub struct JoinPlan { pub table: TableRef, pub left_col: (Option<String>, String), pub right_col: (Option<String>, String) }`, and `Plan::Select { from: TableRef, join: Option<JoinPlan>, filter: Option<Expr>, group_by: Vec<String>, columns: Vec<SelectItem>, order_by: Vec<OrderByItem>, limit: Option<i64>, offset: Option<i64> }` replacing `Plan::SeqScan`.
- Consumes: `Schema`/`eval_scalar`/`eval_tri`/`project_names`/`project_values`/`full_schema` from Task 1.

At the end of this task, `plan()` still rejects `join`/`order_by`/`limit`/`offset`/`group_by` exactly as before (Tasks 3–6 remove one rejection each) — this task only changes the *shape* of the data flowing through, so it's safe to land on its own.

- [ ] **Step 1: Update `sous_chef` tests to expect `Plan::Select`**

In `src/sous_chef/mod.rs`, in `#[cfg(test)] mod tests`, change:

```rust
    #[test]
    fn test_select_becomes_seqscan() {
        let stmt = parse_statement("SERVE * FROM users STRAINED BY age > 18;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::SeqScan { table, .. } if table == "users"));
    }
```

to:

```rust
    #[test]
    fn test_select_becomes_select_plan() {
        let stmt = parse_statement("SERVE * FROM users STRAINED BY age > 18;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { from, .. } if from.name == "users"));
    }
```

and:

```rust
    #[test]
    fn test_select_without_filter() {
        let stmt = parse_statement("SERVE * FROM users;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::SeqScan { filter: None, .. }));
    }
```

to:

```rust
    #[test]
    fn test_select_without_filter() {
        let stmt = parse_statement("SERVE * FROM users;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { filter: None, .. }));
    }
```

- [ ] **Step 2: Run the tests to verify they fail to compile**

Run: `cargo test --lib sous_chef::tests::test_select_becomes_select_plan sous_chef::tests::test_select_without_filter`
Expected: compile error — `Plan::Select` doesn't exist yet.

- [ ] **Step 3: Replace the `Plan` enum and `plan()` in `src/sous_chef/mod.rs`**

Update the imports at the top:

```rust
use crate::parser::ast::{ColumnSpec, Expr, OrderByItem, SelectItem, Statement, TableRef};
use crate::Scottatura;
```

Replace the `Plan` enum:

```rust
#[derive(Debug, Clone)]
pub struct JoinPlan {
    pub table: TableRef,
    pub left_col: (Option<String>, String),
    pub right_col: (Option<String>, String),
}

#[derive(Debug, Clone)]
pub enum Plan {
    Select {
        from: TableRef,
        join: Option<JoinPlan>,
        filter: Option<Expr>,
        group_by: Vec<String>,
        columns: Vec<SelectItem>,
        order_by: Vec<OrderByItem>,
        limit: Option<i64>,
        offset: Option<i64>,
    },
    Insert { table: String, columns: Vec<String>, rows: Vec<Vec<Expr>> },
    Update { table: String, assignments: Vec<(String, Expr)>, filter: Expr },
    Delete { table: String, filter: Expr },
    CreateTable { name: String, columns: Vec<ColumnSpec> },
    DropTable { name: String },
}
```

Replace the `Statement::Select` arm in `plan()`:

```rust
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
            Ok(Plan::Select {
                from: sel.from,
                join: None,
                filter: sel.filter,
                group_by: sel.group_by,
                columns: sel.items,
                order_by: sel.order_by,
                limit: sel.limit,
                offset: sel.offset,
            })
        }
```

(`join` is hardcoded to `None` here because the `sel.join.is_some()` check above already rejected any query that has one — Task 5 replaces this arm.)

- [ ] **Step 4: Update the `execute()` match arm in `src/stove/executor.rs`**

Replace:

```rust
        Plan::SeqScan { table, columns, filter } => {
            let recipe = pot.recipe(&table)?;
            let schema = full_schema(&table, &recipe.columns);
            let raw_rows = pot.scan_noodle(&table)?;
            let out_names = project_names(&columns, &schema);
            let mut out_rows = Vec::new();
            for (key, values) in raw_rows {
                let row = full_row(key, &values);
                if let Some(f) = &filter {
                    if !eval_tri(f, &row, &schema)?.is_true() {
                        continue;
                    }
                }
                out_rows.push(project_values(&columns, &schema, &row)?);
            }
            let n = out_rows.len() as u64;
            Ok(ResultSet { columns: out_names, rows: out_rows, affected_rows: n, last_insert_key: None })
        }
```

with (unpacking the new fields but ignoring the ones not implemented yet — they're always empty/`None` at this point, guaranteed by `plan()`):

```rust
        Plan::Select { from, join: _, filter, group_by: _, columns, order_by: _, limit: _, offset: _ } => {
            let recipe = pot.recipe(&from.name)?;
            let schema = full_schema(&from.name, &recipe.columns);
            let raw_rows = pot.scan_noodle(&from.name)?;
            let out_names = project_names(&columns, &schema);
            let mut out_rows = Vec::new();
            for (key, values) in raw_rows {
                let row = full_row(key, &values);
                if let Some(f) = &filter {
                    if !eval_tri(f, &row, &schema)?.is_true() {
                        continue;
                    }
                }
                out_rows.push(project_values(&columns, &schema, &row)?);
            }
            let n = out_rows.len() as u64;
            Ok(ResultSet { columns: out_names, rows: out_rows, affected_rows: n, last_insert_key: None })
        }
```

Also update the `use crate::sous_chef::Plan;` import line if `JoinPlan` isn't referenced yet (it isn't, until Task 5 — no import change needed here).

- [ ] **Step 5: Run the full test suite**

Run: `cargo test`
Expected: all tests pass (including `stove::executor::execute_tests::*` which exercise `Plan::Select` indirectly via `plan(parse_statement(...))`).

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/sous_chef/mod.rs src/stove/executor.rs
git commit -m "refactor: Plan::SeqScan -> Plan::Select carrying full query shape"
```

---

### Task 3: `ARRANGED BY` (ORDER BY), VUOTO-first

**Files:**
- Modify: `src/sous_chef/mod.rs` (remove the `order_by` rejection, update/remove its test)
- Modify: `src/stove/executor.rs` (`sort_rows`, `compare_vuoto_first`, wire into `execute()`)
- Modify: `tests/executor.rs` (replace the now-stale combined rejection test)

**Interfaces:**
- Consumes: `Schema`, `Plan::Select` from Tasks 1–2; `compare_values` (existing, in `stove/executor.rs`).
- Produces: `fn sort_rows(rows: &mut [Vec<Value>], order_by: &[OrderByItem], out_names: &[String]) -> Result<(), Scottatura>` — consumed directly inside `execute()`, not by later tasks, but Task 7's combined tests exercise it via `Pot::execute`.

- [ ] **Step 1: Write the failing integration test**

In `tests/executor.rs`, add:

```rust
#[test]
fn test_order_by_sorts_ascending_and_descending_with_vuoto_first() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT, age INT);").unwrap();
    pot.execute("LADLE INTO users (name, age) PORTIONS ('Carol', 40), ('Alice', 20), ('Bob') , ('Dave', 20);").unwrap();
    // Bob has VUOTO age.
    let r = pot.execute("SERVE name, age FROM users ARRANGED BY age;").unwrap();
    let names: Vec<String> = r.rows.iter().map(|row| match &row[0] { Value::Text(s) => s.clone(), _ => panic!() }).collect();
    assert_eq!(names[0], "Bob"); // VUOTO first regardless of direction
    let ages: Vec<Value> = r.rows[1..].iter().map(|row| row[1].clone()).collect();
    assert_eq!(ages, vec![Value::Integer(20), Value::Integer(20), Value::Integer(40)]);

    let r = pot.execute("SERVE name, age FROM users ARRANGED BY age DESC;").unwrap();
    let names: Vec<String> = r.rows.iter().map(|row| match &row[0] { Value::Text(s) => s.clone(), _ => panic!() }).collect();
    assert_eq!(names[0], "Bob"); // still first in DESC
    let ages: Vec<Value> = r.rows[1..].iter().map(|row| row[1].clone()).collect();
    assert_eq!(ages, vec![Value::Integer(40), Value::Integer(20), Value::Integer(20)]);
    std::fs::remove_file(&path).ok();
}
```

Also replace the stale combined-rejection test (it currently asserts `ARRANGED BY` returns `ClauseNotYetSupported`, which will become false in this task):

```rust
#[test]
fn test_order_by_limit_bundled_by_all_return_clause_not_yet_supported() {
    ...
    assert!(matches!(
        pot.execute("SERVE * FROM users ARRANGED BY age;").unwrap_err(),
        Scottatura::ClauseNotYetSupported { .. }
    ));
    ...
```

Delete just the `ARRANGED BY` assertion block from this test (keep the `ONLY 10` and `BUNDLED BY` assertions — those still hold until Tasks 4 and 6 respectively). Rename the test to `test_limit_and_bundled_by_still_return_clause_not_yet_supported` to reflect the narrower scope.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test executor test_order_by_sorts_ascending_and_descending_with_vuoto_first`
Expected: FAIL — `plan()` still returns `ClauseNotYetSupported { clause: "ARRANGED BY" }`.

- [ ] **Step 3: Remove the `order_by` rejection in `sous_chef::plan`**

In `src/sous_chef/mod.rs`, delete:

```rust
            if !sel.order_by.is_empty() {
                return Err(Scottatura::ClauseNotYetSupported { clause: "ARRANGED BY".to_string() });
            }
```

Also delete or update the existing `test_order_by_rejected` unit test in the same file (it now needs to assert the opposite — that `ARRANGED BY` produces a `Plan::Select` with a non-empty `order_by`):

```rust
    #[test]
    fn test_order_by_passthrough() {
        let stmt = parse_statement("SERVE * FROM users ARRANGED BY age;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { order_by, .. } if order_by.len() == 1 && order_by[0].column == "age" && !order_by[0].descending));
    }
```

- [ ] **Step 4: Implement `compare_vuoto_first` and `sort_rows` in `src/stove/executor.rs`**

Add near `compare_values`:

```rust
/// Compares two projected values for ORDER BY. VUOTO always sorts first,
/// regardless of ASC/DESC — only non-VUOTO pairs are affected by direction.
/// A type mismatch between two values in the same projected column can't
/// happen for a column sourced from one declared type; if it somehow did
/// (e.g. via a REAL NaN), treat the pair as equal rather than propagating
/// an error through a non-fallible sort comparator.
fn compare_vuoto_first(a: &Value, b: &Value, descending: bool) -> Ordering {
    match (matches!(a, Value::Null), matches!(b, Value::Null)) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => {
            let ord = compare_values(a, b).unwrap_or(Ordering::Equal);
            if descending { ord.reverse() } else { ord }
        }
    }
}

use crate::parser::ast::OrderByItem;

/// Sorts projected rows in place. `order_by` columns must appear in
/// `out_names` (the projected output) — ORDER BY on a column that wasn't
/// selected/aliased is a ColumnNotFound error, not a fallback to the
/// underlying table schema.
fn sort_rows(rows: &mut [Vec<Value>], order_by: &[OrderByItem], out_names: &[String]) -> Result<(), Scottatura> {
    let mut positions = Vec::with_capacity(order_by.len());
    for item in order_by {
        let idx = out_names.iter().position(|n| n == &item.column)
            .ok_or_else(|| Scottatura::ColumnNotFound { col: item.column.clone(), name: "<projection>".to_string() })?;
        positions.push((idx, item.descending));
    }
    rows.sort_by(|a, b| {
        for &(idx, desc) in &positions {
            let ord = compare_vuoto_first(&a[idx], &b[idx], desc);
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    });
    Ok(())
}
```

- [ ] **Step 5: Wire `order_by` into the `Plan::Select` match arm**

Change the destructuring to capture `order_by` instead of ignoring it, and call `sort_rows` after the projection loop, before building the `ResultSet`:

```rust
        Plan::Select { from, join: _, filter, group_by: _, columns, order_by, limit: _, offset: _ } => {
            let recipe = pot.recipe(&from.name)?;
            let schema = full_schema(&from.name, &recipe.columns);
            let raw_rows = pot.scan_noodle(&from.name)?;
            let out_names = project_names(&columns, &schema);
            let mut out_rows = Vec::new();
            for (key, values) in raw_rows {
                let row = full_row(key, &values);
                if let Some(f) = &filter {
                    if !eval_tri(f, &row, &schema)?.is_true() {
                        continue;
                    }
                }
                out_rows.push(project_values(&columns, &schema, &row)?);
            }
            if !order_by.is_empty() {
                sort_rows(&mut out_rows, &order_by, &out_names)?;
            }
            let n = out_rows.len() as u64;
            Ok(ResultSet { columns: out_names, rows: out_rows, affected_rows: n, last_insert_key: None })
        }
```

- [ ] **Step 6: Run the tests**

Run: `cargo test`
Expected: all pass, including the new ORDER BY test and the renamed/narrowed combined-rejection test.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add src/sous_chef/mod.rs src/stove/executor.rs tests/executor.rs
git commit -m "feat: ARRANGED BY (ORDER BY) with VUOTO-first sort semantics"
```

---

### Task 4: `ONLY` / `SKIP` (LIMIT / OFFSET)

**Files:**
- Modify: `src/sous_chef/mod.rs` (remove the `limit`/`offset` rejection, update its test)
- Modify: `src/stove/executor.rs` (apply offset then limit at the end of the pipeline)
- Modify: `tests/executor.rs` (narrow/rename the combined rejection test further; add LIMIT/OFFSET tests)

**Interfaces:**
- Consumes: `Plan::Select.limit: Option<i64>`, `.offset: Option<i64>` (already present on the struct since Task 2).

- [ ] **Step 1: Write the failing integration test**

In `tests/executor.rs`:

```rust
#[test]
fn test_only_and_skip_apply_after_ordering() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle nums (n INT);").unwrap();
    for i in 0..10 {
        pot.execute(&format!("LADLE INTO nums (n) PORTIONS ({i});")).unwrap();
    }
    let r = pot.execute("SERVE n FROM nums ARRANGED BY n ONLY 3 SKIP 2;").unwrap();
    assert_eq!(r.rows, vec![vec![Value::Integer(2)], vec![Value::Integer(3)], vec![Value::Integer(4)]]);

    let r = pot.execute("SERVE n FROM nums ARRANGED BY n ONLY 0;").unwrap();
    assert!(r.rows.is_empty());

    let r = pot.execute("SERVE n FROM nums ARRANGED BY n SKIP 100;").unwrap();
    assert!(r.rows.is_empty());
    std::fs::remove_file(&path).ok();
}
```

Update `test_limit_and_bundled_by_still_return_clause_not_yet_supported` (from Task 3) to drop its `ONLY 10` assertion, keeping only `BUNDLED BY`; rename it to `test_bundled_by_still_returns_clause_not_yet_supported`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test executor test_only_and_skip_apply_after_ordering`
Expected: FAIL — `plan()` still rejects `ONLY`/`SKIP`.

- [ ] **Step 3: Remove the `limit`/`offset` rejection in `sous_chef::plan`**

In `src/sous_chef/mod.rs`, delete:

```rust
            if sel.limit.is_some() || sel.offset.is_some() {
                return Err(Scottatura::ClauseNotYetSupported { clause: "ONLY / SKIP".to_string() });
            }
```

Update the existing `test_limit_rejected` unit test to its positive counterpart:

```rust
    #[test]
    fn test_limit_offset_passthrough() {
        let stmt = parse_statement("SERVE * FROM users ONLY 10 SKIP 5;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { limit: Some(10), offset: Some(5), .. }));
    }
```

- [ ] **Step 4: Apply offset/limit in `execute()`**

Change the `Plan::Select` destructuring to capture `limit`/`offset`, and apply them after `sort_rows`:

```rust
        Plan::Select { from, join: _, filter, group_by: _, columns, order_by, limit, offset } => {
            let recipe = pot.recipe(&from.name)?;
            let schema = full_schema(&from.name, &recipe.columns);
            let raw_rows = pot.scan_noodle(&from.name)?;
            let out_names = project_names(&columns, &schema);
            let mut out_rows = Vec::new();
            for (key, values) in raw_rows {
                let row = full_row(key, &values);
                if let Some(f) = &filter {
                    if !eval_tri(f, &row, &schema)?.is_true() {
                        continue;
                    }
                }
                out_rows.push(project_values(&columns, &schema, &row)?);
            }
            if !order_by.is_empty() {
                sort_rows(&mut out_rows, &order_by, &out_names)?;
            }
            let skip_n = offset.unwrap_or(0).max(0) as usize;
            out_rows = out_rows.into_iter().skip(skip_n).collect();
            if let Some(l) = limit {
                out_rows.truncate(l.max(0) as usize);
            }
            let n = out_rows.len() as u64;
            Ok(ResultSet { columns: out_names, rows: out_rows, affected_rows: n, last_insert_key: None })
        }
```

(The parser already rejects negative `ONLY`/`SKIP` literals and `SKIP` without `ONLY` — `SkipWithoutOnly` — so `.max(0)` here is just defensive, not load-bearing.)

- [ ] **Step 5: Run the tests**

Run: `cargo test`
Expected: all pass.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/sous_chef/mod.rs src/stove/executor.rs tests/executor.rs
git commit -m "feat: ONLY / SKIP (LIMIT / OFFSET), applied after ordering"
```

---

### Task 5: `TOSS` (inner equi-join)

**Files:**
- Modify: `src/sous_chef/mod.rs` (build `JoinPlan` from `sel.join`, remove the join rejection, update its test)
- Modify: `src/stove/executor.rs` (`scan_join` helper, wire into `execute()`)
- Modify: `tests/executor.rs` (replace the stale `test_join_returns_clause_not_yet_supported`)

**Interfaces:**
- Consumes: `Schema`, `column_value`, `values_equal` (existing) from Task 1; `JoinPlan` struct from Task 2.
- Produces: `fn scan_join(pot: &mut Pot, from: &TableRef, j: &JoinPlan) -> Result<(Schema, Vec<Vec<Value>>), Scottatura>`.

- [ ] **Step 1: Write the failing integration test**

Replace `test_join_returns_clause_not_yet_supported` in `tests/executor.rs` with:

```rust
#[test]
fn test_inner_join_combines_matching_rows() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT);").unwrap();
    pot.execute("PLATE noodle orders (user_id INT, total REAL);").unwrap();
    pot.execute("LADLE INTO users (name) PORTIONS ('Tony'), ('Maria');").unwrap(); // ids 1, 2
    pot.execute("LADLE INTO orders (user_id, total) PORTIONS (1, 12.5), (1, 3.0), (2, 7.0);").unwrap();

    let r = pot.execute("SERVE u.name, o.total FROM users u TOSS orders o ON u.id = o.user_id STRAINED BY u.name = 'Tony';").unwrap();
    assert_eq!(r.rows.len(), 2);
    for row in &r.rows {
        assert_eq!(row[0], Value::Text("Tony".to_string()));
    }
}

#[test]
fn test_join_unqualified_ambiguous_column_errors() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT);").unwrap();
    pot.execute("PLATE noodle orders (user_id INT);").unwrap();
    // both sides have an `id` column (the Al Dente Key) -- unqualified `id` in STRAINED BY is ambiguous.
    let err = pot.execute("SERVE * FROM users u TOSS orders o ON u.id = o.user_id STRAINED BY id = 1;").unwrap_err();
    assert!(matches!(err, Scottatura::AmbiguousColumn { name } if name == "id"));
    std::fs::remove_file(&path).ok();
}
```

(Remove `std::fs::remove_file(&path).ok();` duplication concerns — each test still owns its own `temp_path()`; the first test above is missing its cleanup call, add `std::fs::remove_file(&path).ok();` at the end of `test_inner_join_combines_matching_rows` too.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test executor test_inner_join_combines_matching_rows test_join_unqualified_ambiguous_column_errors`
Expected: FAIL — `plan()` still returns `ClauseNotYetSupported { clause: "TOSS (join)" }`.

- [ ] **Step 3: Build `JoinPlan` in `sous_chef::plan` and remove the join rejection**

In `src/sous_chef/mod.rs`, update the imports to include `JoinClause`:

```rust
use crate::parser::ast::{ColumnSpec, Expr, JoinClause, OrderByItem, SelectItem, Statement, TableRef};
```

Replace:

```rust
            if sel.join.is_some() {
                return Err(Scottatura::ClauseNotYetSupported { clause: "TOSS (join)".to_string() });
            }
```

with:

```rust
            let join = sel.join.map(|j: JoinClause| JoinPlan {
                table: j.table,
                left_col: j.left_col,
                right_col: j.right_col,
            });
```

and update the `Plan::Select { .. }` construction at the end of the `Statement::Select` arm to use `join` instead of `join: None`.

Update the existing `test_join_rejected` unit test to its positive counterpart:

```rust
    #[test]
    fn test_join_passthrough() {
        let stmt = parse_statement("SERVE * FROM users u TOSS orders o ON u.id = o.user_id;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { join: Some(j), .. } if j.table.name == "orders"));
    }
```

- [ ] **Step 4: Implement `scan_join` in `src/stove/executor.rs`**

Add near the other pipeline helpers (before `execute()`), and import `JoinPlan`:

```rust
use crate::sous_chef::{JoinPlan, Plan};
```

(replace the existing `use crate::sous_chef::Plan;` line with the above)

```rust
/// Nested-loop inner equi-join. Resolves both join-condition columns
/// against the *combined* row/schema (not each side separately) so that
/// `ON a.x = b.y` and `ON b.y = a.x` both work regardless of which side
/// `left_col`/`right_col` names, and so qualifier ambiguity is caught the
/// same way it is for any other column reference.
fn scan_join(pot: &mut Pot, from: &TableRef, j: &JoinPlan) -> Result<(Schema, Vec<Vec<Value>>), Scottatura> {
    let left_recipe = pot.recipe(&from.name)?;
    let left_owner = from.alias.clone().unwrap_or_else(|| from.name.clone());
    let left_schema = full_schema(&left_owner, &left_recipe.columns);
    let left_rows: Vec<Vec<Value>> = pot.scan_noodle(&from.name)?
        .into_iter().map(|(k, v)| full_row(k, &v)).collect();

    let right_recipe = pot.recipe(&j.table.name)?;
    let right_owner = j.table.alias.clone().unwrap_or_else(|| j.table.name.clone());
    let right_schema = full_schema(&right_owner, &right_recipe.columns);
    let right_rows: Vec<Vec<Value>> = pot.scan_noodle(&j.table.name)?
        .into_iter().map(|(k, v)| full_row(k, &v)).collect();

    let mut schema = left_schema;
    schema.extend(right_schema);

    let mut out = Vec::new();
    for lrow in &left_rows {
        for rrow in &right_rows {
            let mut combined = lrow.clone();
            combined.extend(rrow.iter().cloned());
            let lv = column_value(j.left_col.0.as_deref(), &j.left_col.1, &combined, &schema)?;
            let rv = column_value(j.right_col.0.as_deref(), &j.right_col.1, &combined, &schema)?;
            if matches!(lv, Value::Null) || matches!(rv, Value::Null) {
                continue;
            }
            if values_equal(&lv, &rv)? {
                out.push(combined);
            }
        }
    }
    Ok((schema, out))
}
```

- [ ] **Step 5: Wire `join` into the `Plan::Select` match arm**

Replace the scan portion of the `Plan::Select` arm — the part that builds `schema`/`raw_rows` — to branch on `join`:

```rust
        Plan::Select { from, join, filter, group_by: _, columns, order_by, limit, offset } => {
            let (schema, rows): (Schema, Vec<Vec<Value>>) = if let Some(j) = &join {
                scan_join(pot, &from, j)?
            } else {
                let recipe = pot.recipe(&from.name)?;
                let owner = from.alias.clone().unwrap_or_else(|| from.name.clone());
                let s = full_schema(&owner, &recipe.columns);
                let r = pot.scan_noodle(&from.name)?.into_iter().map(|(k, v)| full_row(k, &v)).collect();
                (s, r)
            };
            let out_names = project_names(&columns, &schema);
            let mut out_rows = Vec::new();
            for row in &rows {
                if let Some(f) = &filter {
                    if !eval_tri(f, row, &schema)?.is_true() {
                        continue;
                    }
                }
                out_rows.push(project_values(&columns, &schema, row)?);
            }
            if !order_by.is_empty() {
                sort_rows(&mut out_rows, &order_by, &out_names)?;
            }
            let skip_n = offset.unwrap_or(0).max(0) as usize;
            out_rows = out_rows.into_iter().skip(skip_n).collect();
            if let Some(l) = limit {
                out_rows.truncate(l.max(0) as usize);
            }
            let n = out_rows.len() as u64;
            Ok(ResultSet { columns: out_names, rows: out_rows, affected_rows: n, last_insert_key: None })
        }
```

Note the non-join branch now also honors `from.alias` as the schema owner (e.g. `SERVE * FROM users u STRAINED BY u.name = 'Tony'` now resolves `u.name` correctly) — this is a strict superset of prior behavior since unaliased single-table queries are unaffected (`owner` falls back to `from.name`, same as Task 2).

- [ ] **Step 6: Run the tests**

Run: `cargo test`
Expected: all pass, including the two new join tests.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add src/sous_chef/mod.rs src/stove/executor.rs tests/executor.rs
git commit -m "feat: TOSS (inner equi-join) via nested-loop scan"
```

---

### Task 6: `BUNDLED BY` (GROUP BY) and aggregate functions

**Files:**
- Modify: `src/lib.rs` (two new Scottatura variants)
- Modify: `src/sous_chef/mod.rs` (aggregate-context validation, remove the `group_by` rejection, update its test)
- Modify: `src/stove/executor.rs` (`build_groups`, `project_grouped`, `eval_aggregate`, wire into `execute()`)
- Modify: `tests/executor.rs` (remove the now-fully-obsolete combined rejection test; add GROUP BY/aggregate tests)

**Interfaces:**
- Consumes: `Schema`, `column_value`, `compare_values`, `Plan::Select.group_by` from earlier tasks.
- Produces: nothing consumed by later tasks — this is the last plan-shape feature.

- [ ] **Step 1: Write the failing integration tests**

In `tests/executor.rs`, delete `test_bundled_by_still_returns_clause_not_yet_supported` entirely (from Task 4 — there's nothing left for it to assert once this task lands) and add:

```rust
#[test]
fn test_group_by_with_aggregates() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle orders (customer TEXT, total REAL);").unwrap();
    pot.execute("LADLE INTO orders (customer, total) PORTIONS ('Tony', 10.0), ('Tony', 5.0), ('Maria', 20.0);").unwrap();

    let r = pot.execute("SERVE customer, COUNT(*), SUM(total) FROM orders BUNDLED BY customer ARRANGED BY customer;").unwrap();
    assert_eq!(r.rows, vec![
        vec![Value::Text("Maria".to_string()), Value::Integer(1), Value::Real(20.0)],
        vec![Value::Text("Tony".to_string()), Value::Integer(2), Value::Real(15.0)],
    ]);
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_ungrouped_aggregate_over_empty_result_yields_zero_count() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle orders (total REAL);").unwrap();
    let r = pot.execute("SERVE COUNT(*), SUM(total) FROM orders;").unwrap();
    assert_eq!(r.rows, vec![vec![Value::Integer(0), Value::Null]]);
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_select_star_in_aggregate_context_rejected() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle orders (customer TEXT);").unwrap();
    let err = pot.execute("SERVE * FROM orders BUNDLED BY customer;").unwrap_err();
    assert!(matches!(err, Scottatura::ClauseNotYetSupported { .. }));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_ungrouped_column_in_aggregate_context_rejected() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle orders (customer TEXT, total REAL);").unwrap();
    let err = pot.execute("SERVE customer, total, COUNT(*) FROM orders BUNDLED BY customer;").unwrap_err();
    assert!(matches!(err, Scottatura::ColumnNotGroupedOrAggregated { name } if name == "total"));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_unknown_aggregate_function_rejected() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle orders (total REAL);").unwrap();
    let err = pot.execute("SERVE MEDIAN(total) FROM orders;").unwrap_err();
    assert!(matches!(err, Scottatura::UnknownAggregateFunction { name } if name == "MEDIAN"));
    std::fs::remove_file(&path).ok();
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test executor test_group_by_with_aggregates test_ungrouped_aggregate_over_empty_result_yields_zero_count test_select_star_in_aggregate_context_rejected test_ungrouped_column_in_aggregate_context_rejected test_unknown_aggregate_function_rejected`
Expected: FAIL — `plan()` still rejects `BUNDLED BY`, and `Scottatura::ColumnNotGroupedOrAggregated`/`UnknownAggregateFunction` don't exist yet.

- [ ] **Step 3: Add the two new Scottatura variants**

In `src/lib.rs`, add to `pub enum Scottatura`:

```rust
    #[error("Scottatura [RICETTA_011]: Ingredient `{name}` is neither in BUNDLED BY nor wrapped in an aggregate function.")]
    ColumnNotGroupedOrAggregated { name: String },

    #[error("Scottatura [EXEC_004]: Unknown aggregate function `{name}`. Supported: COUNT, SUM, AVG, MIN, MAX.")]
    UnknownAggregateFunction { name: String },
```

- [ ] **Step 4: Add aggregate-context validation to `sous_chef::plan`**

In `src/sous_chef/mod.rs`, add helper functions (module-level, above `plan()`):

```rust
fn item_is_aggregate(item: &SelectItem) -> bool {
    matches!(item, SelectItem::Expr { expr: Expr::FunctionCall { .. }, .. })
}

fn validate_aggregate_context(sel: &crate::parser::ast::SelectStatement) -> Result<(), Scottatura> {
    let is_aggregate_context = !sel.group_by.is_empty() || sel.items.iter().any(item_is_aggregate);
    if !is_aggregate_context {
        return Ok(());
    }
    for item in &sel.items {
        match item {
            SelectItem::Star => {
                return Err(Scottatura::ClauseNotYetSupported { clause: "* with BUNDLED BY/aggregates".to_string() });
            }
            SelectItem::Expr { expr: Expr::FunctionCall { name, .. }, .. } => {
                if !matches!(name.to_uppercase().as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX") {
                    return Err(Scottatura::UnknownAggregateFunction { name: name.clone() });
                }
            }
            SelectItem::Expr { expr: Expr::Column { name, .. }, .. } => {
                if !sel.group_by.iter().any(|g| g == name) {
                    return Err(Scottatura::ColumnNotGroupedOrAggregated { name: name.clone() });
                }
            }
            SelectItem::Expr { .. } => {
                return Err(Scottatura::ColumnNotGroupedOrAggregated { name: "<expression>".to_string() });
            }
        }
    }
    Ok(())
}
```

Call it at the top of the `Statement::Select` arm, right after the (now-empty of the group_by check) existing rejections, and delete the `group_by` rejection block:

```rust
        Statement::Select(sel) => {
            validate_aggregate_context(&sel)?;
            let join = sel.join.map(|j: JoinClause| JoinPlan {
                table: j.table,
                left_col: j.left_col,
                right_col: j.right_col,
            });
            Ok(Plan::Select {
                from: sel.from,
                join,
                filter: sel.filter,
                group_by: sel.group_by,
                columns: sel.items,
                order_by: sel.order_by,
                limit: sel.limit,
                offset: sel.offset,
            })
        }
```

(By this point in the plan, the `order_by`/`limit`/`offset`/`join` rejections have already been removed in Tasks 3–5, so this arm should now have no `ClauseNotYetSupported` checks left except the ones inside `validate_aggregate_context`.)

Update the existing `test_bundled_by_rejected`-style unit test (if present) to its positive counterpart plus a validation-failure case:

```rust
    #[test]
    fn test_group_by_passthrough() {
        let stmt = parse_statement("SERVE age, COUNT(*) FROM users BUNDLED BY age;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { group_by, .. } if group_by == vec!["age".to_string()]));
    }

    #[test]
    fn test_star_with_group_by_rejected_at_plan_time() {
        let stmt = parse_statement("SERVE * FROM users BUNDLED BY age;").unwrap();
        assert!(matches!(plan(stmt).unwrap_err(), Scottatura::ClauseNotYetSupported { .. }));
    }
```

- [ ] **Step 5: Implement grouping and aggregate evaluation in `src/stove/executor.rs`**

Add near the other pipeline helpers:

```rust
fn item_has_aggregate(item: &SelectItem) -> bool {
    matches!(item, SelectItem::Expr { expr: Expr::FunctionCall { .. }, .. })
}

/// Buckets rows by their `group_by` column values. Empty `group_by` means
/// exactly one implicit group covering all rows (even zero rows) -- this
/// is what makes `SELECT COUNT(*) FROM t` with no matches yield one row
/// with `COUNT(*) = 0` rather than an empty result set.
fn build_groups(group_by: &[String], schema: &Schema, rows: Vec<Vec<Value>>) -> Result<Vec<(Vec<Value>, Vec<Vec<Value>>)>, Scottatura> {
    if group_by.is_empty() {
        return Ok(vec![(vec![], rows)]);
    }
    let mut groups: Vec<(Vec<Value>, Vec<Vec<Value>>)> = Vec::new();
    for row in rows {
        let key: Vec<Value> = group_by.iter()
            .map(|name| column_value(None, name, &row, schema))
            .collect::<Result<_, _>>()?;
        if let Some((_, bucket)) = groups.iter_mut().find(|(k, _)| *k == key) {
            bucket.push(row);
        } else {
            groups.push((key, vec![row]));
        }
    }
    Ok(groups)
}

fn eval_aggregate(name: &str, arg: Option<&Expr>, group_rows: &[Vec<Value>], schema: &Schema) -> Result<Value, Scottatura> {
    match name.to_uppercase().as_str() {
        "COUNT" => match arg {
            None => Ok(Value::Integer(group_rows.len() as i64)),
            Some(e) => {
                let mut n = 0i64;
                for row in group_rows {
                    if !matches!(eval_scalar(e, row, schema)?, Value::Null) {
                        n += 1;
                    }
                }
                Ok(Value::Integer(n))
            }
        },
        "SUM" | "AVG" => {
            let e = arg.ok_or_else(|| Scottatura::UnknownAggregateFunction { name: name.to_string() })?;
            let (mut sum, mut count, mut is_int) = (0f64, 0i64, true);
            for row in group_rows {
                match eval_scalar(e, row, schema)? {
                    Value::Null => {}
                    Value::Integer(i) => { sum += i as f64; count += 1; }
                    Value::Real(r) => { sum += r; count += 1; is_int = false; }
                    other => return Err(Scottatura::TypeMismatch {
                        got: format!("{other:?}"), col: "<aggregate>".to_string(), expected: "INTEGER or REAL".to_string(),
                    }),
                }
            }
            if count == 0 {
                return Ok(Value::Null);
            }
            if name.eq_ignore_ascii_case("SUM") {
                Ok(if is_int { Value::Integer(sum as i64) } else { Value::Real(sum) })
            } else {
                Ok(Value::Real(sum / count as f64))
            }
        }
        "MIN" | "MAX" => {
            let e = arg.ok_or_else(|| Scottatura::UnknownAggregateFunction { name: name.to_string() })?;
            let mut best: Option<Value> = None;
            for row in group_rows {
                let v = eval_scalar(e, row, schema)?;
                if matches!(v, Value::Null) {
                    continue;
                }
                best = Some(match best {
                    None => v,
                    Some(b) => {
                        let ord = compare_values(&v, &b)?;
                        let want_v = (name.eq_ignore_ascii_case("MIN") && ord == Ordering::Less)
                            || (name.eq_ignore_ascii_case("MAX") && ord == Ordering::Greater);
                        if want_v { v } else { b }
                    }
                });
            }
            Ok(best.unwrap_or(Value::Null))
        }
        other => Err(Scottatura::UnknownAggregateFunction { name: other.to_string() }),
    }
}

fn project_grouped(items: &[SelectItem], group_by: &[String], schema: &Schema, key: &[Value], rows: &[Vec<Value>]) -> Result<Vec<Value>, Scottatura> {
    let mut out = Vec::new();
    for item in items {
        match item {
            SelectItem::Expr { expr: Expr::FunctionCall { name, arg }, .. } => {
                out.push(eval_aggregate(name, arg.as_deref(), rows, schema)?);
            }
            SelectItem::Expr { expr: Expr::Column { name, .. }, .. } => {
                let idx = group_by.iter().position(|g| g == name)
                    .ok_or_else(|| Scottatura::ColumnNotGroupedOrAggregated { name: name.clone() })?;
                out.push(key[idx].clone());
            }
            // sous_chef::validate_aggregate_context already rejects every other shape
            // (Star, arbitrary expressions) before this code runs in aggregate context.
            _ => return Err(Scottatura::ColumnNotGroupedOrAggregated { name: "<expression>".to_string() }),
        }
    }
    Ok(out)
}
```

- [ ] **Step 6: Wire grouping into the `Plan::Select` match arm**

Change the projection step to branch on whether the query is in aggregate context:

```rust
        Plan::Select { from, join, filter, group_by, columns, order_by, limit, offset } => {
            let (schema, rows): (Schema, Vec<Vec<Value>>) = if let Some(j) = &join {
                scan_join(pot, &from, j)?
            } else {
                let recipe = pot.recipe(&from.name)?;
                let owner = from.alias.clone().unwrap_or_else(|| from.name.clone());
                let s = full_schema(&owner, &recipe.columns);
                let r = pot.scan_noodle(&from.name)?.into_iter().map(|(k, v)| full_row(k, &v)).collect();
                (s, r)
            };
            let mut filtered = Vec::new();
            for row in rows {
                if let Some(f) = &filter {
                    if !eval_tri(f, &row, &schema)?.is_true() {
                        continue;
                    }
                }
                filtered.push(row);
            }
            let out_names = project_names(&columns, &schema);
            let is_aggregate_context = !group_by.is_empty() || columns.iter().any(item_has_aggregate);
            let mut out_rows = if is_aggregate_context {
                let groups = build_groups(&group_by, &schema, filtered)?;
                groups.iter()
                    .map(|(key, rows)| project_grouped(&columns, &group_by, &schema, key, rows))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                filtered.iter()
                    .map(|row| project_values(&columns, &schema, row))
                    .collect::<Result<Vec<_>, _>>()?
            };
            if !order_by.is_empty() {
                sort_rows(&mut out_rows, &order_by, &out_names)?;
            }
            let skip_n = offset.unwrap_or(0).max(0) as usize;
            out_rows = out_rows.into_iter().skip(skip_n).collect();
            if let Some(l) = limit {
                out_rows.truncate(l.max(0) as usize);
            }
            let n = out_rows.len() as u64;
            Ok(ResultSet { columns: out_names, rows: out_rows, affected_rows: n, last_insert_key: None })
        }
```

Note `project_names` still runs the old (non-grouped) logic against `columns`/`schema` — that's fine for aggregate context too, since `FunctionCall` items without an alias fall into the `_ => format!("expr{}", i + 1)` branch already, giving output names like `expr2` for unaliased aggregates (e.g. `COUNT(*)`), and `Star` is already excluded from aggregate context by plan-time validation.

- [ ] **Step 7: Run the tests**

Run: `cargo test`
Expected: all pass, including the five new tests from Step 1.

- [ ] **Step 8: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add src/lib.rs src/sous_chef/mod.rs src/stove/executor.rs tests/executor.rs
git commit -m "feat: BUNDLED BY (GROUP BY) and COUNT/SUM/AVG/MIN/MAX aggregates"
```

---

### Task 7: Combined end-to-end coverage, CHANGELOG, final sweep

**Files:**
- Modify: `tests/executor.rs` (one combined multi-feature test)
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: everything from Tasks 1–6 via `Pot::execute`.

- [ ] **Step 1: Write a combined end-to-end test exercising join + group + order + limit together**

In `tests/executor.rs`:

```rust
#[test]
fn test_join_group_order_limit_combined() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle customers (name TEXT);").unwrap();
    pot.execute("PLATE noodle orders (customer_id INT, total REAL);").unwrap();
    pot.execute("LADLE INTO customers (name) PORTIONS ('Tony'), ('Maria'), ('Gino');").unwrap(); // ids 1,2,3
    pot.execute("LADLE INTO orders (customer_id, total) PORTIONS (1, 10.0), (1, 5.0), (2, 100.0), (3, 1.0);").unwrap();

    // `BUNDLED BY` only accepts a single bare identifier (src/parser/mod.rs's
    // group_by parsing calls `expect_ident()`, which does not accept dotted
    // `table.column` syntax) -- use the unqualified column name. It's
    // unambiguous here because only `customers` has a `name` column.
    let r = pot.execute(
        "SERVE c.name, SUM(o.total) FROM customers c TOSS orders o ON c.id = o.customer_id \
         BUNDLED BY name ARRANGED BY name ONLY 2;"
    ).unwrap();
    assert_eq!(r.rows, vec![
        vec![Value::Text("Gino".to_string()), Value::Real(1.0)],
        vec![Value::Text("Maria".to_string()), Value::Real(100.0)],
    ]);
    std::fs::remove_file(&path).ok();
}
```

`group_by` entries are always bare, unqualified names (confirmed from the parser), and `Expr::Column.name` is likewise always the bare name (its qualifier lives in the separate `qualifier` field) — so `validate_aggregate_context`'s `sel.group_by.iter().any(|g| g == name)` and `project_grouped`'s `group_by.iter().position(|g| g == name)` from Task 6 need no adjustment; they already compare bare names to bare names correctly.

- [ ] **Step 2: Run the test**

Run: `cargo test --test executor test_join_group_order_limit_combined`
Expected: PASS.

- [ ] **Step 3: Update CHANGELOG.md**

Add a new bullet list under the existing `## [Unreleased]` → `### Added` section in `CHANGELOG.md` (do not create a new `## [Unreleased]` heading — one already exists):

```markdown
- `TOSS` (inner equi-join) — nested-loop join with qualifier-aware column resolution (`table.column`) and ambiguous-unqualified-column detection
- `ARRANGED BY` (ORDER BY) — multi-key sort with VUOTO always sorting first regardless of ASC/DESC
- `ONLY` / `SKIP` (LIMIT / OFFSET), applied after ordering
- `BUNDLED BY` (GROUP BY) and aggregate functions `COUNT`/`SUM`/`AVG`/`MIN`/`MAX`, with strict validation rejecting `SELECT *` and ungrouped/unaggregated columns in aggregate-context queries
- `AmbiguousColumn`, `ColumnNotGroupedOrAggregated`, `UnknownAggregateFunction` Scottatura error variants (RICETTA_010, RICETTA_011, EXEC_004)
```

- [ ] **Step 4: Run the full test suite and clippy one final time**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo build`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add tests/executor.rs CHANGELOG.md
git commit -m "test: combined join+group+order+limit coverage; update CHANGELOG for Phase 2c"
```
