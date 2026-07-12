# SaucQL Semantics

MarinaraDB uses SaucQL — a custom query language with pasta-themed keywords and standard SQL semantics. This document defines the exact behavior of every supported statement and expression.

## Keyword Reference

| SaucQL | SQL equivalent |
|---|---|
| `SERVE` | `SELECT` |
| `FROM` | `FROM` |
| `STRAINED BY` | `WHERE` |
| `LADLE INTO` | `INSERT INTO` |
| `PORTIONS` | `VALUES` |
| `SEASON` | `UPDATE` |
| `DISCARD FROM` | `DELETE FROM` |
| `PLATE noodle` | `CREATE TABLE` |
| `TOSS OUT noodle` | `DROP TABLE` |
| `TOSS` | `INNER JOIN` |
| `BEGIN SIMMER` | `BEGIN` |
| `FINISH SIMMER` | `COMMIT` |
| `SPILL SIMMER` | `ROLLBACK` |
| `AL DENTE KEY` | `PRIMARY KEY` (auto-increment) |
| `NOT VUOTO` | `NOT NULL` |
| `IS VUOTO` | `IS NULL` |
| `IS NOT VUOTO` | `IS NOT NULL` |
| `ARRANGED BY` | `ORDER BY` |
| `ONLY n` | `LIMIT n` |
| `SKIP n` | `OFFSET n` |
| `BUNDLED BY` | `GROUP BY` |
| `STRAIN name ON noodle (col)` | `CREATE INDEX` |
| `REMOVE STRAINER name` | `DROP INDEX` |

All other tokens — `FROM`, `ON`, `AS`, `SET`, `AND`, `OR`, `NOT`, `IN`, `BETWEEN`, `LIKE`, `ASC`, `DESC`, `BY` — are unchanged.

## Supported Statements

`PLATE noodle`, `TOSS OUT noodle`, `STRAIN`, `REMOVE STRAINER`, `SERVE`, `LADLE INTO`, `SEASON`, `DISCARD FROM`, `BEGIN SIMMER`, `FINISH SIMMER`, `SPILL SIMMER`.

All other statement forms produce a `PARSE_002` Scottatura.

## PLATE noodle (CREATE TABLE)

```sql
PLATE noodle noodle_name (
  col1 TYPE [NOT VUOTO],
  col2 TYPE [NOT VUOTO],
  ...
);
```

- The Al Dente Key (`id`) is implicitly added as the first column. Explicitly declaring a column named `id` is a `RICETTA_006`.
- Supported types: `INTEGER`, `INT`, `BIGINT` (all map to `i64`), `REAL`, `FLOAT` (maps to `f64`), `TEXT`, `BLOB`. Any other type name is `TIPO_003`.
- `NOT VUOTO` is the only supported column constraint. `DEFAULT`, `UNIQUE`, `CHECK`, `REFERENCES` are not supported in v1.
- Noodle names and column names are case-insensitive and normalized to lowercase.

## TOSS OUT noodle (DROP TABLE)

```sql
TOSS OUT noodle noodle_name;
```

- Drops the noodle and all associated Strainer indexes.
- Not allowed inside an active Simmer that has modified the noodle (`RICETTA_004`).

## STRAIN / REMOVE STRAINER (CREATE / DROP INDEX)

```sql
STRAIN idx_name ON noodle_name (col_name);
REMOVE STRAINER idx_name;
```

- Single-column indexes only. Multi-column indexes are `PARSE_004` in v1.
- Index names must be unique within the pot.
- `STRAIN` on an existing noodle walks the heap and populates the B-tree within a single Simmer.

## SERVE (SELECT)

```sql
SERVE [* | col [AS alias], ...]
FROM noodle_name [[AS] alias]
[TOSS noodle_name [[AS] alias] ON col = col]
[STRAINED BY expr]
[ARRANGED BY col [ASC | DESC] [, col [ASC | DESC]]]
[ONLY n [SKIP m]];
```

- At most one `TOSS` (join) per query.
- `SERVE *` expands to all columns in schema-definition order (Al Dente Key first).
- Expressions in the select list: column references, literals, arithmetic (`+`, `-`, `*`, `/`), column aliases.
- `ARRANGED BY` sorts after filtering and joining. Multiple sort columns are supported.
- `ONLY` and `SKIP` are applied last. `SKIP` without `ONLY` is not supported.

## LADLE INTO (INSERT)

```sql
LADLE INTO noodle_name (col1, col2, ...) PORTIONS (val1, val2, ...);
LADLE INTO noodle_name (col1, col2, ...) PORTIONS (val1, val2, ...), (val3, val4, ...);
```

- Al Dente Key is always auto-assigned; including `id` in the column list is `RICETTA_006`.
- Value count must match column count (`RICETTA_009`).
- Type of each value must match the column type (`TIPO_001`).
- `NOT VUOTO` columns with a `VUOTO` literal produce `VINCOLO_002`.
- The auto-assigned Al Dente Key is returned in `ResultSet::last_insert_key`. In the REPL: `1 strand added (id = {key}).`

## SEASON (UPDATE)

```sql
SEASON noodle_name SET col = val [, col = val ...] STRAINED BY expr;
```

- `STRAINED BY` clause is required. A bare `SEASON` with no filter is `PARSE_007` (safety rail).
- The Al Dente Key column cannot be updated (`RICETTA_005`).
- Type of each assigned value must match the column type (`TIPO_001`).
- Updates tombstone the original slot and write a new slot. No in-place mutation.

## DISCARD FROM (DELETE)

```sql
DISCARD FROM noodle_name STRAINED BY expr;
```

- `STRAINED BY` clause is required. A bare `DISCARD FROM noodle` is `PARSE_008` (safety rail).
- Matched rows are tombstoned in the heap and their Strainer entries are marked for removal.

## STRAINED BY Expressions (WHERE)

Supported operators:

| Operator | Types | Notes |
|---|---|---|
| `=`, `!=` | All | VUOTO-safe: `VUOTO = VUOTO` → VUOTO |
| `<`, `>`, `<=`, `>=` | `INTEGER`, `REAL`, `TEXT` | Text: lexicographic UTF-8 order |
| `AND`, `OR`, `NOT` | Boolean | Three-valued logic |
| `IS VUOTO`, `IS NOT VUOTO` | All | Only way to test for VUOTO |
| `BETWEEN lo AND hi` | `INTEGER`, `REAL`, `TEXT` | Inclusive on both ends |
| `LIKE pattern` | `TEXT` | `%` = any sequence, `_` = any character |
| `IN (v1, v2, ...)` | All | Literal value list only; no sub-serves |

## TOSS Semantics (JOIN)

Only `TOSS noodle ON a.col = b.col` (inner equi-join) is supported. The Sous Chef selects a join strategy:

1. **Index-nested-loop**: if either join key has a Strainer, use it. Outer noodle drives; inner probed via index.
2. **Hash join**: build an in-memory hash map on the smaller noodle's join key, probe with the larger. Used when no Strainer is available.

`LEFT TOSS`, `RIGHT TOSS`, `FULL TOSS` are out of scope for v1 (`PARSE_005`).

## VUOTO (NULL) Semantics

- Any arithmetic involving `VUOTO` produces `VUOTO`.
- Any comparison involving `VUOTO` produces `VUOTO` (not true or false).
- `STRAINED BY` includes a strand only if its predicate is `true`. `VUOTO` predicate → strand excluded.
- `ARRANGED BY` treats `VUOTO` as less than all non-null values (first in `ASC`, last in `DESC`).
- `COUNT(col)` skips `VUOTO` values; `COUNT(*)` counts all strands.

## Aggregate Functions

`COUNT(*)`, `COUNT(col)`, `SUM(col)`, `MIN(col)`, `MAX(col)`, `AVG(col)`.

- Aggregates without `BUNDLED BY` collapse the entire result into one strand.
- `BUNDLED BY` is supported with one or more columns.
- `HAVING` is not supported in v1.
- Mixing aggregate and non-aggregate columns without `BUNDLED BY` is `PARSE_001`.

## Type Coercion

MarinaraDB is strictly typed. One implicit conversion: `INTEGER` promotes to `REAL` in mixed arithmetic (e.g., `1 + 2.5 → 3.5`). All other type mismatches produce a `TIPO_001` Scottatura at parse/plan time.

## Identifiers

- Case-insensitive. Normalized to lowercase at parse time.
- SaucQL reserved words used as identifiers must be quoted with double-quotes: `"serve"`.
- Maximum identifier length: 128 characters.

## Comments

`--` starts a line comment. Block comments are not supported in v1.

## Data Types in PLATE

| SaucQL type name | Stored as | Notes |
|---|---|---|
| `INTEGER`, `INT`, `BIGINT` | `i64` | All map to the same 8-byte signed integer |
| `REAL`, `FLOAT` | `f64` | IEEE 754 double |
| `TEXT` | `String` | UTF-8 |
| `BLOB` | `Vec<u8>` | Raw bytes |
