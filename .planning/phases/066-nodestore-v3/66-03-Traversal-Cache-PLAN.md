# Task 66-03: Traversal Cache

## Objective

Implement LRU cache for per-traversal page caching to reduce redundant disk I/O during graph traversals.

## Dependencies

- Task 66-01 (B+Tree Lookup Integration) - Complete
- Task 66-02 (NodePage Loading) - Complete

## Approach

1. Create `TraversalCache` in `src/backend/native/v3/node/store.rs`:
   - `cache: LruCache<u64, Arc<NodePage>>` - LRU page cache
   - `capacity: usize` - Maximum pages to cache (default: 16)

2. Implement LRU cache operations:
   - `get(&mut self, page_id: u64) -> Option<Arc<NodePage>>` - Cache lookup
   - `insert(&mut self, page_id: u64, page: Arc<NodePage>)` - Add page
   - `invalidate(&mut self, page_id: u64)` - Remove page from cache
   - `clear(&mut self)` - Empty cache
   - `len(&self) -> usize` - Current cache size

3. Cache coherency:
   - Invalidate cache entries on page writes
   - Flush cache on checkpoint
   - Per-traversal lifecycle (create at start, drop at end)

4. Performance considerations:
   - Cache hit/miss tracking
   - Configurable capacity
   - Memory-efficient storage (Arc<NodePage> sharing)

## Success Criteria

- [ ] TraversalCache struct (~60 LOC)
- [ ] LRU cache operations (~40 LOC)
- [ ] Cache invalidation on writes (~20 LOC)
- [ ] Unit tests (cache hit/miss, LRU eviction, invalidation)
- [ ] V3 module compiles with native-v3

## LOC Estimate

100 LOC

## File

`src/backend/native/v3/node/store.rs` (extended)
