# Phase 3a: Prep Log Core — Design

## Goal

Replace the `src/prep_log/mod.rs` stub with a working write-ahead log: an
on-disk record format, an append-only writer with fsync-ordered commit
durability, checkpoint-and-truncate, and crash recovery run on every
`Pot::open`. Wire exactly one real write path — heap insert/delete — through
it end-to-end under the existing hardcoded autocommit Simmer ID `0`, so
recovery is provably exercised by `Pot`'s public API.

Out of scope (deferred to Phase 3b — Simmer transactions): `BEGIN` / `COMMIT`
/ `ROLLBACK` SQL statements, the single-lock Serializable isolation model,
Simmer lifecycle/session state, and real (non-zero) Simmer IDs. This phase's
`PrepLog` API (`append_write` / `append_commit` / `append_rollback` /
`checkpoint`) is designed to be consumed as-is by Phase 3b without changes to
its signatures — only callers change (real Simmer IDs instead of `0`, deferred
flush-to-disk instead of synchronous-per-statement).

## Why this comes before the Strainer

`docs/btree-format.md` requires every Strainer (B-tree index) page mutation to
go through the Prep Log first, and the transaction model's locking also
depends on Simmer lifecycle. The stub comments label `strainer` as "Phase 3"
and `prep_log` as "Phase 4", which inverts the real dependency — the Strainer
(Phase 3c in the new numbering) is built on top of the Prep Log, not the other
way around.

## Architecture

```
Pot::open
  -> if .pot pre-existed: prep_log::recovery::recover(&mut pot_file, prep_path)
       (scans .prep directly, replays committed writes into .pot, returns next_lsn)
  -> Catalog::load / Catalog::init (fresh WarmingTray, post-recovery state)
  -> PrepLog::init(prep_path, next_lsn)   (always creates/truncates a fresh .prep)

Pot::heap_chain_insert / heap_chain_delete
  -> mutate cached Page in WarmingTray in place (existing heap_insert/heap_delete)
  -> build after-image from the page's resulting bytes (full 4096-byte image)
  -> prep_log.append_write(0, page_num, 0, after_image)
  -> prep_log.append_commit(0)                 [fsync, write commit record, fsync]
  -> tray.mark_dirty + tray.flush_simmer(0, ...)   [.pot write happens after WAL is durable]
  -> if prep_log.should_checkpoint(): prep_log.checkpoint(&mut tray, &mut pot_file)
```

## Module design

- `src/prep_log/record.rs` — `RecordType`, `LogRecord` with `encode`/`decode`
  per the 40-byte-header-plus-after-image-plus-CRC32 layout in
  `docs/wal-format.md`. `decode` returns `Ok(None)` for a buffer too short to
  contain a full record (crash-boundary truncation, not an error) and
  `Err(Scottatura::PrepLogCrcMismatch)` for a complete-length record whose
  checksum doesn't match.
- `src/prep_log/mod.rs` — `PrepLog`: `init`, `append_write`, `append_commit`,
  `append_rollback`, `should_checkpoint`, `checkpoint`. Owns the `.prep`
  `File` handle and the next-LSN / since-last-checkpoint counters.
- `src/prep_log/recovery.rs` — `recover(pot_file, prep_path) -> Result<u64, Scottatura>`.
  Scans the `.prep` file forward from byte 0, tracks committed Simmer IDs and
  pending (not-yet-known-committed) writes, replays committed writes into the
  `.pot` file, and returns the LSN to resume numbering from (`1` if no `.prep`
  file exists).
- `src/warming_tray/mod.rs` gains `flush_all` (flush every dirty page
  regardless of owning Simmer, used by checkpoint).
- `src/stove/pot_file.rs` gains `sync_all` (thin wrapper over
  `File::sync_all`, used by checkpoint to fsync the `.pot` file).
- `src/stove/pot.rs`: `Pot` gains a `prep_log: PrepLog` field; `Pot::open` now
  recovers before loading the catalog (recovery must run against a
  cache-free `PotFile` so replayed pages aren't shadowed by a stale
  `WarmingTray` entry); `heap_chain_insert`/`heap_chain_delete` route through
  a shared `log_and_flush` helper that logs, commits, flushes, and checks for
  a checkpoint.

## Locked-in design decisions (binding on Phase 3b)

- **Full-page after-images.** Every `Write` record in this phase logs the
  complete mutated 4096-byte page, not a byte-range diff. This makes replay a
  trivial `page.data = after_image` with no read-modify-write, at the cost of
  larger log files. The record format's `offset`/`length` fields remain
  generic (`docs/wal-format.md` already specifies them at the byte level) so
  a later phase can switch to partial-page images without a format change.
- **No `Page::Clone`.** The cached page is mutated in place by the existing
  `heap_insert`/`heap_delete`, and the after-image is read back out of that
  same mutated buffer. This is safe because nothing observes the page between
  the in-memory mutation and the log write completing — there are no
  concurrent readers yet (that arrives with Phase 3b's shared/exclusive
  locking).
- **`.prep` is fully reinitialized on every `Pot::open`.** After recovery
  replay completes, `PrepLog::init` always creates a fresh, empty `.prep`
  file starting at the recovered `next_lsn`. This is safe today because
  autocommit statements under Simmer ID `0` never overlap within a session —
  there is no scenario where a second `.prep` file's records need to coexist
  with the first's. Once Phase 3b introduces concurrent/long-lived Simmers,
  this needs revisiting (an in-flight Simmer's uncommitted writes must not be
  silently discarded by a reinit that happens for an unrelated reason).

## New Scottatura variants

- `PrepLogCrcMismatch { lsn: u64 }` — `STOVE_003`, matches
  `docs/error-taxonomy.md` verbatim (already reserved there).
- `PrepLogIoError { message: String }` — `STOVE_005`, matches
  `docs/error-taxonomy.md` verbatim (already reserved there). Can't reuse the
  existing `Scottatura::IoError(#[from] std::io::Error)` (`STOVE_004`, "I/O
  error on pot file") since that's a `#[from]` conversion already claimed for
  `.pot`-file I/O — `.prep`-file I/O errors are converted manually at each
  call site with `.map_err(...)`.

## Testing

- Unit tests colocated in `record.rs` (encode/decode round-trips per record
  type, truncated-buffer and corrupted-CRC handling, back-to-back record
  decoding), `mod.rs` (writer behavior, LSN monotonicity, checkpoint
  threshold and truncation), `recovery.rs` (replay/discard/checkpoint-reset
  semantics against hand-built `.prep` byte sequences), `warming_tray/mod.rs`
  (`flush_all`), and `pot_file.rs` (`sync_all`).
- Integration tests in `tests/prep_log_recovery.rs`, all driven through
  `Pot::open`/`Pot::create_table`/`Pot::insert_strand`/`Pot::scan_noodle`:
  1. A committed insert's page is corrupted directly in the `.pot` file
     (simulating a crash where the flush was lost) and is restored by
     recovery on reopen.
  2. A garbage trailing fragment appended to `.prep` (simulating a torn
     write) is silently discarded by recovery without losing an earlier
     valid committed write, and the reopened `Pot` remains writable.
  3. Enough inserts to cross `CHECKPOINT_THRESHOLD` (1000 records / 500
     inserts) measurably shrink the `.prep` file versus its pre-checkpoint
     size.
- `cargo test` and `cargo clippy --all-targets -- -D warnings` clean after
  every task, consistent with prior phases.
