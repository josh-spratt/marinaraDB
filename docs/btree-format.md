# Strainer (B-tree Index) Format

Each Strainer is a B+ tree stored in pages within the same `.pot` file. Two page types are used: internal nodes (`0x02`) and leaf nodes (`0x03`). All structural changes to the B-tree are logged to the Prep Log before the page is mutated.

## Internal Node Layout

After the standard 16-byte page header:

```
Offset  Size  Field
0       2     Key count: u16
2       N     Keys: encoded column values, concatenated (see type-system.md for encoding)
2+N     M     Child pointers: [u64; key_count + 1] (page numbers, left-to-right)
```

An internal node with `k` keys has `k + 1` child pointers. Key `i` is the separator between child `i` and child `i + 1`. Lookup descends left if `search_key < key[i]`, right otherwise.

## Leaf Node Layout

After the standard 16-byte page header:

```
Offset  Size  Field
0       2     Key count: u16
2       8     Right sibling page: u64 (0 if rightmost leaf)
10      N     Entries: [(encoded_key, al_dente_key: i64); key_count]
              Each entry: encoded_key bytes + 8-byte i64 Al Dente Key
```

Leaf nodes store `(indexed_value → al_dente_key)` pairs in sorted order. The Al Dente Key is used by the executor to fetch the full row from the heap. Non-unique indexes store multiple entries with the same key value but different Al Dente Keys.

## Node Capacity

Maximum keys per node = `floor((4096 - 16 - 10) / (max_key_size + 8))`

For `INTEGER` indexes: max_key_size = 8, so capacity = `floor(4070 / 16)` = **254 keys per leaf node**.

Nodes split when they reach capacity. The median key is promoted to the parent. If the root splits, a new root page is allocated and the old root becomes a child.

## Operations

### `lookup(value) → Vec<i64>`

1. Start at root page.
2. On internal nodes: binary-search keys, follow the appropriate child pointer.
3. On leaf nodes: binary-search entries, collect all Al Dente Keys matching `value`.
4. Returns an empty `Vec` if the value is not found.

### `range_scan(lo, hi) → impl Iterator<Item = i64>`

1. Descend to the leftmost leaf node where key ≥ `lo`.
2. Iterate entries on the leaf, emitting Al Dente Keys where `lo ≤ key ≤ hi`.
3. Follow `right_sibling_page` pointers to continue across leaf nodes.
4. Stop when `key > hi` or `right_sibling_page = 0`.

### `insert(value, al_dente_key)`

1. Descend to the appropriate leaf node.
2. Insert the entry in sorted order.
3. If the leaf is at capacity: split the leaf, promote the median key to the parent.
4. If the parent is at capacity: split the parent recursively.
5. If the root splits: allocate a new root, write it as a Prep Log `write` record.
6. Each mutated page is written to the Prep Log before modification.

### `delete(value, al_dente_key)`

1. Descend to the appropriate leaf node.
2. Mark the matching entry as a tombstone (set Al Dente Key = `i64::MIN`).
3. Physical removal and node merging are deferred to a future `VACUUM` pass. This avoids rebalancing complexity in v1.

## Strainer Stats

The `strainer_stats()` API returns per-index counters maintained in memory (not persisted across restarts):

```rust
pub struct StrainerStats {
    pub name: String,
    pub lookups: u64,
    pub hits: u64,       // lookup found at least one result
    pub misses: u64,     // lookup returned empty
    pub splits: u64,     // node splits since open
}
```

These are exposed by the programmatic client API and surfaced in the post-v1 `EXPLAIN` output.

## Root Page Tracking

Each index's root page number is stored in the noodle's catalog row (`col_defs` BLOB, field `strainer_root: Option<u64>`). When the root splits, the catalog row is updated atomically within the same Simmer.
