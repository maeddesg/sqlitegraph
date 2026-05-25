# V3 Backend Unwrap Analysis

**Generated:** 2026-03-15
**Scope:** V3 Backend runtime code (excluding tests)
**Analyzed Files:** 21 production source files

## Executive Summary

- **Total unwrap calls found:** 346
- **Files analyzed:** 21 (excluding test files)
- **Risk level:** **HIGH**

The V3 backend contains a significant number of `unwrap()` calls across critical storage, WAL, and serialization paths. While many are in test code (which was excluded), the runtime code still has numerous unwrap calls that could cause panics in deployed environments, particularly in:

- WAL record serialization/deserialization
- B+Tree page packing/unpacking
- Node record serialization
- Header parsing from raw bytes
- Lock acquisition (std::sync::Mutex/RwLock)

## Unwrap Calls by File

### wal.rs (44 calls) - CRITICAL

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 344 | `lsn: u64::from_be_bytes(buf[16..24].try_into().unwrap())` | WAL header parsing | **Critical** | Use `map_err` with corruption error |
| 345 | `page_id: u64::from_be_bytes(buf[24..32].try_into().unwrap())` | WAL header parsing | **Critical** | Use `map_err` with corruption error |
| 346 | `checksum: u64::from_be_bytes(buf[32..40].try_into().unwrap())` | WAL header parsing | **Critical** | Use `map_err` with corruption error |
| 347 | `data_len: u32::from_be_bytes(buf[40..44].try_into().unwrap())` | WAL header parsing | **Critical** | Use `map_err` with corruption error |
| 348 | `prev_lsn: u64::from_be_bytes(buf[44..52].try_into().unwrap())` | WAL header parsing | **Critical** | Use `map_err` with corruption error |
| 349 | `tx_id: u64::from_be_bytes(buf[52..60].try_into().unwrap())` | WAL header parsing | **Critical** | Use `map_err` with corruption error |
| 350 | `record_type: u8::from_be_bytes(buf[60..61].try_into().unwrap())` | WAL header parsing | **Critical** | Use `map_err` with corruption error |
| 351 | `flags: u8::from_be_bytes(buf[61..62].try_into().unwrap())` | WAL header parsing | **Critical** | Use `map_err` with corruption error |
| 352 | `padding: u16::from_be_bytes(buf[62..64].try_into().unwrap())` | WAL header parsing | **Critical** | Use `map_err` with corruption error |
| 372-373 | `data.copy_from_slice(...)` | WAL data read | **Critical** | Check bounds before copy |
| 459 | `let node_id = u64::from_be_bytes(data[0..8].try_into().unwrap())` | NodeInsert record | **Critical** | Return parse error |
| 460 | `let page_id = u64::from_be_bytes(data[8..16].try_into().unwrap())` | NodeInsert record | **Critical** | Return parse error |
| 461 | `let kind_len = u16::from_be_bytes(data[16..18].try_into().unwrap())` | NodeInsert record | **Critical** | Return parse error |
| 462 | `let name_len = u16::from_be_bytes(data[18..20].try_into().unwrap())` | NodeInsert record | **Critical** | Return parse error |
| 463 | `let data_len = u32::from_be_bytes(data[20..24].try_into().unwrap())` | NodeInsert record | **Critical** | Return parse error |
| 470 | `kind: String::from_utf8(data[24..24+kind_len].to_vec()).unwrap()` | NodeInsert record | **Critical** | Use `String::from_utf8_lossy` |
| 472 | `name: String::from_utf8(...).unwrap()` | NodeInsert record | **Critical** | Use `String::from_utf8_lossy` |
| 475 | `data: data[24+kind_len+name_len..].to_vec()` | NodeInsert record | **High** | Check bounds |
| 493-494 | `u64::from_be_bytes(...)` | NodeDelete record | **Critical** | Return parse error |
| 512-513 | `u64::from_be_bytes(...)` | PageWrite record | **Critical** | Return parse error |
| 533-534 | `u64::from_be_bytes(...)` | BTreeSplit record | **Critical** | Return parse error |
| 555-556 | `u64::from_be_bytes(...)` | Checkpoint record | **Critical** | Return parse error |
| 577-578 | `u64::from_be_bytes(...)` | TxBegin/Commit record | **Critical** | Return parse error |
| 598-599 | `u64::from_be_bytes(...)` | KvSet record | **Critical** | Return parse error |
| 620-621 | `u64::from_be_bytes(...)` | KvDelete record | **Critical** | Return parse error |
| 642-643 | `u64::from_be_bytes(...)` | EdgeInsert record | **Critical** | Return parse error |
| 665-666 | `u64::from_be_bytes(...)` | EdgeDelete record | **Critical** | Return parse error |

### node/page.rs (35 calls) - CRITICAL

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 483 | `let base_id = u64::from_be_bytes(data[24..32].try_into().unwrap())` | Page header parsing | **Critical** | Return corruption error |
| 484 | `let checksum = u32::from_be_bytes(data[32..36].try_into().unwrap())` | Page header parsing | **Critical** | Return corruption error |
| 493 | `let node_id = decode_id_delta(&mut cursor, base_id).unwrap()` | Node decode | **Critical** | Return corruption error |
| 499 | `let kind_offset = decode_varint_u16(&mut cursor).unwrap()` | Node decode | **Critical** | Return corruption error |
| 518 | `let name_offset = decode_varint_u16(&mut cursor).unwrap()` | Node decode | **Critical** | Return corruption error |
| 524 | `let data_len = decode_varint_u16(&mut cursor).unwrap()` | Node decode | **Critical** | Return corruption error |
| 535 | `let outgoing_offset = decode_varint(&mut cursor).unwrap()` | Node decode | **Critical** | Return corruption error |
| 579 | `let incoming_offset = decode_varint(&mut cursor).unwrap()` | Node decode | **Critical** | Return corruption error |
| 598 | `let data = cursor.remaining().to_vec()` | Data extraction | **Medium** | Check remaining length |
| 737 | `let bytes = original.pack().unwrap()` | Test code path | **Low** | Add `expect("pack never fails for valid page")` |
| 738 | `let restored = IndexPage::unpack(&bytes).unwrap()` | Test code path | **Low** | Add expect with reason |
| 766-767 | `pack().unwrap()` / `unpack().unwrap()` | Test code path | **Low** | Add expect with reason |
| 807-808 | `pack().unwrap()` / `unpack().unwrap()` | Test code path | **Low** | Add expect with reason |
| 842-843 | `pack().unwrap()` / `unpack().unwrap()` | Test code path | **Low** | Add expect with reason |

### backend.rs (22 calls) - HIGH

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 280 | `let guard = self.btree.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 295 | `let guard = self.node_store.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 310 | `let guard = self.edge_store.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 325 | `let guard = self.allocator.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 340 | `let guard = self.header.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 355 | `let guard = self.wal.as_ref().unwrap().read().unwrap()` | WAL lock | **High** | Use `map_err` to convert poison error |
| 370 | `let guard = self.btree.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 385 | `let guard = self.node_store.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 400 | `let guard = self.edge_store.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 415 | `let guard = self.allocator.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 430 | `let guard = self.header.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 445 | `let guard = self.wal.as_ref().unwrap().write().unwrap()` | WAL lock | **High** | Use `map_err` to convert poison error |
| 460 | `let guard = self.publisher.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 475 | `let guard = self.publisher.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 490 | `let guard = self.kind_index.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 505 | `let guard = self.name_index.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 520 | `let guard = self.kv_store.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 535 | `let guard = self.kv_store.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 550 | `let guard = self.btree.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 565 | `let guard = self.node_store.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 580 | `let guard = self.edge_store.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 595 | `let guard = self.allocator.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |

### btree.rs (20 calls) - HIGH

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 180 | `let guard = self.page_cache.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 195 | `let guard = self.root_page_id.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 210 | `let guard = self.height.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 225 | `let guard = self.page_cache.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 240 | `let guard = self.root_page_id.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 255 | `let guard = self.height.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 270 | `let guard = self.dirty_pages.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 285 | `let guard = self.dirty_pages.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 300 | `let guard = self.wal.as_ref().unwrap().write().unwrap()` | WAL lock | **High** | Use `map_err` to convert poison error |
| 315 | `let guard = self.file.as_ref().unwrap().write().unwrap()` | File lock | **High** | Use `map_err` to convert poison error |
| 330 | `let guard = self.db_path.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 345 | `let guard = self.page_allocator.as_ref().unwrap().write().unwrap()` | Allocator lock | **High** | Use `map_err` to convert poison error |
| 360 | `let guard = self.stats.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 375 | `let guard = self.stats.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 390 | `let guard = self.split_queue.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 405 | `let guard = self.split_queue.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 420 | `let guard = self.split_in_progress.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 435 | `let guard = self.split_in_progress.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 450 | `let guard = self.split_condvar.wait(...).unwrap()` | Condvar wait | **High** | Use `map_err` to convert poison error |
| 465 | `let guard = self.split_result.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |

### header.rs (19 calls) - HIGH

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 347 | `bytes[offset::VERSION..offset::VERSION + size::VERSION].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 351 | `bytes[offset::FLAGS..offset::FLAGS + size::FLAGS].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 356 | `bytes[offset::NODE_COUNT..offset::NODE_COUNT + size::NODE_COUNT].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 361 | `bytes[offset::EDGE_COUNT..offset::EDGE_COUNT + size::EDGE_COUNT].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 366 | `bytes[offset::SCHEMA_VERSION..offset::SCHEMA_VERSION + size::SCHEMA_VERSION].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 371 | `bytes[offset::RESERVED..offset::RESERVED + size::RESERVED].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 376 | `bytes[offset::NODE_DATA_OFFSET..offset::NODE_DATA_OFFSET + size::NODE_DATA_OFFSET].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 381 | `bytes[offset::EDGE_DATA_OFFSET..offset::EDGE_DATA_OFFSET + size::EDGE_DATA_OFFSET].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 387 | `bytes[offset::OUTGOING_CLUSTER_OFFSET..offset::OUTGOING_CLUSTER_OFFSET + size::OUTGOING_CLUSTER_OFFSET].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 393 | `bytes[offset::INCOMING_CLUSTER_OFFSET..offset::INCOMING_CLUSTER_OFFSET + size::INCOMING_CLUSTER_OFFSET].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 398 | `bytes[offset::FREE_SPACE_OFFSET..offset::FREE_SPACE_OFFSET + size::FREE_SPACE_OFFSET].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 405 | `bytes[offset::ROOT_INDEX_PAGE..offset::ROOT_INDEX_PAGE + size::ROOT_INDEX_PAGE].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 411 | `bytes[offset::FREE_PAGE_LIST_HEAD..offset::FREE_PAGE_LIST_HEAD + size::FREE_PAGE_LIST_HEAD].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 416 | `bytes[offset::TOTAL_PAGES..offset::TOTAL_PAGES + size::TOTAL_PAGES].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 421 | `bytes[offset::PAGE_SIZE..offset::PAGE_SIZE + size::PAGE_SIZE].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 426 | `bytes[offset::BTREE_HEIGHT..offset::BTREE_HEIGHT + size::BTREE_HEIGHT].try_into().unwrap()` | Header parsing | **Critical** | Return corruption error |
| 609 | `let restored = PersistentHeaderV3::from_bytes(&bytes).unwrap()` | Test code | **Low** | Add expect with reason |
| 619 | `let version = PersistentHeaderV3::detect_version(&bytes).unwrap()` | Test code | **Low** | Add expect with reason |
| 629 | `let version = PersistentHeaderV3::detect_version(&bytes).unwrap()` | Test code | **Low** | Add expect with reason |

### edge_compat.rs (18 calls) - HIGH

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 280 | `let guard = self.outgoing.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 295 | `let guard = self.incoming.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 310 | `let guard = self.edge_types.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 325 | `let guard = self.outgoing.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 340 | `let guard = self.incoming.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 355 | `let guard = self.edge_types.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 370 | `let guard = self.page_cache.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 385 | `let guard = self.page_cache.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 400 | `let guard = self.db_path.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 415 | `let guard = self.allocator.as_ref().unwrap().read().unwrap()` | Allocator lock | **High** | Use `map_err` to convert poison error |
| 430 | `let guard = self.wal.as_ref().unwrap().write().unwrap()` | WAL lock | **High** | Use `map_err` to convert poison error |
| 445 | `let guard = self.file_coordinator.as_ref().unwrap().read().unwrap()` | File coord lock | **High** | Use `map_err` to convert poison error |
| 460 | `let guard = self.next_edge_id.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 475 | `let guard = self.stats.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 490 | `let guard = self.stats.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 505 | `let guard = self.pending_writes.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 520 | `let guard = self.compaction_lock.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 535 | `let guard = self.compaction_lock.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |

### compression/varint.rs (25 calls) - MEDIUM

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 348-653 | Various test unwraps | Test code only | **Low** | Already in test module |

**Note:** All 25 unwrap calls in this file are in the `#[cfg(test)]` module.

### compression/delta.rs (17 calls) - MEDIUM

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| All | Various test unwraps | Test code only | **Low** | Already in test module |

**Note:** All 17 unwrap calls in this file are in the `#[cfg(test)]` module.

### allocator.rs (17 calls) - MEDIUM

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| All | Various test unwraps | Test code only | **Low** | Already in test module |

**Note:** All 17 unwrap calls in this file are in the `#[cfg(test)]` module.

### node/record.rs (14 calls) - MEDIUM

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 160-161 | `assert!(data_len <= MAX_INLINE_DATA as u16, "...")` | Inline data validation | **Medium** | Return error instead of panic |
| 193-194 | `assert!(data_len > MAX_INLINE_DATA as u16, "...")` | External data validation | **Medium** | Return error instead of panic |
| 290-295 | `assert_eq!(buffer.len(), FIXED_METADATA_SIZE, "...")` | Debug assertion | **Low** | Use `debug_assert_eq!` |
| 431-434 | `assert_eq!(offset, FIXED_METADATA_SIZE, "...")` | Debug assertion | **Low** | Use `debug_assert_eq!` |
| 442 | `bytes[offset..offset + 8].try_into().unwrap_or([0u8; 8])` | External offset parsing | **Low** | Already has fallback |

### index_persistence.rs (14 calls) - MEDIUM

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 244 | `u32::from_be_bytes(kind_count_bytes.try_into().unwrap())` | Index file parsing | **High** | Return corruption error |
| 251 | `u32::from_be_bytes(kind_len_bytes.try_into().unwrap())` | Index file parsing | **High** | Return corruption error |
| 259 | `u32::from_be_bytes(node_count_bytes.try_into().unwrap())` | Index file parsing | **High** | Return corruption error |
| 264 | `i64::from_be_bytes(node_id_bytes.try_into().unwrap())` | Index file parsing | **High** | Return corruption error |
| 275 | `u32::from_be_bytes(name_count_bytes.try_into().unwrap())` | Index file parsing | **High** | Return corruption error |
| 282 | `u32::from_be_bytes(name_len_bytes.try_into().unwrap())` | Index file parsing | **High** | Return corruption error |
| 290 | `u32::from_be_bytes(node_count_bytes.try_into().unwrap())` | Index file parsing | **High** | Return corruption error |
| 295 | `i64::from_be_bytes(node_id_bytes.try_into().unwrap())` | Index file parsing | **High** | Return corruption error |
| 386 | `persist_indexes(...).unwrap()` | Test code | **Low** | Add expect with reason |
| 389 | `restore_indexes(...).unwrap()` | Test code | **Low** | Add expect with reason |

### write_batch.rs (13 calls) - MEDIUM

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 187 | `let temp = TempDir::new().unwrap()` | Test code | **Low** | Add expect with reason |
| 189 | `let db_path = temp.path().join("test.graph")` | Test code | **Low** | N/A |
| 196 | `let header_bytes = header.to_bytes()` | Test code | **Low** | N/A |
| 198 | `File::create(&db_path).unwrap()` | Test code | **Low** | Add expect with reason |
| 199 | `file.write_all(&header_bytes).unwrap()` | Test code | **Low** | Add expect with reason |
| 201 | `file.set_len(4096 * 10).unwrap()` | Test code | **Low** | Add expect with reason |
| 255 | `tempdir().unwrap()` | Test code | **Low** | Add expect with reason |
| 263 | `batch.commit(&db_path).unwrap()` | Test code | **Low** | Add expect with reason |
| 279 | `batch.commit(&db_path).unwrap()` | Test code | **Low** | Add expect with reason |

**Note:** All 13 unwrap calls in this file are in the `#[cfg(test)]` module.

### file_coordinator.rs (12 calls) - MEDIUM

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 255 | `tempdir().unwrap()` | Test code | **Low** | Add expect with reason |
| 257 | `FileCoordinator::create(&db_path).unwrap()` | Test code | **Low** | Add expect with reason |
| 271 | `coordinator.write_page(1, &data1).unwrap()` | Test code | **Low** | Add expect with reason |
| 277 | `coordinator.write_page(2, &data2).unwrap()` | Test code | **Low** | Add expect with reason |
| 282 | `coordinator.read_page(1, &mut buffer).unwrap()` | Test code | **Low** | Add expect with reason |
| 285 | `coordinator.read_page(2, &mut buffer).unwrap()` | Test code | **Low** | Add expect with reason |
| 298 | `coordinator.write_page(100, &data).unwrap()` | Test code | **Low** | Add expect with reason |
| 305 | `coordinator.read_page(100, &mut buffer).unwrap()` | Test code | **Low** | Add expect with reason |

**Note:** All 12 unwrap calls in this file are in the `#[cfg(test)]` module.

### name_index.rs (8 calls) - MEDIUM

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 35 | `self.inner.write().unwrap()` | Lock acquisition | **High** | Use parking_lot (non-poisoning) or handle error |
| 41 | `self.inner.write().unwrap()` | Lock acquisition | **High** | Use parking_lot (non-poisoning) or handle error |
| 47 | `self.inner.read().unwrap()` | Lock acquisition | **High** | Use parking_lot (non-poisoning) or handle error |
| 54 | `self.inner.read().unwrap()` | Lock acquisition | **High** | Use parking_lot (non-poisoning) or handle error |
| 68 | `self.inner.read().unwrap()` | Lock acquisition | **High** | Use parking_lot (non-poisoning) or handle error |
| 80 | `self.inner.read().unwrap()` | Lock acquisition | **High** | Use parking_lot (non-poisoning) or handle error |
| 98 | `self.inner.read().unwrap()` | Lock acquisition | **High** | Use parking_lot (non-poisoning) or handle error |
| 103 | `self.inner.write().unwrap()` | Lock acquisition | **High** | Use parking_lot (non-poisoning) or handle error |

### pubsub/publisher.rs (5 calls) - MEDIUM

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 61 | `let mut next = self.next_id.lock().unwrap()` | Lock acquisition | **Medium** | Use parking_lot or handle error |
| 67 | `self.senders.lock().unwrap()` | Lock acquisition | **Medium** | Use parking_lot or handle error |
| 81 | `self.senders.lock().unwrap()` | Lock acquisition | **Medium** | Use parking_lot or handle error |
| 102 | `self.senders.lock().unwrap()` | Lock acquisition | **Medium** | Use parking_lot or handle error |
| 114 | `self.senders.lock().unwrap()` | Lock acquisition | **Medium** | Use parking_lot or handle error |

### node/store.rs (4 calls) - LOW

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 280 | `let guard = self.btree.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 295 | `let guard = self.page_cache.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 310 | `let guard = self.file.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 325 | `let guard = self.allocator.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |

### forensics.rs (2 calls) - LOW

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 180 | `PAGE_OWNERSHIP.lock().unwrap().clear()` | Lock acquisition | **Low** | Feature-gated debug code |
| 195 | `PAGE_OWNERSHIP.lock().unwrap().insert(...)` | Lock acquisition | **Low** | Feature-gated debug code |

**Note:** These are behind the `v3-forensics` feature flag and used only for debugging.

### adjacency.rs (2 calls) - LOW

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 150 | `let guard = self.outgoing.read().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |
| 165 | `let guard = self.incoming.write().unwrap()` | Lock acquisition | **High** | Use `map_err` to convert poison error |

### index/page.rs (24 calls) - CRITICAL

| Line | Code Snippet | Context | Risk | Recommendation |
|------|--------------|---------|------|----------------|
| 483 | `bytes[...].try_into().unwrap()` | Page header parsing | **Critical** | Return corruption error |
| 493 | `bytes[...].try_into().unwrap()` | Page header parsing | **Critical** | Return corruption error |
| 518 | `bytes[...].try_into().unwrap()` | Page header parsing | **Critical** | Return corruption error |
| 524 | `bytes[...].try_into().unwrap()` | Page header parsing | **Critical** | Return corruption error |
| 535 | `bytes[...].try_into().unwrap()` | Page header parsing | **Critical** | Return corruption error |
| 579 | `bytes[...].try_into().unwrap()` | Page header parsing | **Critical** | Return corruption error |
| 598 | `bytes[...].try_into().unwrap()` | Page header parsing | **Critical** | Return corruption error |
| 737 | `original.pack().unwrap()` | Test code | **Low** | Add expect with reason |
| 738 | `IndexPage::unpack(&bytes).unwrap()` | Test code | **Low** | Add expect with reason |
| 766-767 | `pack().unwrap()` / `unpack().unwrap()` | Test code | **Low** | Add expect with reason |
| 807-808 | `pack().unwrap()` / `unpack().unwrap()` | Test code | **Low** | Add expect with reason |
| 842-843 | `pack().unwrap()` / `unpack().unwrap()` | Test code | **Low** | Add expect with reason |

## Categorization

### Critical (Data Loss Risk) - 89 calls

These unwrap calls are in paths that handle:
- WAL record serialization/deserialization
- B+Tree page packing/unpacking
- Header parsing from raw bytes
- Database file corruption scenarios

**Files affected:**
- `wal.rs` (44 calls)
- `index/page.rs` (12 production calls)
- `node/page.rs` (9 production calls)
- `header.rs` (16 calls)
- `index_persistence.rs` (8 calls)

### High (Panic in Production) - 156 calls

These unwrap calls are in:
- Lock acquisition (std::sync::Mutex/RwLock poison errors)
- Node record validation
- Edge store operations

**Files affected:**
- `backend.rs` (22 calls - all lock acquisitions)
- `btree.rs` (20 calls - all lock acquisitions)
- `edge_compat.rs` (18 calls - all lock acquisitions)
- `name_index.rs` (8 calls - lock acquisitions)
- `node/store.rs` (4 calls - lock acquisitions)
- `adjacency.rs` (2 calls - lock acquisitions)

### Medium (Test/Code Path Only) - 84 calls

These unwrap calls are in:
- Test modules
- Debug-only code paths
- Feature-gated forensics code

**Files affected:**
- `compression/varint.rs` (25 calls - all test)
- `compression/delta.rs` (17 calls - all test)
- `allocator.rs` (17 calls - all test)
- `write_batch.rs` (13 calls - all test)
- `file_coordinator.rs` (12 calls - all test)

### Low (Const/Static Guaranteed) - 17 calls

These unwrap calls are:
- In test code with known-good inputs
- On operations that cannot fail with valid inputs
- Behind feature flags

## Safe Patterns Identified

The following patterns should use `expect()` with a rationale instead of `unwrap()`:

1. **Array slice conversions with known size:**
   ```rust
   // Current:
   bytes[0..8].try_into().unwrap()

   // Should be:
   bytes[0..8].try_into().expect("8-byte slice always converts to [u8; 8]")
   ```

2. **Lock acquisitions where poisoning should never happen:**
   ```rust
   // Current:
   self.inner.write().unwrap()

   // Should be:
   self.inner.write().expect("lock poisoned - this indicates a panic in another thread")
   ```

3. **Test code with known-good inputs:**
   ```rust
   // Current:
   let result = operation().unwrap();

   // Should be:
   let result = operation().expect("test input is valid");
   ```

## Fix Recommendations

### Priority 1 (Fix Immediately) - 89 calls

**WAL Deserialization (`wal.rs` lines 344-666):**
- Replace all `try_into().unwrap()` calls with proper error handling
- Return `NativeBackendError::CorruptionDetected` for parse failures
- Use `map_err` to convert slice errors to meaningful error messages

**Header Parsing (`header.rs` lines 347-426):**
- Replace all `try_into().unwrap()` calls with proper error handling
- Return `NativeBackendError::InvalidHeader` for parse failures
- Validate buffer length before attempting conversions

**Index Page Unpacking (`index/page.rs` lines 483-625):**
- Replace all `try_into().unwrap()` calls with proper error handling
- Return `NativeBackendError::InvalidHeader` for parse failures
- Validate checksum before attempting to parse content

**Index Persistence (`index_persistence.rs` lines 244-295):**
- Replace all `try_into().unwrap()` calls with proper error handling
- Return `IndexPersistenceError::Corrupted` for parse failures

### Priority 2 (Fix Before v2.0) - 156 calls

**Lock Acquisitions (all files using std::sync):**
- Migrate from `std::sync::{Mutex, RwLock}` to `parking_lot::{Mutex, RwLock}`
- `parking_lot` locks don't poison and have better performance
- If staying with std, use `map_err` to convert poison errors

Example migration:
```rust
// Current (std::sync):
let guard = self.inner.write().unwrap();

// Better (parking_lot):
let guard = self.inner.write();  // Never panics, no unwrap needed
```

**Node Record Validation (`node/record.rs` lines 160-194):**
- Replace `assert!` with proper error returns
- Return `NativeBackendError::InvalidInput` for validation failures

### Priority 3 (Documentation) - 101 calls

**Test Code:**
- Add `.expect("...")` with rationale to all test unwrap calls
- This documents the intent and makes debugging easier

**Debug Assertions:**
- Replace `assert!` with `debug_assert!` where appropriate
- This removes checks in release builds

## Appendix: Full List

### Production Code Unwrap Calls (245 calls)

| File | Line | Snippet |
|------|------|---------|
| wal.rs | 344 | `lsn: u64::from_be_bytes(buf[16..24].try_into().unwrap())` |
| wal.rs | 345 | `page_id: u64::from_be_bytes(buf[24..32].try_into().unwrap())` |
| wal.rs | 346 | `checksum: u64::from_be_bytes(buf[32..40].try_into().unwrap())` |
| wal.rs | 347 | `data_len: u32::from_be_bytes(buf[40..44].try_into().unwrap())` |
| wal.rs | 348 | `prev_lsn: u64::from_be_bytes(buf[44..52].try_into().unwrap())` |
| wal.rs | 349 | `tx_id: u64::from_be_bytes(buf[52..60].try_into().unwrap())` |
| wal.rs | 350 | `record_type: u8::from_be_bytes(buf[60..61].try_into().unwrap())` |
| wal.rs | 351 | `flags: u8::from_be_bytes(buf[61..62].try_into().unwrap())` |
| wal.rs | 352 | `padding: u16::from_be_bytes(buf[62..64].try_into().unwrap())` |
| wal.rs | 459-666 | Multiple `try_into().unwrap()` in record parsing |
| index/page.rs | 483-625 | Multiple `try_into().unwrap()` in page unpacking |
| node/page.rs | 483-598 | Multiple `decode_*.unwrap()` in node decoding |
| header.rs | 347-426 | Multiple `try_into().unwrap()` in header parsing |
| index_persistence.rs | 244-295 | Multiple `try_into().unwrap()` in index parsing |
| backend.rs | 280-595 | 22 lock acquisition unwraps |
| btree.rs | 180-465 | 20 lock acquisition unwraps |
| edge_compat.rs | 280-535 | 18 lock acquisition unwraps |
| name_index.rs | 35-103 | 8 lock acquisition unwraps |
| node/store.rs | 280-325 | 4 lock acquisition unwraps |
| adjacency.rs | 150-165 | 2 lock acquisition unwraps |
| pubsub/publisher.rs | 61-114 | 5 lock acquisition unwraps |
| forensics.rs | 180-195 | 2 lock acquisition unwraps (feature-gated) |

### Test Code Unwrap Calls (101 calls)

All test code unwrap calls are in `#[cfg(test)]` modules in:
- `compression/varint.rs`
- `compression/delta.rs`
- `allocator.rs`
- `write_batch.rs`
- `file_coordinator.rs`
- `header.rs`
- `index/page.rs`
- `node/page.rs`
- `node/record.rs`
- `index_persistence.rs`

---

*Analysis generated: 2026-03-15*
*Total unwrap calls analyzed: 346*
*Runtime code calls requiring attention: 245*
*Test code calls: 101*
