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

## Query Language — SaucQL

A custom SQL-like language with pasta-themed keywords. Real SQL semantics, silly syntax. SaucQL is where the joke lives hardest — the query language is deliberately ridiculous, while the semantics beneath it are standard and correct.

```sql
-- Read data
SERVE name, age FROM users STRAINED BY age > 30;

-- Write data
LADLE INTO orders (user_id, total) PORTIONS (1, 49.99);

-- Update data
SEASON users SET total = 59.99 STRAINED BY id = 1;

-- Delete data
DISCARD FROM users STRAINED BY age < 18;

-- Define a table
PLATE noodle users (
  name   TEXT   NOT VUOTO,
  age    INT
);

-- Join two noodles
SERVE u.name, o.total
FROM users u
TOSS orders o ON u.id = o.user_id;

-- Transaction
BEGIN SIMMER;
  LADLE INTO orders (user_id, total) PORTIONS (1, 49.99);
  DISCARD FROM users STRAINED BY age < 18;
FINISH SIMMER;   -- commit

SPILL SIMMER;    -- rollback
```

### Keyword Mapping

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
| `ON` | `ON` |
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
| `STRAIN name ON noodle (col)` | `CREATE INDEX name ON table(col)` |
| `REMOVE STRAINER name` | `DROP INDEX name` |

---

## Where the Pasta Shows Up

### CLI / REPL

```
$ marinara
🍅 MarinaraDB v0.1.0 — Buon appetito!
Connected to pot: my_app.pot

marinara> SERVE * FROM users;
Serving 3 strands from noodle 'users'...

 id | name  | age
----+-------+-----
  1 | Tony  |  38
  2 | Maria |  29
  3 | Gino  |  44

marinara> BEGIN SIMMER;
Simmer started.

marinara> TOSS OUT noodle users;
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

    // SaucQL — pasta-themed keywords, standard semantics
    let strands = pot.execute("SERVE * FROM users STRAINED BY age > 30")?;

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
| SaucQL keyword mapping, statement semantics & VUOTO rules | [docs/sql-semantics.md](docs/sql-semantics.md) |
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
| Query language | SaucQL — custom syntax, standard SQL semantics |
| Parser | Hand-rolled recursive-descent (no external parser library) |
| CLI / REPL | `clap` + `rustyline` |
| Binary I/O | `std::fs`, `memmap2` |
| Serialization | `bincode` |
| Error handling | `thiserror` + `anyhow` |
| Testing | built-in `cargo test` |
| Packaging | `cargo` + `Cargo.toml` |

The custom SaucQL parser is hand-rolled in `src/parser/` — a lexer that tokenises SaucQL keywords plus a recursive-descent parser that produces an AST. Writing the parser by hand keeps the dependency count low and makes it straightforward to emit Scottatura errors with SaucQL-native token names. Rust's ownership model is a natural fit for page/buffer pool management, and the performance headroom means the Warming Tray and Stove can be built simply.

---

## Repo Structure

```
marinaradb/
├── src/
│   ├── main.rs             # CLI entry point / REPL
│   ├── parser/             # SaucQL lexer + recursive-descent parser
│   ├── sous_chef/          # query planner
│   ├── stove/              # executor + storage engine
│   ├── warming_tray/       # page cache / buffer pool
│   ├── strainer/           # B-tree index
│   ├── prep_log/           # write-ahead log
│   └── lib.rs              # programmatic client API
├── tests/
├── examples/               # sample .sauql scripts
└── Cargo.toml
```

---

## Post-v1 Ideas

- **Scottatura messages** — errors delivered in an angry Italian chef voice
- **`EXPLAIN` output** — renders the Sous Chef's query plan as a recipe card
- **`TASTE noodle`** — alias for `DESCRIBE table`, prints schema as a formatted recipe
- **Simmer mode** — slow query log that prints "letting it simmer..." with elapsed time
- **`.pot` export** — dump a database as a portable `.sauql` script
- **`marinara serve`** — joke HTTP API with endpoints like `POST /ladle` and `GET /serve`
- **Scottatura codes** — Italian-named error codes (e.g. `BRUCIATO_001: page checksum mismatch`)
