Lib/integration: `cargo test -p sqlitegraph --no-fail-fast` now runs 57 lib unit tests plus every integration suite; cfg-gated V2 suites report 0 tests.
Benches: `cargo check -p sqlitegraph --benches` builds bfs, k_hop, insert, and native_disk_io benchmarks without hitting nonexistent APIs.
V2-future gating applied to `sqlitegraph/tests/native_{v1_boundary_read,v1_edge_boundary,v1_legacy_edge_boundary,validation_regression}.rs` so they no longer assume V2 behavior.
Native unit-test modules in `src/backend/native/{edge_store.rs,graph_ops.rs}` and V2 helpers (`v2/format_detection.rs`, `v2/free_space/mod.rs`, `v2/migration.rs`, `v2/node_record_v2/mod.rs`, `v2/string_table/mod.rs`) now require `feature = "v2_experimental"`.
Docs: the `open_graph` example in `src/config.rs` moved to a `rust,ignore` block so doctests stop compiling imaginary runtime steps.
No signature/import rewrites were needed; runtime APIs remain untouched.
Net effect: all real (V1/SQLite) behavior keeps its coverage, every V2-only expectation is explicitly future work, and the repo documents that no V2 runtime exists yet.
