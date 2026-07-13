# Phase 2b: Sous Chef Planner + Stove Executor — Design

## Goal

Add `Pot::execute(sauql: &str) -> Result<ResultSet, Scottatura>`, wiring the Phase 2a parser to a new planner and executor so that single-table `SERVE`, `LADLE INTO`, `SEASON`, and `DISCARD FROM` (with `STRAINED BY`) run end-to-end against a real `.pot` file.

Out of scope for this phase (deferred to Phase 2c unless noted): joins, `ARRANGED BY`, `ONLY`/`SKIP`, `BUNDLED BY`, aggregate functions. Deferred to a later phase: transaction execution (`BEGIN SIMMER`/`FINISH SIMMER`/`SPILL SIMMER`) pending the Prep Log (WAL), and Strainer index creation/use (Phase 3).

## Architecture

```
SaucQL string
  -> parser::parse_statement()   [existing, Phase 2a]
  -> sous_chef::plan()           [new] ast::Statement -> Plan
  -> stove::executor::execute()  [new] Plan + &mut Pot -> ResultSet
```

`Pot::execute` is the single public entry point that chains these three steps.

## Sous Chef: Plan (`src/sous_chef/mod.rs`)

```rust
pub enum Plan {
    SeqScan { table: String, columns: Vec<SelectItem>, filter: Option<Expr> },
    Insert { table: String, columns: Vec<String>, rows: Vec<Vec<Expr>> },
    Update { table: String, assignments: Vec<(String, Expr)>, filter: Expr },
    Delete { table: String, filter: Expr },
    CreateTable { name: String, columns: Vec<ColumnSpec> },
    DropTable { name: String },
    CreateIndex { index_name: String, table: String, column: String },
    DropIndex { index_name: String },
}

pub fn plan(stmt: Statement) -> Result<Plan, Scottatura>;
```

`plan()` is a thin translation layer today (one variant per statement kind, mostly pass-through), but it is the seam where Phase 3 adds `Plan::IndexScan` and chooses between it and `SeqScan`.

Rejections at plan time:
- `Select` with `join.is_some()`, non-empty `order_by`, `limit.is_some()`, `offset.is_some()`, or non-empty `group_by` → `Scottatura::ClauseNotYetSupported { clause }` naming the specific clause.
- `BeginSimmer` / `FinishSimmer` / `SpillSimmer` → `Scottatura::TransactionNotSupported`.
- `CreateIndex` / `DropIndex` → `Scottatura::StrainerNotSupported` (Strainer is a Phase 3 stub).

## Stove Executor (`src/stove/executor.rs`, new module)

### Expression evaluation

`fn eval(expr: &Expr, row: &[Value], schema: &[ColumnDef]) -> Result<Value, Scottatura>`

Implements every operator in `docs/sql-semantics.md` §"STRAINED BY Expressions": `=`, `!=`, `<`, `>`, `<=`, `>=`, `AND`, `OR`, `NOT`, `IS VUOTO`/`IS NOT VUOTO`, `BETWEEN`, `LIKE` (`%`/`_` wildcards), `IN`. VUOTO is three-valued: any comparison or arithmetic touching VUOTO produces VUOTO, and a VUOTO filter result excludes the row (matches only on `true`). `INTEGER` promotes to `REAL` in mixed arithmetic/comparison; all other type mismatches are `Scottatura::TypeMismatch`.

### SeqScan

Walks the heap for `table`, following `next_page` pointers (heap becomes genuinely multi-page in this phase — `heap_insert` gains page-allocation-and-link-on-full instead of erroring). For each live (non-tombstoned) row: evaluate `filter` if present, keep only rows where it evaluates to `true`; project `columns`, expanding `SelectItem::Star` to all schema columns in declaration order (Al Dente Key first) and applying aliases.

### Insert

Resolves the statement's explicit column list against the schema — order may differ from declaration order. Any schema column omitted from the list is filled with `Value::Null` (checked against `NOT VUOTO` same as an explicit VUOTO). Rejects `id` appearing in the column list (`RICETTA_006`). Evaluates each value `Expr` (literals and arithmetic only — no column references make sense in this position) via the same `eval` used for filters. Delegates to `heap_insert`, extended to allocate and link a new heap page when the current one is full.

### Update

Scans + filters as in SeqScan (filter is required — enforced at parse time already). For each matched row: evaluate the new values for assigned columns, tombstone the old slot, insert a new slot with the merged row (unassigned columns keep their old value). Rejects assignment to `id` — new `Scottatura::AlDenteKeyNotUpdatable` variant (RICETTA_005, referenced in `docs/error-taxonomy.md` but not yet defined in the enum).

### Delete

Scans + filters, tombstones matched slots via the existing `heap_delete`.

### DDL passthrough

`Plan::CreateTable`/`DropTable` call the existing `Pot::create_table`/`drop_table`. `Plan::CreateIndex`/`DropIndex` are unreachable in this phase (rejected at plan time) but kept in the enum for Phase 3.

## New Scottatura variants

- `ClauseNotYetSupported { clause: String }`
- `TransactionNotSupported`
- `StrainerNotSupported`
- `AlDenteKeyNotUpdatable` (RICETTA_005)

## Testing

- Unit tests in `src/stove/executor.rs` for each evaluator operator, VUOTO three-valued logic, and type coercion.
- Integration tests in `tests/executor.rs`: full CRUD lifecycle through `Pot::execute` (create → insert with a reordered/partial column list → update → delete → drop); multi-page insert (enough rows to force a heap page split, verified via scan); deferred-clause error paths (join, `ARRANGED BY`, `ONLY`, `BUNDLED BY`, transactions, `STRAIN`/`REMOVE STRAINER`).
- `cargo test`, `cargo build`, `cargo clippy --all-targets -- -D warnings` all clean, consistent with the Phase 2a self-review checklist.
