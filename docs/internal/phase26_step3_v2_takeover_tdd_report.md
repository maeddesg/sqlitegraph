| Test | Status | Failure |
| --- | --- | --- |
| `default_insert_uses_v2_version_byte` | FAIL | Version byte read as `1` after `NodeStore::write_node`, proving V1 writer still default. |
| `index_rebuild_uses_v2_index_only` | FAIL | Version byte remains `1` even after `NodeStore::rebuild_v2_index()`. |
| `adjacency_uses_clustered_metadata_by_default` | FAIL | `EdgeStore::write_edge` returned `CorruptNodeRecord { reason: "Expected node record version 1, found 2" }`, so clustered adjacency never initializes. |

Command `cargo test -p sqlitegraph --tests --features v2_experimental --test v2_takeover_routing_tests -- --nocapture` also fails earlier because existing unit tests (`backend::native::edge_store::tests::test_edge_roundtrip`, `backend::native::graph_ops::tests::test_native_bfs_simple`, `backend::native::v2::format_detection::tests::test_format_validation_v1_returns_unsupported`, etc.) panic with the same `Expected node record version 1, found 2` / `Header encoding size mismatch` errors. Those failures pre-date this step and confirm the runtime is still hard-wired to V1 headers even when V2 helpers compile.

These new tests encode the desired Phase 26 target state: the default writer/reader/index rebuild should serialize V2 records and adjacency operations should populate clustered metadata. Until the V2 takeover lands, every path continues to emit V1 layouts, so the tests rightfully fail and document the missing behavior.
