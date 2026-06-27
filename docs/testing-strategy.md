# Testing Strategy

MarinaraDB uses a two-layer testing approach: unit tests per module and SQL-level integration tests. No mocking of storage I/O — tests that touch the stove or prep log use real `tempfile` files.

## Unit Tests

Unit tests live in `#[cfg(test)]` blocks within each source file. Each module is tested in isolation against its public interface.

### Coverage by Module

| Module | What to test |
|---|---|
| `parser/` | AST shape for every supported statement type; `PARSE_` errors for all invalid forms |
| `stove/` (storage) | Page header encode/decode round-trips; slot allocation and tombstoning; overflow pointer read/write; file header validation; `STOVE_` errors on corrupt magic/checksum |
| `warming_tray/` | Page fetch (hit and miss); dirty page tracking; eviction order; Simmer-owned page blocking |
| `prep_log/` | Record encode/decode; CRC32 generation and verification; truncation on bad checksum; checkpoint record write/read; recovery replay correctness |
| `strainer/` | Insert and lookup for all four types; range scan across leaf sibling chain; node split triggers new root; tombstone delete; `StrainerStats` counters |
| `sous_chef/` | Plan selection: seq scan when no index; index scan when Strainer exists on WHERE col; index-nested-loop join vs hash join selection |
| `lib.rs` (Scottatura) | All error variants format correctly via `Display` |

### Guidelines

- Use `tempfile::NamedTempFile` for any test that reads/writes `.pot` or `.prep` files.
- Unit tests must not depend on execution order.
- Each test function tests one behavior. Name tests as `test_<what>_<condition>` (e.g., `test_insert_not_null_violation`).

## SQL Integration Tests

Integration tests live in `tests/` as pairs: a `.sql` script and a `.expected` output file. The test harness (`tests/harness.rs`) opens a fresh in-memory pot, runs the script through the full stack (parser → sous chef → stove), and diffs stdout against the expected file byte-for-byte.

### Test File Conventions

```
tests/
├── harness.rs                        ← shared runner logic
├── select_basic.sql
├── select_basic.expected
├── select_where_operators.sql
├── select_where_operators.expected
├── select_join_inner.sql
├── select_join_inner.expected
├── select_aggregates.sql
├── select_aggregates.expected
├── select_order_limit.sql
├── select_order_limit.expected
├── insert_autokey.sql
├── insert_autokey.expected
├── insert_multi_row.sql
├── insert_multi_row.expected
├── update_where.sql
├── update_where.expected
├── delete_where.sql
├── delete_where.expected
├── ddl_create_drop.sql
├── ddl_create_drop.expected
├── index_create_lookup.sql
├── index_create_lookup.expected
├── transaction_commit.sql
├── transaction_commit.expected
├── transaction_rollback.sql
├── transaction_rollback.expected
├── error_type_mismatch.sql
├── error_type_mismatch.expected
├── error_missing_noodle.sql
├── error_missing_noodle.expected
├── error_not_null.sql
├── error_not_null.expected
└── crash_recovery.sql
    crash_recovery.expected
```

### Expected Output Format

The expected file matches REPL stdout exactly, including the result table and row-count line:

```
 id | name  | age
----+-------+-----
  1 | Tony  |  38
  2 | Maria |  29

Serving 2 strands from noodle 'users'.
```

Error test cases expect the `Scottatura` line:

```
Scottatura [TIPO_001]: Cannot store TEXT value in ingredient `age` (expected INTEGER).
```

### Crash Recovery Test

`crash_recovery.sql` is special: it is split into two phases, separated by a deliberate `std::process::abort()` call injected by the harness between phase one and phase two.

- **Phase one**: Opens a `.pot` file (on disk, not in-memory), starts a Simmer, inserts rows, then aborts mid-simmer.
- **Phase two**: A new process opens the same `.pot` file, verifies the uncommitted strands are absent and previously committed strands are present.

The harness manages the two-process lifecycle via `std::process::Command`.

## Running Tests

```
cargo test                    # all tests
cargo test --lib              # unit tests only
cargo test --test '*'         # integration tests only
cargo test transaction        # all tests matching "transaction"
```

## What Not to Test

- SQL behavior already specified by the SQL standard and validated by `sqlparser-rs` (e.g., tokenization of string literals). Trust the library.
- Performance characteristics. No benchmarks in v1.
- Concurrent access. Single-lock model makes concurrency testing unnecessary in v1.
