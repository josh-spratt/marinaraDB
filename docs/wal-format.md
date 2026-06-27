# Prep Log (WAL) Format

The Prep Log is a write-ahead log stored in a separate file: `<name>.prep` alongside the `.pot` file. It is an append-only sequence of records. No page in the `.pot` file is ever written before its corresponding Prep Log record is flushed to disk.

## Log Record Layout

Every record has the same header structure, followed by a variable-length after-image payload:

```
Offset  Size  Field
0       4     Magic: 0x50524550 ("PREP")
4       8     LSN: u64              — Log Sequence Number, monotonically increasing from 1
12      8     Simmer ID: u64        — transaction ID (0 = auto-commit statement)
20      1     Record type: u8
               0x01 = write
               0x02 = commit
               0x03 = rollback
               0x04 = checkpoint
21      3     Reserved: [u8; 3]     — zero-filled
24      8     Page number: u64      — which plate is modified (0 for commit/rollback/checkpoint)
32      2     Offset in page: u16   — byte offset within the page where the after-image starts
34      2     Length: u16           — byte length of the after-image (0 for commit/rollback/checkpoint)
36      N     After-image: [u8; N]  — exact bytes written to the page
36+N    4     CRC32: u32            — checksum of all bytes from offset 0 through 35+N
```

Total minimum record size: 40 bytes (header + CRC32, no payload). Maximum: 40 + 4096 = 4136 bytes (full page image).

## Write Ordering

The write-ahead guarantee is enforced as follows:

1. Before mutating any byte in the Warming Tray's dirty copy of a page, write the corresponding `write` record to the Prep Log buffer.
2. Before returning success to the client for a `COMMIT`, flush the Prep Log buffer to disk (`fsync`), then write and flush the `commit` record.
3. Dirty pages in the Warming Tray are only written to the `.pot` file after the `commit` record is durable.

This ensures the `.pot` file never contains uncommitted data.

## Checkpointing

A checkpoint is triggered when the Prep Log reaches 1000 records (configurable via the file header's reserved bytes in a future version).

Checkpoint procedure:
1. Write a `checkpoint` record to the log (LSN advances).
2. Flush all dirty pages from the Warming Tray to the `.pot` file (`fsync` the `.pot`).
3. `fsync` the `.prep` file.
4. Truncate the `.prep` file, retaining only the checkpoint record and any records written after it.

After a clean checkpoint, crash recovery only needs to replay from the checkpoint record forward.

## Crash Recovery

On startup, if a `.prep` file exists alongside the `.pot` file:

1. Scan all records from the beginning (or from the most recent `checkpoint` record if one exists).
2. Build a set of committed Simmer IDs: any Simmer ID that appears in a `commit` record.
3. For each `write` record:
   - Verify the CRC32. If it fails, stop processing at this record (partial write at crash boundary) and truncate the log here.
   - If the record's Simmer ID is in the committed set, re-apply the after-image to the `.pot` file page.
   - If not committed, discard (no undo needed — uncommitted dirty pages were never flushed).
4. `fsync` the `.pot` file.
5. Truncate and reinitialize the `.prep` file.

Records for uncommitted Simmers are silently discarded. Because dirty pages are never written to `.pot` without WAL, the pot file never requires undo.

## LSN Semantics

LSNs are assigned sequentially from 1. They are never reused, even across checkpoint truncations. The current maximum LSN is stored in the checkpoint record so the sequence can continue correctly after truncation.

```
Checkpoint record payload (stored in after-image field, length = 8):
  [max_lsn_at_checkpoint: u64]
```
