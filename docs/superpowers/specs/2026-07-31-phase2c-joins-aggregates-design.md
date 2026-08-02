# Phase 2c: Joins, Ordering, Limiting, Grouping & Aggregates — Design

## Goal

Wire up the query features the Phase 2a parser already accepts but Phase 2b's planner rejects: `TOSS` (inner equi-join), `ARRANGED BY` (ORDER BY), `ONLY`/`SKIP` (LIMIT/OFFSET), `BUNDLED BY` (GROUP BY), and aggregate functions (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX`). This closes out v1's read-side query scope (transactions and Strainer indexes remain Phase 3).

The parser (`src/parser/ast.rs`) already produces `SelectStatement.join`, `.group_by`, `.order_by`, `.limit`, `.offset`, and `Expr::FunctionCall` — none of this reaches the executor today; `sous_chef::plan` rejects all of it with `ClauseNotYetSupported`. It also already rejects, at parse time, non-equi joins, outer joins, multiple joins, and `HAVING` (`OuterTossNotSupported`, `NonEquiJoinNotSupported`, `MultipleTossNotSupported`, `HavingNotSupported`) — so the planner/executor only ever need to handle a single inner equi-join.

## Architecture: pipeline

`Plan::SeqScan` is replaced by `Plan::Select`, carrying the whole query. Execution runs a standard pipeline: **join → filter → group/aggregate → project → order → limit/offset**.

```rust
// src/sous_chef/mod.rs
pub struct JoinPlan {
    pub table: TableRef,
    pub left_col: (Option<String>, String),
    pub right_col: (Option<String>, String),
}

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
    Insert { .. }, Update { .. }, Delete { .. }, CreateTable { .. }, DropTable { .. }, // unchanged
}
```

`sous_chef::plan` becomes a pass-through translation for `Statement::Select` (no more clause rejection for join/group_by/order_by/limit/offset — those move to plan-time *validation* instead, see below).

## Qualifier-aware schema (executor refactor)

Today `stove/executor.rs` resolves columns via `column_value(name, row, schema: &[ColumnDef])`, ignoring `Expr::Column`'s `qualifier` field entirely — fine for single-table queries. Joins require resolving `u.name` vs bare `name` across two tables' schemas, including detecting ambiguity.

New schema type:

```rust
pub(crate) struct SchemaColumn {
    pub owner: String,   // table name or join alias
    pub col: ColumnDef,
}
pub(crate) type Schema = Vec<SchemaColumn>;
```

`full_schema`/`full_row` are updated to tag columns with an `owner` (the table name, or alias if `TableRef.alias` is set). `column_value(qualifier: Option<&str>, name: &str, row, schema: &Schema)`:
- `qualifier = Some(q)`: match `owner == q && col.name == name`; `ColumnNotFound` if none.
- `qualifier = None`: match all entries where `col.name == name`; zero → `ColumnNotFound`; exactly one → that value; more than one → `AmbiguousColumn { name }` (new error).

Every `eval_scalar`/`eval_tri` call site in `stove/executor.rs` moves from `&[ColumnDef]` to `&Schema`. Single-table queries just build a one-table `Schema`, so this is additive complexity, not a behavior change, for existing non-join queries.

## Join execution

Nested-loop join (this is a toy DB; no join index selection). For `Plan::Select` with `join: Some(JoinPlan)`:

1. Scan all rows of `from.table` → `(schema_l, rows_l)`; scan all rows of `join.table` → `(schema_r, rows_r)`.
2. Combined schema = `schema_l ++ schema_r` (owners: `from.alias.unwrap_or(from.name)` / `join.table.alias.unwrap_or(join.table.name)`).
3. For each `(row_l, row_r)` pair, resolve `left_col`/`right_col` against the combined schema and compare with `values_equal`; VUOTO on either side excludes the pair (matches WHERE's three-valued semantics — an equi-join condition is just an implicit `AND` in `WHERE` terms).
4. Matching pairs become combined rows, fed into the rest of the pipeline (filter → group → project → order → limit/offset) unchanged.

`SelectItem::Star` in a joined query expands to all columns from both schemas in scan order (left table's columns, including `id`, then right table's) — duplicate names (e.g. both tables have `id`) are allowed in `*` output, matching normal SQL behavior; only *explicit unqualified* references are subject to the ambiguity check above.

## GROUP BY + aggregates

A query is in **aggregate context** if `group_by` is non-empty OR any `SelectItem` contains `Expr::FunctionCall`. Plan-time validation (in a new `sous_chef` or executor-side validation step, run before execution) in aggregate context:

- `SelectItem::Star` → `ClauseNotYetSupported { clause: "* with BUNDLED BY/aggregates" }`.
- Every non-aggregate `SelectItem::Expr` must be exactly `Expr::Column { qualifier, name }` where `name` is present in `group_by` — anything else (arithmetic, a plain column not in the group list) → `ColumnNotGroupedOrAggregated { name }` (new error variant). This is intentionally stricter than SQLite; out-of-scope expressions can be added in a later phase if needed.
- `Expr::FunctionCall { name, .. }` where `name` is not one of `COUNT`/`SUM`/`AVG`/`MIN`/`MAX` (case-insensitive) → `UnknownAggregateFunction { name }` (new error variant).

Execution: after filter, bucket surviving rows by evaluating `group_by` columns against the schema (empty `group_by` ⇒ single implicit group covering all filtered rows, even if that's zero rows — e.g. `SERVE COUNT(*) FROM t STRAINED BY 1=0` still yields one row with `COUNT(*) = 0`, matching standard SQL). For each group, evaluate every `SelectItem`:
- Plain `Column` → take the value from the group's first row (well-defined: it's constrained to be a `group_by` column, so it's constant across the group).
- `FunctionCall { name: "COUNT", arg: None }` → row count in group.
- `FunctionCall { name: "COUNT", arg: Some(e) }` → count of rows where `eval_scalar(e, ...)` is non-VUOTO.
- `SUM`/`AVG` → accumulate over non-VUOTO `eval_scalar` results, requiring `Integer`/`Real` (else `TypeMismatch`); VUOTO result if the group has zero non-VUOTO values. `AVG` = sum / count-of-non-null as `Real`.
- `MIN`/`MAX` → fold with `compare_values`, skipping VUOTO; VUOTO result if all values were VUOTO.

One output row per group.

## ORDER BY

Multi-key stable sort (`Vec<OrderByItem>`, later keys break ties on earlier keys). Per-key comparator: VUOTO always sorts first, regardless of `descending`. When both values are non-VUOTO: ascending uses `compare_values` directly; descending reverses it. Applied to the *projected* rows using the projected column's position (an `ORDER BY` column must appear in the projection or `group_by`/schema — if not resolvable, `ColumnNotFound`).

## LIMIT / OFFSET

Applied last: `offset` (default 0) rows are skipped, then up to `limit` rows are kept. `offset` beyond the row count yields an empty result; `limit: Some(0)` yields an empty result. No new errors — negative values are already rejected by the parser.

## New Scottatura variants

- `AmbiguousColumn { name: String }` — unqualified column matches more than one side of a join.
- `ColumnNotGroupedOrAggregated { name: String }` — aggregate-context select item is neither grouped nor aggregated.
- `UnknownAggregateFunction { name: String }` — `FunctionCall` name isn't `COUNT`/`SUM`/`AVG`/`MIN`/`MAX`.

## Testing

- Unit tests (`stove/executor.rs`): qualifier-aware `column_value` (qualified, unqualified-unique, unqualified-ambiguous), join row-combination, each aggregate function including all-VUOTO groups and `COUNT(*)` vs `COUNT(col)`, VUOTO-first ordering in both ASC/DESC, LIMIT/OFFSET boundary cases (offset past end, limit 0).
- Integration tests (`tests/executor.rs`), extending the existing CRUD-lifecycle style: a two-table join end-to-end (`users`/`orders`-style fixture), a grouped-aggregate query end-to-end, a combined `ARRANGED BY ... ONLY n SKIP m` end-to-end, rejection paths for the three new error variants and for `SELECT *` in aggregate context.
- `cargo test`, `cargo build`, `cargo clippy --all-targets -- -D warnings` all clean, consistent with prior phases' self-review checklist.
