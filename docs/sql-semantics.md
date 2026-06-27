# SQL Semantics

MarinaraDB parses standard SQL via `sqlparser-rs`. This document defines the exact behavior of each supported statement and expression form.

## Supported Statements

`CREATE TABLE`, `DROP TABLE`, `CREATE INDEX`, `DROP INDEX`, `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `BEGIN`, `COMMIT`, `ROLLBACK`.

All other statement types produce a `PARSE_002` Scottatura.

## CREATE TABLE

```sql
CREATE TABLE noodle_name (
  col1 TYPE [NOT NULL],
  col2 TYPE [NOT NULL],
  ...
);
```

- The Al Dente Key (`id INTEGER`) is implicitly added as the first column. Explicitly defining a column named `id` is a `RICETTA_002`.
- Supported types: `INTEGER`, `REAL`, `TEXT`, `BLOB`. Any other type name is `TIPO_001`.
- `NOT NULL` is the only supported column constraint. `DEFAULT`, `UNIQUE`, `CHECK`, `REFERENCES` are not supported in v1.
- Noodle names and column names are case-insensitive and normalized to lowercase internally.

## DROP TABLE

```sql
DROP TABLE noodle_name;
```

- Drops the noodle and all associated Strainer indexes.
- Not allowed inside an active Simmer that has modified the noodle (`RICETTA_004`).

## CREATE INDEX / DROP INDEX

```sql
CREATE INDEX idx_name ON noodle_name (col_name);
DROP INDEX idx_name;
```

- Single-column indexes only. Multi-column indexes are `PARSE_001` in v1.
- Index names must be unique within the pot.
- `CREATE INDEX` on an existing noodle walks the heap and populates the B-tree within a single Simmer.

## SELECT

```sql
SELECT [* | col [AS alias], ...]
FROM noodle_name [[AS] alias]
[INNER JOIN noodle_name [[AS] alias] ON col = col]
[WHERE expr]
[ORDER BY col [ASC | DESC] [, col [ASC | DESC]]]
[LIMIT n [OFFSET m]];
```

- At most one `INNER JOIN` per query.
- `SELECT *` expands to all columns in schema-definition order (Al Dente Key first).
- Expressions in the select list: column references, integer/real/text literals, arithmetic (`+`, `-`, `*`, `/`), and column aliases.
- `ORDER BY` sorts after filtering and joining. Multiple sort columns are supported.
- `LIMIT` and `OFFSET` are applied last. `OFFSET` without `LIMIT` is not supported.

## INSERT

```sql
INSERT INTO noodle_name (col1, col2, ...) VALUES (val1, val2, ...);
INSERT INTO noodle_name (col1, col2, ...) VALUES (val1, val2, ...), (val3, val4, ...);
```

- Al Dente Key is always auto-assigned; including it in the column list is `RICETTA_003`.
- Value count must match column count (`PARSE_001`).
- Type of each value must match the column type (`TIPO_001`).
- `NOT NULL` columns with a `NULL` value produce `VINCOLO_002`.
- The auto-assigned Al Dente Key is returned in `ResultSet::last_insert_key`. In the REPL, it is printed as `1 strand added (id = {key}).`

## UPDATE

```sql
UPDATE noodle_name SET col = val [, col = val ...] WHERE expr;
```

- `WHERE` clause is required. A bare `UPDATE` with no filter is `PARSE_001` (safety rail).
- The Al Dente Key column cannot be updated (`RICETTA_003`).
- Type of each assigned value must match the column type (`TIPO_001`).
- Updates are applied to a copy of the row; the original slot is tombstoned and a new slot is written (no in-place update). This keeps WAL logic simple.

## DELETE

```sql
DELETE FROM noodle_name WHERE expr;
```

- `WHERE` clause is required. A bare `DELETE FROM noodle` is `PARSE_001` (safety rail).
- Matched rows are tombstoned in the heap and their Strainer entries are marked for removal.

## WHERE Expressions

Supported operators:

| Operator | Types | Notes |
|---|---|---|
| `=`, `!=` | All | NULL-safe: `NULL = NULL` → NULL |
| `<`, `>`, `<=`, `>=` | `INTEGER`, `REAL`, `TEXT` | Text: lexicographic UTF-8 order |
| `AND`, `OR`, `NOT` | Boolean | Three-valued logic |
| `IS NULL`, `IS NOT NULL` | All | Only way to test for NULL |
| `BETWEEN lo AND hi` | `INTEGER`, `REAL`, `TEXT` | Inclusive on both ends |
| `LIKE pattern` | `TEXT` | `%` = any sequence, `_` = any character |
| `IN (v1, v2, ...)` | All | Literal value list only; no subqueries |

## JOIN Semantics

Only `INNER JOIN ... ON a.col = b.col` (equi-join) is supported. The Sous Chef selects a join strategy:

1. **Index-nested-loop join**: if either join key has a Strainer, use it. Outer table drives; inner table probed via index.
2. **Hash join**: build an in-memory hash map on the smaller table's join key, probe with the larger. Used when no Strainer is available.

`LEFT JOIN`, `RIGHT JOIN`, and `FULL JOIN` are out of scope for v1 (`PARSE_001`).

## NULL Semantics (Summary)

- Any arithmetic involving `NULL` produces `NULL`.
- Any comparison involving `NULL` produces `NULL` (not `true` or `false`).
- `WHERE` includes a row only if its predicate is `true`. `NULL` predicate → row excluded.
- `ORDER BY` treats `NULL` as less than all non-null values (sorts first in `ASC`, last in `DESC`).
- `COUNT(col)` skips `NULL` values; `COUNT(*)` counts all rows including those with nulls.

## Aggregate Functions

`COUNT(*)`, `COUNT(col)`, `SUM(col)`, `MIN(col)`, `MAX(col)`, `AVG(col)`.

- Aggregates without `GROUP BY` collapse the entire result into one row.
- `GROUP BY` is supported with one or more columns.
- `HAVING` is not supported in v1.
- Mixing aggregate and non-aggregate columns without `GROUP BY` is `PARSE_001`.

## Identifiers

- Case-insensitive. Normalized to lowercase at parse time.
- Reserved words used as identifiers must be quoted with double-quotes: `"select"`.
- Maximum identifier length: 128 characters.
