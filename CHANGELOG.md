# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `Pot::execute(sauql: &str) -> Result<ResultSet, Scottatura>` — parses, plans, and executes a SaucQL statement end-to-end
- Sous Chef planner (`sous_chef::plan`) — translates parsed statements into an executable `Plan`, rejecting joins/`ARRANGED BY`/`ONLY`/`SKIP`/`BUNDLED BY`/transactions/Strainer DDL with clear `EXEC_*` errors
- Stove executor — single-table `SERVE`/`LADLE INTO`/`SEASON`/`DISCARD FROM` with full `STRAINED BY` expression support (comparisons, `AND`/`OR`/`NOT`, `IS VUOTO`, `BETWEEN`, `LIKE`, `IN`, three-valued VUOTO logic, `INTEGER`→`REAL` coercion)
- Multi-page heap traversal — heap storage now chains across pages via `next_page` instead of capping a table at one page
- `RICETTA_005`, `EXEC_001`–`EXEC_003` Scottatura error codes
- `examples/trattoria_seed.sauql` — sample seed data (18 customers, 18 dishes, 40 orders) for a fictional Italian restaurant, runnable via `cargo run -- demo.pot < examples/trattoria_seed.sauql`

## [0.1.0] - 2026-06-28

### Added

- `Pot` — core database file type backed by a paged `.pot` binary format
- Page (Plate) layer with CRC32 checksums and fixed 4 KB page size
- Heap file storage for noodle (table) rows with sequential scan
- Catalog — persists noodle schemas (Recipes) inside the pot file
- Row encoding/decoding for all four value types: `INTEGER`, `REAL`, `TEXT`, `BLOB`
- `NOT VUOTO` (NOT NULL) constraint enforcement on insert
- Auto-increment Al Dente Key (primary key) generation
- Write-Ahead Log (Prep Log) skeleton — structure in place, full recovery coming in a later phase
- B-tree Strainer index skeleton — API stubbed, implementation coming in a later phase
- Scottatura error type with typed variants and Italian-flavoured messages
- 8 integration tests covering create/drop noodle, insert, scan, persistence, and constraint violations

[Unreleased]: https://github.com/joshuaspratt/marinaraDB/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/joshuaspratt/marinaraDB/releases/tag/v0.1.0
