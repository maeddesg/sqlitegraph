# Phase 36: Test Inventory and Classification

## Analysis of Current V2 Test Corruption

Based on comprehensive analysis of existing test files, here's the classification of all V2-related tests:

## LIST A: Tests Requiring Complete Rewrite
*(Must be rewritten to use clean Phase 34 pipeline)*

1. **phase31_v2_default_takeover_tests.rs**
   - **Status**: ❌ All 6 tests failing with "Cluster size mismatch: expected 8, found X"
   - **Issue**: Tests use old corrupt cluster data from Phases 31-34
   - **Action**: Complete rewrite using clean fixture builders

2. **phase32_cluster_pipeline_reconstruction_tests.rs**
   - **Status**: ❌ Multiple cluster corruption failures
   - **Issue**: Tests expect corrupt cluster behavior
   - **Action**: Rewrite to test clean cluster behavior

3. **phase31_2_cluster_deserialization_tests.rs**
   - **Status**: ❌ Contains "Cluster size mismatch" tests expecting corruption
   - **Issue**: Tests designed to expose corruption bug that's now fixed
   - **Action**: Rewrite or delete - corruption bug is resolved

4. **phase33_v2_cluster_architecture_tests.rs**
   - **Status**: ❌ Uses manual cluster creation bypassing Phase 34 pipeline
   - **Issue**: Tests create clusters manually instead of using write_edge()
   - **Action**: Rewrite to use real edge insertion pipeline

## LIST B: Tests Requiring Deletion
*(Tests that no longer apply after Phase 34 fixes)*

1. **phase31_2_cluster_deserialization_tests.rs**
   - **Reason**: Tests specifically for corruption bug that Phase 34 fixed
   - **Action**: Delete entire file - corruption no longer exists

2. **Any test with "cluster size mismatch" in name**
   - **Reason**: Testing for corruption that's been eliminated
   - **Action**: Remove corruption-focused tests

## LIST C: Tests Requiring V1 Fallback Behavior Validation
*(Tests that should verify V1 fallback works correctly)*

1. **edge_store.rs unit tests** (if any exist)
   - **Requirement**: Verify fallback to V1 when cluster metadata missing
   - **Action**: Add specific V1 fallback validation tests

2. **adjacency.rs unit tests** (if any exist)
   - **Requirement**: Verify routing decisions and error classification
   - **Action**: Add router fallback behavior tests

## LIST D: Tests Already Valid Under New V2 Pipeline

1. **phase35_v2_adjacency_router_rewrite_tests.rs**
   - **Status**: ✅ Uses clean graph creation via public API
   - **Behavior**: Tests show correct routing behavior for new graphs
   - **Action**: Keep as-is - validates router improvements

2. **phase34_v2_cluster_pipeline_tests.rs**
   - **Status**: ✅ Uses EdgeCluster API directly (Phase 34 validation)
   - **Behavior**: Tests single edge cluster creation works correctly
   - **Action**: Keep - validates Phase 34 pipeline

3. **phase30_v2_record_boundary_tests.rs**
   - **Status**: ⚠️ Uses manual NodeRecord creation, but tests valid boundary behavior
   - **Action**: Keep - validates V2 record sizing correctly

4. **phase31_3_cluster_neighbor_id_tests.rs**
   - **Status**: ⚠️ May need validation but tests core neighbor ID correctness
   - **Action**: Verify and potentially update

## Corruption Patterns Identified

### Pattern 1: "Cluster size mismatch: expected 8, found X"
- **Root Cause**: Manual cluster serialization inconsistent with EdgeCluster::deserialize()
- **Affected Files**: All phase31 and phase32 tests
- **Solution**: Use Phase 34 write_edge() pipeline instead of manual cluster creation

### Pattern 2: InvalidMagic errors in direct cluster access
- **Root Cause**: Tests trying to read clusters directly without proper V2 metadata
- **Affected Files**: Tests with manual GraphFile manipulation
- **Solution**: Use public APIs and AdjacencyIterator routing

### Pattern 3: Empty neighbor results where neighbors should exist
- **Root Cause**: Incoming cluster metadata not properly set during edge insertion
- **Affected Files**: Phase 31 incoming neighbor tests
- **Solution**: Ensure Phase 34 update_v2_clustered_adjacency handles both directions

## Recommended Rewrite Priority

### Priority 1 (Critical - Block all V2 testing):
1. phase31_v2_default_takeover_tests.rs
2. phase32_cluster_pipeline_reconstruction_tests.rs

### Priority 2 (High - Clean up corruption tests):
3. phase31_2_cluster_deserialization_tests.rs (DELETE)
4. phase33_v2_cluster_architecture_tests.rs

### Priority 3 (Medium - Validation and cleanup):
5. phase31_3_cluster_neighbor_id_tests.rs
6. Any remaining manual cluster creation tests

## Technical Requirements for Rewrites

All rewritten tests MUST:
1. Use `GraphConfig::native()` and `open_graph()` public API
2. Create nodes via `graph.insert_node()` → uses Phase 34 pipeline internally
3. Create edges via `graph.insert_edge()` → triggers Phase 34 cluster creation
4. Validate neighbors via `graph.neighbors()` → uses Phase 35 router
5. Avoid manual `GraphFile`, `NodeStore`, `EdgeStore` manipulation unless specifically testing those components
6. Use helper functions from `helpers/v2_fixture_builders.rs` for common patterns

## Success Criteria

After Phase 36 completion:
- ✅ All V2 tests should create clean data using Phase 34 pipeline
- ✅ No "Cluster size mismatch" errors should appear
- ✅ All neighbor queries should return correct results
- ✅ V2 clustered adjacency should be default path for new graphs
- ✅ V1 fallback should work correctly for edge cases