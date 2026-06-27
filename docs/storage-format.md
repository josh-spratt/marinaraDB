# Storage Format

MarinaraDB stores each database (Pot) in a single `.pot` binary file. All multi-byte integers are little-endian. The file is a flat array of 4096-byte pages (Plates).

## File Header (Page 0)

Page 0 is always the file header. It is never used as a heap or B-tree page.

```
Offset  Size  Field
0       4     Magic: 0x4D524442 ("MRDB")
4       4     Version: u32 (current: 1)
8       4     Page size: u32 (always 4096)
12      4     Page count: u32 (total pages including header)
16      8     Root catalog page: u64 (page index of the noodle directory)
24      8     First free page: u64 (head of free-page linked list; 0 = none)
32      4     CRC32: checksum of bytes 0–31
36      4060  Reserved: zero-filled
```

A file that does not begin with the magic bytes `0x4D524442` is rejected with `STOVE_002`.

## Page Header (All Pages 1+)

Every non-header page begins with a 16-byte page header:

```
Offset  Size  Field
0       1     Page type: u8
               0x01 = heap
               0x02 = btree-internal
               0x03 = btree-leaf
               0x04 = overflow
               0x05 = free (on free-page list)
1       1     Flags: u8 (reserved, must be 0)
2       2     Free space offset: u16 (byte offset where next slot entry goes)
4       4     Slot count: u32 (number of slot entries)
8       8     Next page: u64 (next page in chain; 0 if last)
```

## Heap Pages

Heap pages (type `0x01`) use a slotted-page layout. Slot entries grow forward from byte 16; row data grows backward from byte 4095.

### Slot Directory

Each slot entry is 8 bytes:

```
Offset  Size  Field
0       4     Row data offset: u32 (byte offset from start of page)
4       4     Row data length: u32 (0xFFFFFFFF = tombstone / deleted row)
```

`Free space offset` in the page header tracks where the next slot entry starts. Free space available = `4096 - free_space_offset - (sum of all row data lengths)`.

### Row Layout

```
[al_dente_key: i64 (8 bytes)]
[null_bitmap: ceil(col_count / 8) bytes]
[col0_data]
[col1_data]
...
[colN_data]
```

- Al Dente Key is always first, never null, always 8 bytes.
- Null bitmap follows immediately. Bit `i` (LSB-first) = 1 means column `i` is NULL.
- Column data follows in schema-definition order. Fixed-width columns are always present (8 bytes); null flag in bitmap determines whether the bytes are meaningful. Variable-length columns: if null, 4-byte length field with value `0xFFFFFFFF` and no content bytes. If not null and ≤ 512 bytes, 4-byte length + content. If > 512 bytes, 4-byte sentinel `0xFFFFFFFE` + 8-byte overflow page pointer.

### Deleted Rows (Tombstones)

`DELETE` marks the row's slot entry with length `0xFFFFFFFF`. The row data bytes remain on the page until a `VACUUM` reclaims the space. The executor skips tombstoned slots during scans.

## Overflow Pages

Overflow pages (type `0x04`) store `TEXT` or `BLOB` values longer than 512 bytes.

```
Offset  Size  Field
0       16    Page header (next page = continuation page, or 0 if last)
16      4     Total value length: u32 (full byte count across all overflow pages)
20      4076  Content bytes (continues on next page if needed)
```

The on-heap column value is replaced with a sentinel `0xFFFFFFFE` (4 bytes) followed by the first overflow page number (8 bytes).

## The Catalog

The root catalog page (recorded in the file header at offset 16) is a heap page whose rows describe noodle definitions. Each catalog row contains:

```
[al_dente_key: i64]           — internal catalog ID
[null_bitmap: 1 byte]
[name: TEXT]                  — noodle name
[root_heap_page: INTEGER]     — first heap page for this noodle's strands
[root_strainer_page: INTEGER] — first B-tree page for the primary index (0 = none)
[col_defs: BLOB]              — bincode-serialized Vec<ColumnDef>
```

`ColumnDef` is a Rust struct: `{ name: String, type: ColType, nullable: bool, strainer_root: Option<u64> }`.

## Free Page List

Freed pages (from `DROP TABLE` or overflow reclamation) are linked via their `Next page` field and tracked from `First free page` in the file header. New page allocations pull from this list before extending the file.
