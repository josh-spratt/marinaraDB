# Type System

MarinaraDB supports four scalar types. All other SQL types are rejected at parse time with a `TIPO_` Scottatura.

## Types

| SQL name | Rust repr | On-disk size | Notes |
|---|---|---|---|
| `INTEGER` | `i64` | 8 bytes | Little-endian signed 64-bit |
| `REAL` | `f64` | 8 bytes | IEEE 754 little-endian double |
| `TEXT` | `String` | `u32` length prefix + UTF-8 bytes | Max inline: 512 bytes; longer values use an overflow page |
| `BLOB` | `Vec<u8>` | `u32` length prefix + raw bytes | Max inline: 512 bytes; longer values use an overflow page |

## NULL (Vuoto)

Every column value may be `NULL` (Vuoto). Nullability is tracked via a null bitmap in each row header — one bit per column, packed into `ceil(col_count / 8)` bytes. The bitmap is the authoritative null indicator.

Variable-length types (`TEXT`, `BLOB`) use a sentinel length of `0xFFFFFFFF` as a belt-and-suspenders backup, but the bitmap takes precedence.

NULL semantics follow standard three-valued logic:
- Any comparison with `NULL` evaluates to `NULL`, not `true` or `false`.
- `WHERE` clauses include only rows where the predicate evaluates to `true`.
- `NULL IS NULL` → `true`. `NULL IS NOT NULL` → `false`.

## Al Dente Key (Primary Key)

Every noodle has exactly one Al Dente Key: an `INTEGER` column, auto-increment, never `NULL`. It is stored as an `i64` in the first 8 bytes of every row, before the null bitmap. Al Dente Keys start at `1` and increment by `1`. Gaps may appear after deletes; gaps are never backfilled.

## Type Coercion

MarinaraDB is strictly typed. There is exactly one implicit conversion: `INTEGER` promotes to `REAL` in mixed arithmetic expressions (e.g., `1 + 2.5 → 3.5`). All other type mismatches produce a `TIPO_001` Scottatura at parse/plan time, before any execution begins.

## Encoding

### Fixed-width types
`INTEGER` and `REAL` are stored as exactly 8 bytes, little-endian. The null bitmap determines whether the bytes represent a real value or should be ignored.

### Variable-length types
`TEXT` and `BLOB` are stored as a `u32` length (4 bytes, little-endian) followed by that many bytes of content. If the total encoded length exceeds 512 bytes, the column value is replaced on the heap page with an 8-byte overflow page pointer (see `storage-format.md`).

## Literals

| Type | Valid literal forms |
|---|---|
| `INTEGER` | `42`, `-7`, `0` |
| `REAL` | `3.14`, `-0.5`, `1.0` |
| `TEXT` | `'hello'`, `''`, `'it''s'` (single-quote escaped) |
| `BLOB` | `X'deadbeef'` (hex string) |
| `NULL` | `NULL` |
