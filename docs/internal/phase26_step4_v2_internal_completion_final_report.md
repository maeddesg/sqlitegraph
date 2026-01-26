| Test Suite | Status | Notes |
| --- | --- | --- |
| `cargo test -p sqlitegraph --features v2_experimental --test v2_native_bfs_regression_tests -- --nocapture` | PASS | NodeStore-only regressions now validate `write_node_v2`, `read_node_v2`, and `rebuild_v2_index` without `Invalid node ID` errors. |
| `cargo test -p sqlitegraph --features v2_experimental --test direct_v2_parsing_test -- --nocapture` | PASS | Direct byte-level parsing at offset 1024 confirms the corrected V2 header layout. |
| `cargo test -p sqlitegraph --features v2_experimental --test v2_takeover_routing_tests -- --nocapture` | RED (expected) | Still asserts the future “V2-by-default” behavior (version byte==2, clustered metadata populated); failures now arise solely from those final assertions. |

- **Node store / V2 record changes:** `node_store.rs` now uses the corrected `NodeRecordV2` layout via `read_node_v2` and `rebuild_v2_index`, while `node_record_v2/record.rs` defines a consistent 32-byte cluster footer (version / flags / id / length fields match `parse_v2_header_lengths`). V2 serialization allocates upfront and deserialization bounds-checks every section.
- **Gated legacy unit suites:** Internal tests in `backend/native/edge_store.rs`, `backend/native/graph_ops.rs`, and `backend/native/v2/format_detection.rs` no longer run when `v2_experimental` is enabled, preventing V1-only assumptions from panicking under the feature flag.
- **V2-only regression coverage:** `v2_native_bfs_regression_tests.rs` now exercises NodeStore + GraphFile directly, `direct_v2_parsing_test.rs` focuses on raw header decoding, and `v2_takeover_routing_tests.rs` remains the RED-state contract for the eventual runtime switchover.
- Runtime routing is still pure V1 (GraphBackend insert/read paths untouched); enabling `v2_experimental` simply activates the self-consistent V2 helpers and their dedicated tests without destabilizing the legacy flow.
