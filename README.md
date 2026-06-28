# MarinaraDB

> *A relational database that's been simmering on the stove all day.*

A fully functional toy RDBMS written in Rust, with pasta-themed terminology and a custom SQL-like query language called SaucQL. See [SPEC.md](SPEC.md) for the full design.

## Current Status

**Phase 1 — Storage Foundation** (`v0.1.0`)

The storage layer is implemented and tested. The REPL, query parser, and executor are coming in later phases. Right now you can use the Rust library API directly.

What works:
- Open/create a `.pot` database file
- Create and drop noodles (tables) with typed ingredients (columns)
- Insert strands (rows), including `NOT VUOTO` (NOT NULL) enforcement
- Sequential scan of all strands in a noodle
- Data persists across process restarts
- Auto-increment Al Dente Keys (primary keys)

What's stubbed (not yet implemented):
- SaucQL parser and REPL
- WAL-based crash recovery
- B-tree Strainer indexes

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

All 8 integration tests should pass. They cover create/drop noodle, insert, scan, persistence, and constraint violations.

## Run the Binary

The CLI is a stub at this stage — it prints a banner and exits:

```sh
cargo run
# MarinaraDB v0.1.0 — Buon appetito!
```

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

## Versioning and Releases

This project uses [Semantic Versioning](https://semver.org). The version in `Cargo.toml` is the source of truth.

To cut a release:
1. Update `version` in `Cargo.toml`
2. Add a dated section to `CHANGELOG.md`
3. Commit: `git commit -m "chore: release vX.Y.Z"`
4. Tag: `git tag vX.Y.Z`
5. Push: `git push && git push --tags`

See [CHANGELOG.md](CHANGELOG.md) for the full history.
