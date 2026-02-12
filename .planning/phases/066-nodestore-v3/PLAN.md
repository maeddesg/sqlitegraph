# Phase 66: NodeStore V3

## Objective

Implement page-based node storage with B+Tree index lookup for unlimited node capacity with O(log n) complexity.

## Dependencies

- Phase 63a (V3 Storage Foundation) - Complete
- Phase 63b (V3 Compression Layer) - Complete
- Phase 64 (Page Allocator) - Complete
- Phase 65 (V3 WAL Integration) - Complete

## Approach

1. Create `NodeStoreV3` struct in `src/backend/native/v3/node/store.rs`:
   - `btree: BTreeManager` - B+Tree for node_id → page_id lookups
   - `page_cache: LruCache<PageId, Vec<u8>>` - Per-traversal page caching
   - `page_loader: PageLoader` - Load pages from disk
   - `allocator: Arc<PageAllocator>` - Page allocation interface

2. Implement B+Tree lookup operations:
   - `get_node(&mut self, node_id: i64) -> Option<NodeRecordV3>` - Main lookup
   - `load_page(&mut self, page_id: u64) -> Result<NodePage>` - Load node page
   - `get_neighbors(&mut self, node_id: i64) -> Result<Vec<i64>>` - Neighbor queries

3. Integrate with existing V3 components:
   - Use BTreeManager for index lookups
   - Use PageAllocator for page allocation
   - Use V3WALRecord types for logging

4. Add per-traversal cache:
   - LRU cache for recently accessed pages
   - Invalidate on page writes
   - Flush on checkpoint

## Success Criteria

- [ ] BTreeManager lookup integration (~150 LOC)
- [ ] NodePage loading and decompression (~200 LOC)
- [ ] Traversal cache implementation (~100 LOC)
- [ ] NodeStore V3 tests (~150 LOC)
- [ ] V3 module compiles with native-v3

## LOC Estimate

600 LOC

## Files

`src/backend/native/v3/node/store.rs` (new module)

## V3 NodeStore Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    NodeStore V3 Flow                         │
├─────────────────────────────────────────────────────────────────┤
│ Query: get_node(node_id)                                     │
│                                                              │
│    ┌──────────────────────────────────────────────────────┐   │
│    │ 1. B+Tree Lookup: node_id → page_id?        │   │
│    │    Uses BTreeManager to find which page contains     │   │
│    │    the target node_id                                 │   │
│    └──────────────────────────────────────────────────────┘   │
│                                                              │
│    ┌──────────────────────────────────────────────────────┐   │
│    │ 2. Check Page Cache                             │   │
│    │    Is page_id already in memory?                  │   │
│    │    If YES → Use cached NodePage               │   │
│    │    If NO → Continue to step 3                 │   │
│    └──────────────────────────────────────────────────────┘   │
│                                                              │
│    ┌──────────────────────────────────────────────────────┐   │
│    │ 3. Load NodePage                                 │   │
│    │    Read page_id from disk                        │   │
│    │    Decompress NodeRecordV3 records              │   │
│    │    Find target node_id in page               │   │
│    │    Return NodeRecordV3                         │   │
│    └──────────────────────────────────────────────────────┘   │
│                                                              │
└─────────────────────────────────────────────────────────────────────────┘

Cache Coherency:
- Page writes invalidate cached pages
- Checkpoint flushes all cache entries
- LRU eviction when cache full
```

## Implementation Notes

1. **B+Tree Integration**: Use BTreeManager from Phase 63 for node_id → page_id mapping

2. **Page Decompression**: NodePage stores 10-50 compressed NodeRecordV3 records
   - delta encoding: node_id = page_base_id + id_delta
   - varint encoding for variable-length fields
   - String table lookup for kind/name

3. **Traversal Cache**:
   - LRU cache with configurable size (default: 16 pages = 64KB)
   - Per-traversal: create fresh cache at start, drop at end
   - Reduces page reads for sequential access patterns

4. **Integration with WAL**:
   - NodePage loads logged via V3WALRecord::PageWrite
   - Node updates logged via V3WALRecord::NodeUpdate
   - Page allocation logged via V3WALRecord::PageAllocate

5. **Error Handling**:
   - StorageError for page I/O failures
   - CompressionError for decompression failures
   - NotFoundError for missing node_id

## Testing Strategy

1. **Unit Tests**:
   - B+Tree lookup correctness
   - Page cache hit/miss rates
   - Node decompression
   - Error handling paths

2. **Integration Tests**:
   - End-to-end node storage/retrieval
   - Cache effectiveness
   - Concurrent access patterns
