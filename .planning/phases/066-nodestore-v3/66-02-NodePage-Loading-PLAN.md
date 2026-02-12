# Task 66-02: NodePage Loading

## Objective

Implement NodePage loading from disk with decompression of NodeRecordV3 records.

## Dependencies

- Task 66-01 (B+Tree Lookup Integration) - Complete

## Approach

1. Create `PageLoader` component in `src/backend/native/v3/node/store.rs`:
   - `file: Arc<File>` - Underlying V3 database file
   - `page_size: usize` - Page size (4KB default)

2. Implement page loading operations:
   - `load_page(&mut self, page_id: u64) -> Result<NodePage>` - Load full page
   - `load_page_bytes(&self, page_id: u64) -> Result<Vec<u8>>` - Raw bytes

3. Integrate with NodePage module:
   - Use NodePage::unpack() for decompression
   - Handle page checksum validation
   - Parse 10-50 NodeRecordV3 records per page

4. Buffer management:
   - Read buffer for efficient I/O
   - Page alignment handling

## Success Criteria

- [ ] PageLoader struct (~50 LOC)
- [ ] load_page() implementation (~80 LOC)
- [ ] NodePage decompression integration (~50 LOC)
- [ ] Checksum validation (~20 LOC)
- [ ] Unit tests (loading, checksums, errors)
- [ ] V3 module compiles with native-v3

## LOC Estimate

200 LOC

## File

`src/backend/native/v3/node/store.rs` (extended)
