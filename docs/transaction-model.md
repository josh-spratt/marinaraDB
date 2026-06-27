# Transaction Model (Simmers)

A Simmer is MarinaraDB's name for a transaction. Simmers provide full ACID guarantees at the Serializable isolation level.

## Isolation Level

**Serializable** — the strongest standard isolation level. Concurrent Simmers appear to execute one at a time, with no interleaving visible to clients.

In v1, serializability is achieved via a single database-level lock. This is simple and provably correct; it is explicitly a v1 performance trade-off. Post-v1 work can replace it with MVCC or finer-grained locking without changing the external contract.

## Locking Protocol

Two lock modes:

| Mode | Held by | Blocks |
|---|---|---|
| Shared | Any read-only Simmer | Exclusive lock acquisition |
| Exclusive | Any Simmer with writes | All other lock acquisition |

Rules:
- Multiple Simmers may hold shared locks simultaneously.
- An exclusive lock is acquired by the first write operation inside a Simmer (`INSERT`, `UPDATE`, `DELETE`, DDL). It is held until `COMMIT` or `ROLLBACK`.
- Lock acquisition follows strict two-phase locking (2PL): locks are acquired as needed, released only at Simmer end.
- If a Simmer cannot acquire an exclusive lock within **5 seconds**, it receives `LOCK_001` and is automatically rolled back.

## Simmer Lifecycle

```
BEGIN
  → Allocate a new Simmer ID (monotonically increasing u64)
  → Register in the active-simmer table (in memory)
  → No lock yet (lazy acquisition)

[read operations]
  → Acquire shared lock if not already held
  → Read pages from Warming Tray (or disk if not cached)

[write operations]
  → Acquire exclusive lock (blocking; 5s timeout)
  → Write Prep Log record for each page mutation
  → Mutate page in Warming Tray only (never to disk yet)

COMMIT
  → Flush Prep Log buffer to disk (fsync)
  → Write commit record to Prep Log (fsync)
  → Flush dirty Warming Tray pages to .pot file (fsync)
  → Remove Simmer from active-simmer table
  → Release lock

ROLLBACK
  → Evict all dirty pages owned by this Simmer from Warming Tray
  → Write rollback record to Prep Log
  → Remove Simmer from active-simmer table
  → Release lock
```

## Auto-Commit

Statements executed outside a `BEGIN` block are implicitly wrapped in a single-statement Simmer:

```
[implicit BEGIN]
INSERT INTO ...
[implicit COMMIT]
```

The Simmer ID for auto-commit statements is `0` in the Prep Log.

## Nested Transactions

Not supported. A `BEGIN` issued while a Simmer is already active produces `SIMMER_003`. `SAVEPOINT` is out of scope for v1.

## COMMIT / ROLLBACK Without BEGIN

- `COMMIT` with no active Simmer → `SIMMER_001`
- `ROLLBACK` with no active Simmer → `SIMMER_002`

## Dirty Page Ownership

Each dirty page in the Warming Tray carries the Simmer ID that last modified it. On `ROLLBACK`, all pages owned by the rolled-back Simmer are evicted. On `COMMIT`, ownership is cleared and the pages become clean after being flushed.

If the Warming Tray needs to evict a dirty page owned by an in-progress Simmer to make room, it blocks until the owning Simmer commits or rolls back. This preserves the write-ahead guarantee.

## Deadlock

Deadlock is structurally impossible under the single-lock model: there is one resource (the exclusive lock), so a cycle cannot form. The 5-second timeout handles starvation from a long-running writer.
