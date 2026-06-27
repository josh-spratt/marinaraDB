# 🍅 MarinaraDB

> *A relational database that's been simmering on the stove all day.*

---

## Overview

MarinaraDB is a fully functional toy RDBMS written in Python. It speaks standard SQL but names every internal component after pasta and Italian food. The joke lives in the internals, CLI output, logs, error messages, and programmatic API — not in the query language.

---

## Terminology

| Real RDBMS | MarinaraDB |
|---|---|
| Database | **Pot** |
| Table | **Noodle** |
| Row | **Strand** |
| Column | **Ingredient** |
| Index | **Strainer** |
| Primary Key | **Al Dente Key** |
| Foreign Key | **Marinara Bond** |
| Transaction | **Simmer** |
| Schema | **Recipe** |
| Query Planner | **Sous Chef** |
| Storage Engine | **The Stove** |
| Write-Ahead Log | **Prep Log** |
| Buffer Pool / Page Cache | **The Warming Tray** |
| Page | **Plate** |
| Null | **Vuoto** |
| Error | **Scottatura** |

---

## Query Language

Standard SQL — no changes. `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `JOIN`, `BEGIN`, `COMMIT`, `ROLLBACK` all work exactly as expected.

```sql
SELECT name, age FROM users WHERE age > 30;

BEGIN;
INSERT INTO orders (user_id, total) VALUES (1, 49.99);
COMMIT;
```

---

## Where the Pasta Shows Up

### CLI / REPL

```
$ marinara
🍅 MarinaraDB v0.1.0 — Buon appetito!
Connected to pot: my_app.pot

marinara> SELECT * FROM users;
Serving 3 strands from noodle 'users'...

 id | name  | age
----+-------+-----
  1 | Tony  |  38
  2 | Maria |  29
  3 | Gino  |  44

marinara> BEGIN;
Simmer started.

marinara> DROP TABLE users;
Scottatura: Cannot toss a noodle mid-simmer without a Marinara Bond check.
```

### Internal Logs

```
[STOVE]        Allocated new plate 42 in pot my_app.pot
[WARMING TRAY] Cache miss — fetching plate 42 from disk
[SOUS CHEF]    Query plan: SeqScan on noodle 'users' → Filter → Sort
[PREP LOG]     Simmer #00041 committed, 3 strands modified
```

### Programmatic Client (Rust)

```rust
use marinaradb::Pot;

fn main() -> anyhow::Result<()> {
    let pot = Pot::open("my_app.pot")?;

    // Standard SQL — no surprises
    let strands = pot.execute("SELECT * FROM users WHERE age > 30")?;

    // Themed client API
    println!("{:?}", pot.noodles());          // list tables
    println!("{:?}", pot.recipe("users")?);   // describe table schema
    println!("{:?}", pot.strainer_stats());   // index usage stats

    // Simmer = transaction
    let mut simmer = pot.simmer()?;
    simmer.execute("INSERT INTO users (name, age) VALUES ('Rosa', 51)")?;
    simmer.finish()?; // commit

    Ok(())
}
```

---

## Architecture

```
┌──────────────────────────────────────┐
│           marinara CLI / REPL        │
└─────────────────┬────────────────────┘
                  │ SQL string
┌─────────────────▼────────────────────┐
│            SQL Parser                │  ← standard SQL, no changes
└─────────────────┬────────────────────┘
                  │ AST
┌─────────────────▼────────────────────┐
│      Sous Chef  (Query Planner)      │  ← rule-based plan generation
└─────────────────┬────────────────────┘
                  │ execution plan
┌─────────────────▼────────────────────┐
│         The Stove  (Executor)        │  ← walks + executes the plan
└─────────────────┬────────────────────┘
          ┌───────┴────────┐
┌─────────▼──────┐  ┌──────▼──────────────┐
│  Warming Tray  │  │  Strainer  (B-tree)  │
│  (Page Cache)  │  │  Index               │
└─────────┬──────┘  └─────────────────────┘
          │
┌─────────▼──────────────────────────┐
│    The Stove — Storage Layer       │
│    .pot files, Plate (page) I/O    │
│    Prep Log (WAL)                  │
└────────────────────────────────────┘

### Detailed Design Docs

Each component has a companion specification document:

| Component | Document |
|---|---|
| Type system (`INTEGER`, `TEXT`, `REAL`, `BLOB`) | [docs/type-system.md](docs/type-system.md) |
| `.pot` file & page (Plate) layout | [docs/storage-format.md](docs/storage-format.md) |
| Prep Log (WAL) format & recovery | [docs/wal-format.md](docs/wal-format.md) |
| Strainer B-tree format & operations | [docs/btree-format.md](docs/btree-format.md) |
| SQL statement semantics & NULL rules | [docs/sql-semantics.md](docs/sql-semantics.md) |
| Simmer (transaction) lifecycle & locking | [docs/transaction-model.md](docs/transaction-model.md) |
| Scottatura error codes | [docs/error-taxonomy.md](docs/error-taxonomy.md) |
| Unit & integration testing strategy | [docs/testing-strategy.md](docs/testing-strategy.md) |

Full design document (module APIs, architectural decisions): [docs/superpowers/specs/2026-06-27-marinaradb-design.md](docs/superpowers/specs/2026-06-27-marinaradb-design.md)
```

### Storage

- Data stored in `.pot` files (binary, page-based, modeled loosely on SQLite)
- One `.pot` file per database (Pot)
- Noodles (tables) stored as heap files within the pot
- Strainer indexes stored as B-tree structures alongside the heap
- Prep Log (WAL) written before any Plate is mutated, enabling crash recovery

---

## v1 Feature Scope

**In scope:**

- `CREATE TABLE` / `DROP TABLE` (DDL)
- `SELECT`, `INSERT`, `UPDATE`, `DELETE` (DML)
- `WHERE` filtering with basic operators (`=`, `<`, `>`, `!=`, `AND`, `OR`)
- `INNER JOIN` (single join per query)
- `ORDER BY`, `LIMIT`
- Auto-increment primary keys (Al Dente Keys)
- Single-column Strainer indexes
- `BEGIN` / `COMMIT` / `ROLLBACK` transactions with WAL
- REPL + `.sql` script file execution

**Out of scope for v1:**

- Subqueries, CTEs, window functions
- Multi-column indexes
- `LEFT` / `RIGHT` / `FULL` joins
- Views, triggers, stored procedures
- Networked / multi-client access

---

## Stack

| Concern | Choice |
|---|---|
| Language | Rust (2021 edition) |
| SQL parsing | `sqlparser-rs` |
| CLI / REPL | `clap` + `rustyline` |
| Binary I/O | `std::fs`, `memmap2` |
| Serialization | `bincode` |
| Error handling | `thiserror` + `anyhow` |
| Testing | built-in `cargo test` |
| Packaging | `cargo` + `Cargo.toml` |

`sqlparser-rs` is the de facto standard SQL parser in the Rust ecosystem — it produces a typed AST for standard SQL and is used in production by projects like DataFusion and DeltaLake. All the pasta naming happens downstream of parsing. Rust's ownership model is a natural fit for page/buffer pool management, and the performance headroom means the Warming Tray and Stove can be built simply without worrying about Python's I/O overhead.

---

## Repo Structure

```
marinaradb/
├── src/
│   ├── main.rs             # CLI entry point / REPL
│   ├── parser/             # sqlparser-rs wrapper + AST normalization
│   ├── sous_chef/          # query planner
│   ├── stove/              # executor + storage engine
│   ├── warming_tray/       # page cache / buffer pool
│   ├── strainer/           # B-tree index
│   ├── prep_log/           # write-ahead log
│   └── lib.rs              # programmatic client API
├── tests/
├── examples/               # sample .sql scripts
└── Cargo.toml
```

---

## Post-v1 Ideas

- **Scottatura messages** — errors delivered in an angry Italian chef voice
- **`EXPLAIN` output** — renders the Sous Chef's query plan as a recipe card
- **`TASTE noodle`** — alias for `DESCRIBE table`, prints schema as a formatted recipe
- **Simmer mode** — slow query log that prints "letting it simmer..." with elapsed time
- **`.pot` export** — dump a database as a portable `.sql` script
- **`marinara serve`** — joke HTTP API with endpoints like `POST /ladle` and `GET /serve`
- **Scottatura codes** — Italian-named error codes (e.g. `BRUCIATO_001: page checksum mismatch`)
