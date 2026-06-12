# RocksDB Column Families for Memory Organ

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)  
**Priority:** HIGH — memory architecture optimization  
**Status:** Proposal (requires Memory organ implementation)

## Current State

Memory organ uses a single RocksDB instance with flat key-value storage. All memory types (episodic, semantic, procedural) share the same keyspace, meaning:
- No per-type compaction or optimization
- Mixed access patterns (sequential episodic vs random semantic) share the same bloom filters
- No per-type TTL or retention policies
- Difficult to query specific memory types efficiently

## Proposal

Separate into RocksDB Column Families:

### Architecture

```rust
// Column Families for Memory Organ
cf_handles = {
    "episodic":   CF — Time-series events, sequential access, TTL-based expiry
    "semantic":   CF — Facts/knowledge, random access, no TTL (permanent)
    "procedural": CF — Skills/patterns, read-heavy, compaction-optimized
    "index":      CF — Cross-reference graph, LSM-tree friendly
    "meta":       CF — Schema, config, stats — small, hot cache
}
```

### Per-CF Tuning

| CF | Write Pattern | Read Pattern | Compaction | TTL |
|----|--------------|-------------|------------|-----|
| episodic | Append-heavy | Sequential range scan | Universal (write-optimized) | 30-90 days |
| semantic | Occasional writes | Random point lookups | Leveled (read-optimized) | None |
| procedural | Rare writes | Frequent reads | Leveled (read-optimized) | None |
| index | Moderate writes | Random + range | Universal | None |
| meta | Infrequent | Hot cache | FIFO | None |

### Expected Benefits

- **3-5x read improvement** for semantic queries (purpose-built bloom filters per CF)
- **2-3x write improvement** for episodic ingestion (universal compaction for append-heavy)
- **Per-type TTL**: episodic memories auto-expire without scanning semantic
- **Independent compaction**: CF-level compaction avoids stalling all reads
- **Cleaner backup/restore**: Export individual CFs

### Implementation

Requires Memory organ (port 9030) to be implemented first. Add CF handles during RocksDB open:

```rust
let mut cf_opts = ColumnFamilyOptions::default();
cf_opts.set_compaction_style(CompactionStyle::Universal);
cf_opts.set_ttl(30 * 24 * 3600); // 30 days for episodic

let db = DB::open_cf_descriptors(&opts, path, vec![
    ColumnFamilyDescriptor::new("episodic", cf_opts),
    ColumnFamilyDescriptor::new("semantic", semantic_opts),
    ColumnFamilyDescriptor::new("procedural", procedural_opts),
    ColumnFamilyDescriptor::new("index", index_opts),
    ColumnFamilyDescriptor::new("meta", meta_opts),
])?;
```