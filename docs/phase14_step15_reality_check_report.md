# Phase 14 Step 15 – Reality Check & Inventory

## 1. Mandatory Pre-Read Summary
- `docs/phase14_kernel_redesign_plan.md`: confirms the clustered V2 layout (NodeRecordV2 + EdgeCluster + StringTable) plus migration rules and ≥2× perf goals.
- `docs/phase14_v1_disk_io_profiling_final_report.md`: reiterates the node-257 corruption fix, the Step-11 two-stage read requirement, and why the edge clustering work exists (70–85% V1 I/O gap).
- `docs/phase14_step15_v2_integration_plan.md`: lists the execution checklist; header modernization is already logged there.

## 2. V2 Source Inventory (`sqlitegraph/src/backend/native/v2/…`)
| Module | LOC (approx) | Public API (per `mod.rs`) | Notes |
| --- | --- | --- | --- |
| `edge_cluster/{cluster.rs,compact_record.rs,record_ext.rs}` | 190 | `EdgeCluster`, `CompactEdgeRecord`, `Direction`, `EdgeRecordCompactExt` | Serialization/deserialization implemented, `create_from_edges` builds clusters using `StringTable`. |
| `node_record_v2/{record.rs,conversion.rs,metrics.rs}` | 230 | `NodeRecordV2`, `NodeRecordV2Ext`, `NodeRecordV2Operations`, `AdjacencyMetrics` | Variable-length nodes serialize adjacency offsets/counts; conversion traits still return zero offsets for clusters. |
| `string_table/{table.rs,serialization.rs,metrics.rs}` | 210 | `StringTable`, `CompressionMetrics` | Shared edge-type dictionary; offsets tracked but no on-disk integration yet. |
| `free_space/{manager.rs,...}` | 260 | `FreeSpaceManager`, `FreeBlock`, `AllocationStrategy`, stats structs | In-memory allocator only; not wired into stores. |
| `format_detection.rs` | 320 | `FileFormat`, `GraphFileFormatExt`, `FileFormatDetector`, `MigrationBenefits`, `FormatValidation` | Detects/validates headers; migration helpers estimate benefits. |
| `migration.rs` | 320 | `V1ToV2Migrator`, `GraphFileMigrationExt` | Currently fabricates placeholder nodes/edges and never updates actual node offsets. |

Missing modules mentioned in design docs: **none** (all referenced pieces exist, but runtime wiring is unfinished).

## 3. TDD Surface (Reality Check)
- `sqlitegraph/tests/native_kernel_layout_tests.rs` contains every test cited in the plan: `test_v2_cluster_roundtrip` (line 292), `test_v1_to_v2_migration`/`test_v1_to_v2_migration_correctness` (lines 514/741), `test_cluster_adjacency_correctness` (line 571), `test_storage_efficiency_gains` (line 602), `test_io_locality_benchmarks` (line 622). They are marked `#[test]` but currently gated by `#[cfg(test)]` fixture helpers; they fail to run meaningfully because the runtime is still V1.
- Legacy corruption guards: `tests/native_v1_edge_boundary_tests.rs` + `tests/native_disk_io_profile_tests.rs` remain active so we must preserve Step-11 semantics while moving to V2.

## 4. Actual Edge-Insertion Call Path (V1 Runtime Today)
1. `NativeGraphBackend::insert_edge` (`sqlitegraph/src/backend/native/graph_backend.rs:58-80`) routes through `EdgeStore`.
2. `EdgeStore::allocate_edge_id` (`edge_store.rs:520-545`) increments header counters.
3. `EdgeStore::write_edge` (`edge_store.rs:24-120`) serializes 256-byte slots, writes via `GraphFile::write_bytes`, and immediately calls `update_node_adjacency`.
4. `EdgeStore::update_node_adjacency` (`edge_store.rs:132-159`) constructs a fresh `NodeStore`, then:
   - `NodeStore::read_node` (`node_store.rs:96-174`) → `read_node_internal` (`node_store.rs:177-248`) which performs the Step-11 two-stage read using `GraphFile::read_bytes` → `GraphFile::read_with_ahead` (`graph_file.rs:286-340`).
   - After incrementing counts, `NodeStore::write_node` rewrites the entire 4 KB slot.
5. No code path currently touches `NodeRecordV2` or `EdgeCluster`; even the unused `read_node_internal_v2` helper (node_store.rs:251-360) just converts V2-on-disk nodes back to V1 structs with zeroed adjacency.

## 5. GraphFile Read Path Status
- Lines `graph_file.rs:286-340` confirm the “offset ≥ 1_048_576 direct-read hack” no longer exists; the Step-11 two-stage read is intact (read-ahead + fallback direct read).
- The node reader in `node_store.rs:177-248` still depends on fixed-slot offsets; all node reads go through that logic, satisfying the Step-11 safety requirement for now.

## 6. Tests Executed (per TDD instruction)
| Command | Result | Notes |
| --- | --- | --- |
| `cargo test -p sqlitegraph v1_edge_insertion_257_boundary_should_not_corrupt` | ✅ pass | Ensures the node-257 corruption fix remains intact. |
| `cargo test -p sqlitegraph v1_edge_boundary_small_edges_should_read_successfully` | ✅ pass | Confirms small-edge read path stability. |

## 7. Immediate Observations
- V2 scaffolding exists but is completely disconnected from runtime paths (NodeStore/EdgeStore/Adjacency).
- Migration scaffolding is placeholder-only; it fabricates nodes/edges instead of reading from disk.
- String table, edge clusters, and free-space manager have self-tests but no integration points.
- The Step-11 reader must be reused verbatim when NodeStore switches to variable-length NodeRecordV2 serialization to avoid regressing the boundary tests listed above.

**Next up (once code changes are allowed):** lift the NodeStore/EdgeStore implementations to V2-only, ensure the adjacency helpers follow clusters instead of edge IDs, and replace the migration placeholders with real extraction logic. All of this work should keep referencing this inventory to prevent drift.
