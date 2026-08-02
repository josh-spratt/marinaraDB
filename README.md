# MarinaraDB

> *A relational database that's been simmering on the stove all day.*

A fully functional toy RDBMS written in Rust, with pasta-themed terminology and a custom SQL-like query language called SaucQL. See [SPEC.md](SPEC.md) for the full design.

## Current Status

**Phase 2c — Joins, Ordering, Limiting, Grouping & Aggregates** (unreleased, building on `v0.1.0`)

The storage layer, SaucQL parser, and executor are implemented and tested, including multi-table queries. Transactions and Strainer indexes are parsed but not yet executable.

What works:
- Open/create a `.pot` database file; data persists across process restarts
- Create and drop noodles (tables) with typed ingredients (columns)
- `Pot::execute(sauql)` — run a SaucQL string end-to-end:
  - `PLATE noodle` / `TOSS OUT noodle` (CREATE/DROP TABLE)
  - `LADLE INTO ... PORTIONS ...` (INSERT), including partial column lists and multi-row inserts
  - `SERVE ... FROM ... STRAINED BY ...` (SELECT with WHERE) — full expression support: comparisons, `AND`/`OR`/`NOT`, `IS VUOTO`, `BETWEEN`, `LIKE`, `IN`, three-valued VUOTO logic, `INTEGER`→`REAL` coercion
  - `TOSS` (inner equi-join) — a single join per query, with qualifier-aware column resolution (`table.column`) and ambiguity detection for unqualified references that match more than one joined table
  - `ARRANGED BY` (ORDER BY) — multi-key sort; VUOTO always sorts first, regardless of `ASC`/`DESC`
  - `ONLY`/`SKIP` (LIMIT/OFFSET), applied after ordering
  - `BUNDLED BY` (GROUP BY) with `COUNT`/`SUM`/`AVG`/`MIN`/`MAX` aggregate functions
  - `SEASON ... SET ... STRAINED BY ...` (UPDATE)
  - `DISCARD FROM ... STRAINED BY ...` (DELETE)
- Multi-page heap storage — tables are no longer capped at one page of rows
- Auto-increment Al Dente Keys (primary keys)
- `NOT VUOTO` (NOT NULL) constraint enforcement

Two intentional scope limits for now (see `docs/sql-semantics.md`): `ARRANGED BY` can only sort by a column that's in the query's own `SERVE` list, and `BUNDLED BY` only accepts a single bare unqualified column name (so grouping a joined query by a column both tables share, like the implicit `id`, isn't yet expressible).

What's stubbed (not yet executable — parses but returns a clear error):
- Transactions (`BEGIN`/`FINISH`/`SPILL SIMMER`) — awaiting the Prep Log (WAL)
- `STRAIN`/`REMOVE STRAINER` (B-tree indexes) — Phase 3
- WAL-based crash recovery

## Prerequisites

- Rust toolchain (`rustc` + `cargo`), 2021 edition or later
- Install via [rustup](https://rustup.rs) if you don't have it: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

## Build

```sh
cargo build
```

## Run Tests

```sh
cargo test
```

All 173 tests should pass (unit tests across `src/`, plus integration suites in `tests/foundation.rs`, `tests/parser.rs`, and `tests/executor.rs` covering create/drop noodle, insert, scan, persistence, constraint violations, joins, ordering, limiting, grouping, and aggregates).

## Run the Binary

Launch an interactive REPL session:

```sh
cargo run
# MarinaraDB v0.1.0 — Buon appetito!
# Connected to marinara.pot. Type .help for commands, .quit to exit.
marinara> PLATE noodle users (name TEXT, age INT);
marinara> LADLE INTO users (name, age) PORTIONS ('Tony', 38);
marinara> SERVE * FROM users;
id | name | age
---|------|----
1  | Tony | 38
marinara> .tables
users
marinara> .quit
```

Or open a specific database file:

```sh
cargo run -- path/to/db.pot
```

Commands:
- End each SaucQL statement with `;` (the parser's statement terminator)
- `.tables` — list all tables
- `.help` — show command help
- `.quit` / `.exit` — exit the shell

Error recovery: if a statement fails to parse or execute, MarinaraDB prints the error and returns to the prompt; the session continues normally.

## Use the Library

Add `marinaradb` as a path dependency in your `Cargo.toml`, then:

```rust
use marinaradb::{ColumnDef, ColType, Pot, Value};

let mut pot = Pot::open("my_app.pot")?;
pot.create_table("users", vec![
    ColumnDef { name: "name".into(), col_type: ColType::Text,    nullable: false, strainer_root: None },
    ColumnDef { name: "age".into(),  col_type: ColType::Integer, nullable: true,  strainer_root: None },
])?;

let key = pot.insert_strand("users", vec![Value::Text("Tony".into()), Value::Integer(38)])?;

let rows = pot.scan_noodle("users")?; // Vec<(i64, Vec<Value>)>
```

Or drive it with SaucQL directly via `Pot::execute`, which parses, plans, and runs a statement in one call:

```rust
use marinaradb::Pot;

let mut pot = Pot::open("my_app.pot")?;
pot.execute("PLATE noodle users (name TEXT NOT VUOTO, age INT);")?;
pot.execute("LADLE INTO users (name, age) PORTIONS ('Tony', 38), ('Maria', 29);")?;
pot.execute("SEASON users SET age = 39 STRAINED BY name = 'Tony';")?;

let result = pot.execute("SERVE name, age FROM users STRAINED BY age > 30;")?;
for row in &result.rows {
    println!("{row:?}");
}
```

See [Current Status](#current-status) for what's parsed but not yet runnable (transactions, Strainer indexes).

## Versioning and Releases

This project uses [Semantic Versioning](https://semver.org). The version in `Cargo.toml` is the source of truth.

To cut a release:
1. Update `version` in `Cargo.toml`
2. Add a dated section to `CHANGELOG.md`
3. Commit: `git commit -m "chore: release vX.Y.Z"`
4. Tag: `git tag vX.Y.Z`
5. Push: `git push && git push --tags`

See [CHANGELOG.md](CHANGELOG.md) for the full history.
