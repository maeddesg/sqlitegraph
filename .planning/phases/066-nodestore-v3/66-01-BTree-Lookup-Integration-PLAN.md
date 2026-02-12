# Task 66-01: B+Tree Lookup Integration

## Objective

Integrate BTreeManager with NodeStore for node_id → page_id lookups with O(log n) complexity.

## Dependencies

- Phase 63a (V3 Storage Foundation) - Complete
- Phase 63b (V3 Compression Layer) - Complete
- Phase 64 (Page Allocator) - Complete
- Phase 65 (V3 WAL Integration) - Complete

## Approach

1. Create B+Tree lookup integration in `src/backend/native/v3/node/store.rs`:
   - `lookup_page(&self, node_id: i64) -> Option<u64>` - Query B+Tree
   - `lookup_node(&self, node_id: i64) -> Result<NodeRecordV3>` - Full node retrieval
   - Integration with PageAllocator for page tracking

2. Use BTreeManager from Phase 63:
   - `tree: BTreeManager` - B+Tree index reference
   - Methods: `get(&self, key: &i64) -> Option<&[u8]>`

3. Implement node retrieval flow:
   - B+Tree lookup to find page_id containing node_id
   - Load NodePage from disk
   - Decompress NodeRecordV3 from page
   - Return node data

4. Error handling:
   - StorageError for page not found
   - CompressionError for decompression failure

## Success Criteria

- [ ] BTreeManager lookup integration (~150 LOC)
- [ ] Node retrieval with decompression (~50 LOC)
- [ ] Error handling for missing nodes
- [ ] Unit tests for lookup operations
- [ ] V3 module compiles with native-v3

## LOC Estimate

200 LOC

## File

`src/backend/native/v3/node/store.rs` (new module)
