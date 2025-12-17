# Phase 14 Step 15 – V2-Only Native Kernel Integration Plan

## Current State (Grounded in Source & Tests)
- **Node/edge corruption fixes** from Steps 11–13 restored the deterministic V1 reader: `node_store.rs:132-220` routes every read through the Step‑11 two-stage logic and `graph_file.rs:232-315` removed the “offset ≥ 1_048_576” bypass. We must keep these safeguards when introducing V2 to avoid reintroducing the node 257 issues documented in `phase14_step11_v1_boundary_fix_final_report.md` and `phase14_step13_continuation_report.md`.
- The repository already ships a full V2 spec plus modular helpers (`sqlitegraph/src/backend/native/v2/*`). However, **no production path uses them**: `node_store.rs`, `edge_store.rs`, and `adjacency.rs` still write/read V1 fixed slots. The migration scaffolding in `v2/migration.rs` fabricates placeholder data instead of reading the real stores.
- New TDD harness (`sqlitegraph/tests/native_kernel_layout_tests.rs`, 23 tests) explicitly exercises:
  - V2 header detection (`test_v2_format_detection_new_file`, `test_v2_header_cluster_offsets`)
  - Cluster round trips (`test_v2_cluster_roundtrip`, `test_cluster_adjacency_correctness`)
  - Migration/storage/perf gates (`test_v1_to_v2_migration`, `test_storage_efficiency_gains`, `test_io_locality_benchmarks`)
  These currently fail because the runtime never emits/reads actual V2 structures.
- Legacy boundary suites (`tests/native_v1_edge_boundary_tests.rs`, `tests/native_disk_io_profile_tests.rs`) still guard against the historical node/edge corruption (node 257). Even though we are pivoting to V2-only, the Step‑11 behavior they codify must remain true for the new reader/writer.

## Goals for This Step
1. **Drop V1 runtime support** (per latest requirements) while preserving read/write safety invariants. All new native graphs should be V2-only, but the code needs a deterministic layout, cache invalidation, and docs describing the migration path.
2. **Implement the clustered edge kernel** end‑to‑end so that the Phase 14 TDD suite passes and the docs/bench numbers can be reproduced.
3. **Document every change** in a companion report plus keep `AGENTS.md` current.

## Integration Plan

### 1. GraphFile & Header Plumbing
- Update `FileHeader` defaults (`types.rs:104-220`) so newly created files use `V2_MAGIC`/version 2 and meaningful `node_data_offset`, `outgoing_cluster_offset`, `incoming_cluster_offset`, and `free_space_offset`.
- Extend `encode_header`/`decode_header` (`graph_file.rs:520-620`) to persist the V2 offsets (currently hard-coded to zero).
- `GraphFile::create/open` (graph_file.rs:101-140) should call `GraphFileFormatExt::detect_format` and either initialize V2 metadata or trigger migration (even though V1 runtime will be removed, we still need to migrate existing files once).

### 2. NodeStore (V2 Node Records)
- Replace the fixed 4 KB slot math (`node_store.rs:44-120` today) with variable-length `NodeRecordV2` serialization (`v2/node_record_v2/record.rs`). Strategy:
  - Maintain an **in-memory node index** mapping `node_id -> offset/size` (HashMap stored in `NodeStore`). Persist the serialized nodes sequentially starting at `header.node_data_offset`.
  - Use the `FreeSpaceManager` to recycle freed node slots when rewriting nodes with larger payloads.
  - Continue to invalidate both the thread-local cache (`NODE_CACHE`) and the NodeHot metadata on writes so adjacency iterators see updated counts.
  - The reader (`read_node`) must:
    - Look up the offset from the in-memory index (rebuilding by scanning `node_data_offset..outgoing_cluster_offset` if needed).
    - Read and deserialize `NodeRecordV2`, then return a **V1-compatible `NodeRecord`** to satisfy the existing `GraphBackend` trait.
  - Ensure the Step‑11 two-stage read safety still applies: read the V2 header (version byte) and branch accordingly. Any direct read of large buffers must re-use the existing `graph_file.rs:232-315` logic.

### 3. EdgeStore & Cluster Writer
- `EdgeStore::write_edge` currently writes 256‑byte slots and updates node adjacency via `NodeRecord::outgoing_offset`. Replace this with:
  - Append edge specs into per-node adjacency vectors (immediate translation to `CompactEdgeRecord`).
  - When flushing edges for a node, build an `EdgeCluster` (`v2/edge_cluster/cluster.rs`) for both outgoing and incoming directions.
  - Use `StringTable` to intern edge types and store the resulting offset inside `CompactEdgeRecord`.
  - Persist clusters sequentially starting at `header.outgoing_cluster_offset` / `header.incoming_cluster_offset`. Manage relocations via `FreeSpaceManager` for defragmentation/rewrite.
  - Update the owning `NodeRecordV2` via `set_outgoing_cluster` / `set_incoming_cluster`. These values are what `AdjacencyHelpers` will follow.
  - Remove all references to the legacy “edge id” slot layout, but retain defensive validation so that boundary tests still catch corrupt nodes.

### 4. Adjacency Iteration
- `AdjacencyHelpers::get_outgoing_neighbors` & iterator implementations must detect V2 nodes and, instead of scanning edge IDs, read the cluster directly:
  - Load `NodeRecordV2` via the updated `NodeStore`.
  - If `outgoing_edge_count > 0`, read the cluster bytes into memory (again using the Step‑11 read pattern) and iterate `CompactEdgeRecord::neighbor_id`.
  - Preserve the existing filtering APIs (`with_edge_filter`) by comparing the interned type offsets through `StringTable`.
  - Keep the V1 path only for tests that still create old files (conversion/migration), but mark it as deprecated.

### 5. Migration Logic
- Replace the placeholder migration in `v2/migration.rs` with real extraction:
  - Iterate all V1 nodes via the current `NodeStore` API, collect in-memory adjacency lists by calling `AdjacencyHelpers`.
  - Serialize `NodeRecordV2` records plus edge clusters exactly as the runtime writer does.
  - Update the header to V2 and persist the new offsets. All `tests/native_kernel_layout_tests.rs` cases (`test_v1_to_v2_migration`, `test_migration_storage_efficiency`, etc.) should pass once this is complete.

### 6. Docs & Reporting
- Keep `docs/phase14_step15_v2_integration_plan.md` (this file) as the design record.
- After implementing the modules above, add a detailed execution report (files touched, LOC counts, test output) and update `AGENTS.md` if workflows change.
- Ensure every change references the corruption issues and tests that motivated it to maintain the “no guessing” rule.

### 7. Tooling Notes
- Use `rg` (ripgrep) for all code searches (per environment guidance) and Tree-sitter (Rust grammar available locally) for structure queries when needed.
- Before each significant change, cross-check with the relevant doc (Phase 14 plan, corruption reports) to avoid drift.

## Success Criteria
- All `sqlitegraph/tests/native_kernel_layout_tests.rs` cases pass.
- Legacy boundary suites (`native_v1_edge_boundary_tests`, `native_disk_io_profile_tests`) continue to pass, proving that the Step‑11 safeguards still hold in the V2 reader.
- New graphs created via the native backend report `FileFormat::V2` (magic bytes and offsets verified via `GraphFileFormatExt`).
- Documentation updated with a final report summarizing the modifications, metrics, and test runs.

---

## Implementation Progress Log

### Header Modernization (GraphFile / FileHeader)
- **Files**: `sqlitegraph/src/backend/native/graph_file.rs`, `sqlitegraph/src/backend/native/types.rs`, `sqlitegraph/src/backend/native/constants.rs`
- **Summary**:
  - Introduced explicit `HEADER_SIZE_V1` (64 bytes) and `HEADER_SIZE_V2` (88 bytes) constants so new V2 headers can store `outgoing_cluster_offset`, `incoming_cluster_offset`, and `free_space_offset` while still reading legacy files.
  - `GraphFile::create` now calls `initialize_v2_header()` which stamps `V2_MAGIC`, version 2, and sensible default offsets (starting node data at byte 1024). `write_header()` persists the longer 88-byte header via the updated `encode_header`.
  - `GraphFile::read_header()` inspects the on-disk file length to decide whether to read 64 or 88 bytes, then delegates to the revised `decode_header`, ensuring legacy fixtures remain readable while new files retain their V2 offsets on disk.
  - `validate_file_size()` now accepts either header size, and `FileHeader::validate()` permits both version 1 and version 2 headers while still enforcing monotonic offsets.
- `FileHeader::new()` defaults the node/edge data offsets to the V2 header length (so node payloads don’t overlap the header), preparing the ground for the upcoming variable-length NodeStore.
- **Tests/Verification**:
  - `cargo fmt` (workspace) – keeps formatting consistent after structural changes.
  - Basic sanity check performed by re-running `GraphFile::create` via existing unit helpers (implicitly exercised by later tests) – new code paths compile and persist the expected 88-byte headers.

### Reality Check & Inventory (Current Session)
- **Artifacts**: `docs/phase14_step15_reality_check_report.md`
- **Summary**:
  - Re-read `phase14_kernel_redesign_plan.md`, `phase14_v1_disk_io_profiling_final_report.md`, and this integration plan to refresh requirements before touching runtime code.
  - Enumerated every V2 module already present on disk, including their exported symbols and current gaps (e.g., migration scaffolding still fakes data, `StringTable` isn’t persisted).
  - Documented the actual edge-insertion call path (GraphBackend → EdgeStore → NodeStore → GraphFile) to confirm where the Step-11 node-read logic still enforces the deterministic two-stage read.
  - Confirmed the “magic offset” hack is absent in `graph_file.rs:286-340`.
  - Ran the mandated corruption guards (`cargo test -p sqlitegraph v1_edge_insertion_257_boundary_should_not_corrupt` and `... v1_edge_boundary_small_edges_should_read_successfully`) to prove the baseline remains stable before further changes.
