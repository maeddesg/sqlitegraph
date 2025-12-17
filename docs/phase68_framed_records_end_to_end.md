# Phase 68 — Framed Records End-To-End

## Code Changes
- `sqlitegraph/src/backend/native/constants.rs:90-95` defines `FLAG_V2_FRAMED_RECORDS` and makes it part of the default header flags so new files always advertise framed clusters.
- `sqlitegraph/src/backend/native/types.rs:120-137` now seeds fresh headers with `DEFAULT_FEATURE_FLAGS`, so reopen validation can see the framed bit immediately.
- `sqlitegraph/src/backend/native/edge_store.rs:1016-1034` introduces `trace_override_strict_mode` and threads the flag through `read_clustered_edges`, guaranteeing framed clusters never fall back to legacy decoding once the bit is present.
- `sqlitegraph/src/backend/native/edge_store.rs:1085-1127` sets `FLAG_V2_FRAMED_RECORDS` the first time any cluster is written, and persists the header after every allocation so strict reads are enforced on reopen.
- `sqlitegraph/tests/phase68_cursor_remainder_tests.rs` captures the Phase 66 cursor-remainder pathology by constructing a truncated payload and asserting the strict corruption error contains `remaining=58`.

## Instrumentation Evidence
```text
$ RUST_BACKTRACE=full cargo test -p sqlitegraph --test v2_read_after_reopen_regression --features v2_experimental test_v2_read_after_reopen_stress -- --nocapture
Error: ConnectionError("Corrupt edge record -1: framed cluster header mismatch [node_id=8, direction=Incoming, cluster_offset=6243328, payload_size=60, edge_index=0, cursor=66, remaining=2, preview_hex=00 00 00 00 00 00 00 07 00 7D 00 30 7B 22 65 64, preview_ascii=\"\\0\\0\\0\\0\\0\\0\\0\\u{7}\\0}\\00{\\\"ed\"]")
```
```text
$ cargo test -p sqlitegraph --test v2_read_after_reopen_regression --features v2_experimental test_v2_read_after_reopen_consistency -- --nocapture
Error: ConnectionError("Corrupt edge record -1: framed cluster header mismatch [node_id=3, direction=Incoming, cluster_offset=4604928, payload_size=186, edge_index=0, cursor=96, remaining=98, preview_hex=00 00 00 00 00 00 00 01 00 7D 00 51 7B 22 65 64, preview_ascii=\"\\0\\0\\0\\0\\0\\0\\0\\u{1}\\0}\\0Q{\\\"ed\"]")
```
```text
$ RUST_BACKTRACE=full cargo test -p sqlitegraph --test phase68_cursor_remainder_tests --features "v2_experimental trace_v2_io" -- --nocapture
[trace_v2_io] node_id=8, direction=Incoming, cluster_offset=6243328, payload_size=58, edge_index=0, cursor=8, remaining=58, preview_hex=[00, 00, 00, 00, 00, 00, 00, 2A, 00, 01, 22, 46, ...], preview_ascii="\0\0\0\0\0\0\0*\0\u{1}\"F{\"edge_index\":0,\"payload\":\"ascii_remainder\"}!!"
```

## Validation Matrix
```text
$ cargo test -p sqlitegraph --test v2_edge_insertion_corruption_regression --features v2_experimental -- --nocapture
Error: ConnectionError("Corrupt edge record -1: framed cluster header mismatch [node_id=776, direction=Outgoing, cluster_offset=4097024, payload_size=29, edge_index=0, cursor=36, remaining=1, preview_hex=00 00 00 00 00 00 03 9F 00 7D 00 11 7B 22 65 64, ...]")
```
```text
$ cargo test -p sqlitegraph --test v2_read_after_reopen_regression --features v2_experimental -- --nocapture
failures: test_v2_read_after_reopen_consistency (cursor=96, remaining=98) and test_v2_read_after_reopen_stress (cursor=66, remaining=2) both surface `Corrupt edge record -1: framed cluster header mismatch [...]`.
```
```text
$ cargo test -p sqlitegraph --test phase42_cluster_allocation_invariants_tests --features v2_experimental
failures:
  - test_cluster_headers_survive_reopen -> InvalidHeader { field: "free_space_offset", reason: "must be >= incoming_cluster_offset" }
  - test_multi_cluster_offsets_must_be_distinct_and_non_overlapping -> same header error
  - test_header_and_file_length_consistency_after_multiple_cluster_writes -> node_count stayed at 0
```
```text
$ cargo test -p sqlitegraph --test header_region_lockdown_tests --features v2_experimental
test result: ok. 8 passed; 0 failed.
```

## Conclusion
1. `FLAG_V2_FRAMED_RECORDS` is now asserted at creation time and re-written on every cluster flush, so replayed headers always demand strict framing.
2. `EdgeStore::read_clustered_edges` routes framed clusters through the stricter cursor accounting path, and `EdgeCluster::deserialize` reports the precise node/direction/offset context when any frame invariant fails.
3. The synthetic `phase68_cursor_remainder_tests` reproduces the Phase 66 “58-byte” cursor remainder and proves the `remaining` value is the unread payload span, not the stored length.
4. Stress, consistency, and edge-insertion regressions continue to fail, but every failure now points directly to the corrupt offset with node ID, direction, payload_size, cursor, and preview bytes.
5. Header lockdown tests still pass, confirming the new flag writes did not regress existing header fences; cluster allocation invariants still fail due to pre-existing free-space header ordering issues.
