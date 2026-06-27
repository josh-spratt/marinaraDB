# Error Taxonomy (Scottatura Codes)

All errors in MarinaraDB are `ScottaturaCodes`, exposed as a `thiserror` enum in `lib.rs`. Every code has a machine-readable identifier, a structured message, and a REPL display format.

## REPL Display Format

```
Scottatura [CODE]: message text.
```

Example:
```
Scottatura [RICETTA_004]: Cannot toss a noodle mid-simmer without a Marinara Bond check.
```

## Error Categories

| Category | Prefix | When |
|---|---|---|
| Parse | `PARSE_` | SQL the parser could not understand |
| Type | `TIPO_` | Type mismatches, bad literals, unsupported types |
| Schema | `RICETTA_` | Missing/duplicate noodles or ingredients, DDL violations |
| Constraint | `VINCOLO_` | PK violations, NOT NULL violations |
| Transaction | `SIMMER_` | Misuse of BEGIN / COMMIT / ROLLBACK |
| Storage | `STOVE_` | I/O errors, checksum failures, corrupt pages |
| Lock | `LOCK_` | Timeout waiting for exclusive lock |

## Parse Errors (PARSE_)

| Code | Message |
|---|---|
| `PARSE_001` | Unexpected token `{token}` at position {pos}. |
| `PARSE_002` | Statement type not supported. |
| `PARSE_003` | Subqueries are not supported in v1. |
| `PARSE_004` | Multi-column indexes are not supported in v1. |
| `PARSE_005` | LEFT / RIGHT / FULL JOIN is not supported in v1; use INNER JOIN. |
| `PARSE_006` | Only equi-joins (ON a.col = b.col) are supported. |
| `PARSE_007` | UPDATE requires a WHERE clause. |
| `PARSE_008` | DELETE requires a WHERE clause. |
| `PARSE_009` | OFFSET without LIMIT is not supported. |
| `PARSE_010` | HAVING is not supported in v1. |
| `PARSE_011` | At most one JOIN per query is supported in v1. |
| `PARSE_012` | Identifier `{name}` exceeds maximum length of 128 characters. |

## Type Errors (TIPO_)

| Code | Message |
|---|---|
| `TIPO_001` | Cannot store {got} value in ingredient `{col}` (expected {expected}). |
| `TIPO_002` | Operator `{op}` is not defined for {left} and {right}. |
| `TIPO_003` | Unknown type `{type_name}`. Supported types: INTEGER, REAL, TEXT, BLOB. |
| `TIPO_004` | Invalid literal `{literal}` for type {type}. |

## Schema Errors (RICETTA_)

| Code | Message |
|---|---|
| `RICETTA_001` | Noodle `{name}` does not exist. |
| `RICETTA_002` | Noodle `{name}` already exists. |
| `RICETTA_003` | Ingredient `{col}` not found in noodle `{name}`. |
| `RICETTA_004` | Cannot toss a noodle mid-simmer without a Marinara Bond check. |
| `RICETTA_005` | Cannot update the Al Dente Key (`id`) column. |
| `RICETTA_006` | Column `id` is reserved for the Al Dente Key; remove it from the column list. |
| `RICETTA_007` | Index `{name}` does not exist. |
| `RICETTA_008` | Index `{name}` already exists. |
| `RICETTA_009` | Value count ({got}) does not match column count ({expected}). |

## Constraint Errors (VINCOLO_)

| Code | Message |
|---|---|
| `VINCOLO_001` | Al Dente Key conflict — a strand with key {key} already exists in `{noodle}`. |
| `VINCOLO_002` | Vuoto not allowed in ingredient `{col}` (declared NOT NULL). |

## Transaction Errors (SIMMER_)

| Code | Message |
|---|---|
| `SIMMER_001` | No active simmer — COMMIT requires a BEGIN first. |
| `SIMMER_002` | No active simmer — ROLLBACK requires a BEGIN first. |
| `SIMMER_003` | Nested simmers are not supported. |

## Storage Errors (STOVE_)

| Code | Message |
|---|---|
| `STOVE_001` | Page checksum mismatch on plate {page} — pot may be corrupt. |
| `STOVE_002` | Magic number invalid — `{path}` is not a pot file. |
| `STOVE_003` | Prep Log CRC32 mismatch at LSN {lsn} — truncating log at this point. |
| `STOVE_004` | I/O error on pot file: {os_error}. |
| `STOVE_005` | I/O error on prep log: {os_error}. |
| `STOVE_006` | Pot file version {version} is not supported by this build. |

## Lock Errors (LOCK_)

| Code | Message |
|---|---|
| `LOCK_001` | Simmer timed out after {ms}ms waiting for exclusive lock; rolled back. |

## Rust Error Type

```rust
#[derive(Debug, thiserror::Error)]
pub enum Scottatura {
    #[error("Scottatura [PARSE_001]: Unexpected token `{token}` at position {pos}.")]
    UnexpectedToken { token: String, pos: usize },

    #[error("Scottatura [RICETTA_001]: Noodle `{name}` does not exist.")]
    NoodleNotFound { name: String },

    #[error("Scottatura [STOVE_001]: Page checksum mismatch on plate {page} — pot may be corrupt.")]
    PageChecksumMismatch { page: u64 },

    // ... one variant per code
}
```

All public APIs return `Result<T, Scottatura>`. The REPL catches `Scottatura` and prints the `Display` form. The programmatic client exposes the enum directly so callers can match on specific codes.
