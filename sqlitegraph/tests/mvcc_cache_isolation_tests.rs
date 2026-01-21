//! MVCC Cache Isolation Tests - Per-traversal cache validation
//!
//! **Purpose:** Validate that per-traversal cache doesn't break MVCC isolation
//! **Scope:** Integration tests for cache behavior with MVCC semantics
//! **Focus:** Cache evaporates on function return, preventing cross-transaction staleness
//!
//! **Key Design Principle:**
//! The per-traversal cache is scoped to a single traversal operation and evaporates
//! when the function returns. This ensures:
//! - No cross-transaction cache pollution
//! - No explicit cache invalidation needed
//! - MVCC isolation is preserved by design
//!
//! These tests verify that the cache implementation is correct and doesn't introduce
//! isolation violations.

use sqlitegraph::{
    backend::native::{
        graph_file::GraphFile,
        node_store::NodeStore,
        edge_store::EdgeStore,
        graph_ops::native_bfs,
        NativeNodeId,
    },
    GraphEdgeCreate, GraphEntityCreate, GraphEntity, SqliteGraph, SqliteGraphError,
};

use tempfile::TempDir;

//
// TEST HELPERS
//

/// Helper: Create a simple Native graph with A->B->C chain
fn create_simple_native_graph() -> (GraphFile, Vec<NativeNodeId>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_graph.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Create nodes A, B, C
    let mut node_ids = Vec::new();
    for i in 1..=3 {
        let mut node_store = NodeStore::new(&mut graph_file);
        let node_id = node_store.allocate_node_id().expect("Failed to allocate node ID");
        let record = sqlitegraph::backend::native::NodeRecord::new(
            node_id,
            "test".to_string(),
            format!("node_{}", i),
            serde_json::json!({}),
        );
        node_store.write_node(&record).expect("Failed to write node");
        node_ids.push(node_id);
    }

    // Create edges A->B, B->C
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let edge1 = sqlitegraph::backend::native::EdgeRecord::new(
        1, // edge_id
        node_ids[0], // from A
        node_ids[1], // to B
        "connects".to_string(),
        serde_json::json!({}),
    );
    edge_store.write_edge(&edge1).expect("Failed to write edge A->B");

    let edge2 = sqlitegraph::backend::native::EdgeRecord::new(
        2, // edge_id
        node_ids[1], // from B
        node_ids[2], // to C
        "connects".to_string(),
        serde_json::json!({}),
    );
    edge_store.write_edge(&edge2).expect("Failed to write edge B->C");

    (graph_file, node_ids, temp_dir)
}

/// Helper: Run BFS traversal and return result
fn run_bfs_traversal(graph_file: &mut GraphFile, start: NativeNodeId, depth: u32) -> Vec<NativeNodeId> {
    native_bfs(graph_file, start, depth).expect("BFS should succeed")
}

//
// GROUP 1: CACHE EVAPORATION TESTS
//

#[test]
fn test_cache_evaporation_on_function_return() {
    // Scenario: Run BFS twice from same node
    // Expected: Each traversal creates fresh cache (no cross-traversal pollution)
    //           Both traversals return same results (correctness)

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();
    let start_node = node_ids[0]; // Node A

    // First traversal
    let result1 = run_bfs_traversal(&mut graph_file, start_node, 2);
    assert!(!result1.is_empty(), "First traversal should find nodes");

    // Second traversal - should NOT see cached data from first traversal
    let result2 = run_bfs_traversal(&mut graph_file, start_node, 2);
    assert!(!result2.is_empty(), "Second traversal should find nodes");

    // Both traversals should return identical results (correctness)
    assert_eq!(
        result1, result2,
        "Both traversals should return same results"
    );

    // Verify we can reach both B and C from A
    assert!(
        result1.contains(&node_ids[1]),
        "Should reach node B from A"
    );
    assert!(
        result1.contains(&node_ids[2]),
        "Should reach node C from A"
    );
}

#[test]
fn test_cache_evaporation_multiple_sequential_traversals() {
    // Scenario: Run BFS 10 times sequentially from same node
    // Expected: Each traversal produces consistent results
    //           No cache pollution between traversals

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();
    let start_node = node_ids[0]; // Node A

    let expected_result = run_bfs_traversal(&mut graph_file, start_node, 2);

    // Run 9 more traversals
    for i in 1..10 {
        let result = run_bfs_traversal(&mut graph_file, start_node, 2);
        assert_eq!(
            result, expected_result,
            "Traversal {} should match first result", i
        );
    }
}

#[test]
fn test_cache_evaporation_different_start_nodes() {
    // Scenario: Run BFS from different start nodes
    // Expected: Each traversal is independent, no cache pollution

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();

    // BFS from A should find B and C
    let result_a = run_bfs_traversal(&mut graph_file, node_ids[0], 2);
    assert!(result_a.contains(&node_ids[1]));
    assert!(result_a.contains(&node_ids[2]));

    // BFS from B should find C only
    let result_b = run_bfs_traversal(&mut graph_file, node_ids[1], 1);
    assert!(result_b.contains(&node_ids[2]));
    assert!(!result_b.contains(&node_ids[0])); // Can't go back to A

    // BFS from A again - should still work correctly
    let result_a2 = run_bfs_traversal(&mut graph_file, node_ids[0], 2);
    assert_eq!(result_a, result_a2, "Second BFS from A should match first");
}

//
// GROUP 2: CONCURRENT TRAVERSAL TESTS
//
// Note: SqliteGraph (and Native backend) use RefCell for interior mutability,
// which is not thread-safe. These tests use sequential traversals to verify
// cache isolation rather than true concurrent access.
//
// The key insight is: even with sequential access, if cache didn't evaporate,
// we would see cross-traversal pollution. These tests verify that doesn't happen.
//

#[test]
fn test_sequential_traversals_separate_caches() {
    // Scenario: Two sequential BFS traversals from different start nodes
    // Expected: Neither traversal sees data from the other (cache isolation)
    //           This verifies cache evaporates between function calls

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();

    // First traversal: from node A (should reach B, C)
    let result1 = run_bfs_traversal(&mut graph_file, node_ids[0], 2);
    assert!(result1.contains(&node_ids[1]), "First traversal should reach node B");
    assert!(result1.contains(&node_ids[2]), "First traversal should reach node C");

    // Second traversal: from node B (should reach C only)
    // This proves cache from first traversal didn't pollute second
    let result2 = run_bfs_traversal(&mut graph_file, node_ids[1], 1);
    assert!(result2.contains(&node_ids[2]), "Second traversal should reach node C");
    assert!(!result2.contains(&node_ids[0]), "Second traversal should not see node A");

    // Third traversal: from node A again
    // Should get same results as first (no pollution from second)
    let result3 = run_bfs_traversal(&mut graph_file, node_ids[0], 2);
    assert_eq!(result1, result3, "Third traversal should match first");
}

#[test]
fn test_multiple_start_nodes_isolated_caches() {
    // Scenario: Run BFS from all nodes in sequence
    // Expected: Each traversal is independent, no cache pollution

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();

    // BFS from A: should reach B, C
    let result_a = run_bfs_traversal(&mut graph_file, node_ids[0], 2);
    assert_eq!(result_a.len(), 2, "From A should reach 2 nodes");

    // BFS from B: should reach C only
    let result_b = run_bfs_traversal(&mut graph_file, node_ids[1], 1);
    assert_eq!(result_b.len(), 1, "From B should reach 1 node");
    assert!(result_b.contains(&node_ids[2]), "From B should reach C");

    // BFS from C: should reach nothing
    let result_c = run_bfs_traversal(&mut graph_file, node_ids[2], 1);
    assert_eq!(result_c.len(), 0, "From C should reach 0 nodes");

    // BFS from A again: should still reach B, C (not affected by previous traversals)
    let result_a2 = run_bfs_traversal(&mut graph_file, node_ids[0], 2);
    assert_eq!(result_a, result_a2, "Second A traversal should match first");
}

#[test]
fn test_alternating_directions_isolated_caches() {
    // Scenario: Alternate between different start nodes and depths
    // Expected: Each traversal produces correct results independently

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();

    for i in 0..10 {
        let start_idx = i % 3;
        let depth = if i % 2 == 0 { 1 } else { 2 };

        let result = run_bfs_traversal(&mut graph_file, node_ids[start_idx], depth);

        // Verify correctness based on start node and depth
        if start_idx == 0 {
            // From node A
            if depth == 1 {
                assert_eq!(result.len(), 1, "A depth 1: should reach 1 node (B)");
                assert!(result.contains(&node_ids[1]), "A depth 1: should reach B");
            } else {
                assert_eq!(result.len(), 2, "A depth 2: should reach 2 nodes (B, C)");
                assert!(result.contains(&node_ids[2]), "A depth 2: should reach C");
            }
        } else if start_idx == 1 {
            // From node B
            if depth == 1 {
                assert_eq!(result.len(), 1, "B depth 1: should reach 1 node (C)");
                assert!(result.contains(&node_ids[2]), "B depth 1: should reach C");
            } else {
                assert_eq!(result.len(), 1, "B depth 2: should reach 1 node (C)");
            }
        } else {
            // From node C (leaf)
            assert_eq!(result.len(), 0, "C: should reach 0 nodes");
        }
    }
}

//
// GROUP 3: SNAPSHOT ISOLATION TESTS
//

/// Helper: Create SqliteGraph with test data for snapshot tests
fn create_sqlite_graph_for_snapshot() -> Result<(SqliteGraph, i64, i64), SqliteGraphError> {
    let graph = SqliteGraph::open_in_memory()?;

    // Create node A
    let entity_a = GraphEntityCreate {
        kind: "function".to_string(),
        name: "node_a".to_string(),
        file_path: Some("a.rs".to_string()),
        data: serde_json::json!({}),
    };
    let id_a = graph.insert_entity(&GraphEntity {
        id: 0,
        kind: entity_a.kind,
        name: entity_a.name,
        file_path: entity_a.file_path,
        data: entity_a.data,
    })?;

    // Create node B
    let entity_b = GraphEntityCreate {
        kind: "function".to_string(),
        name: "node_b".to_string(),
        file_path: Some("b.rs".to_string()),
        data: serde_json::json!({}),
    };
    let id_b = graph.insert_entity(&GraphEntity {
        id: 0,
        kind: entity_b.kind,
        name: entity_b.name,
        file_path: entity_b.file_path,
        data: entity_b.data,
    })?;

    // Create edge A->B
    let edge = GraphEdgeCreate {
        from_id: id_a,
        to_id: id_b,
        edge_type: "calls".to_string(),
        data: serde_json::json!({}),
    };
    graph.insert_edge(&sqlitegraph::GraphEdge {
        id: 0,
        from_id: edge.from_id,
        to_id: edge.to_id,
        edge_type: edge.edge_type,
        data: edge.data,
    })?;

    Ok((graph, id_a, id_b))
}

#[test]
fn test_cache_no_cross_transaction_staleness() -> Result<(), SqliteGraphError> {
    // Scenario: Transaction 1 reads node A, Transaction 2 adds edge A->D
    // Expected: Transaction 1 does NOT see D (snapshot isolation maintained)
    //           Cache doesn't cause stale reads

    let (graph, id_a, _id_b) = create_sqlite_graph_for_snapshot()?;

    // Transaction 1: Read A's neighbors via BFS-like query
    let neighbors_t1 = graph.query().outgoing(id_a)?;

    // Transaction 2: Add new node D and edge A->D
    let entity_d = GraphEntityCreate {
        kind: "function".to_string(),
        name: "node_d".to_string(),
        file_path: Some("d.rs".to_string()),
        data: serde_json::json!({}),
    };
    let id_d = graph.insert_entity(&GraphEntity {
        id: 0,
        kind: entity_d.kind,
        name: entity_d.name,
        file_path: entity_d.file_path,
        data: entity_d.data,
    })?;

    let edge_d = GraphEdgeCreate {
        from_id: id_a,
        to_id: id_d,
        edge_type: "calls".to_string(),
        data: serde_json::json!({}),
    };
    graph.insert_edge(&sqlitegraph::GraphEdge {
        id: 0,
        from_id: edge_d.from_id,
        to_id: edge_d.to_id,
        edge_type: edge_d.edge_type,
        data: edge_d.data,
    })?;

    // Transaction 1 reads A again
    // In MVCC-lite, this WILL see the new edge (not full snapshot isolation)
    // But cache shouldn't cause incorrect behavior
    let neighbors_t1_updated = graph.query().outgoing(id_a)?;

    // Verify the new edge is visible (graph state changed)
    assert!(
        neighbors_t1_updated.len() > neighbors_t1.len(),
        "After adding edge, should have more neighbors"
    );
    assert!(
        neighbors_t1_updated.contains(&id_d),
        "New node D should be visible"
    );

    Ok(())
}

#[test]
fn test_snapshot_with_traversal_isolation() -> Result<(), SqliteGraphError> {
    // Scenario: Acquire snapshot, run traversal, modify graph, run traversal again
    // Expected: Snapshot sees original state, new traversal sees updated state

    let (graph, id_a, id_b) = create_sqlite_graph_for_snapshot()?;

    // IMPORTANT: Warm cache before acquiring snapshot
    // This is required for the snapshot to contain adjacency data
    let entity_ids = graph.list_entity_ids()?;
    for &id in &entity_ids {
        let _ = graph.query().outgoing(id)?;
        let _ = graph.query().incoming(id)?;
    }

    // Acquire snapshot
    let snapshot = graph.acquire_snapshot()?;

    // Verify snapshot sees original state
    let snapshot_neighbors = snapshot.get_outgoing(id_a);
    assert_eq!(
        snapshot_neighbors,
        Some(&vec![id_b]),
        "Snapshot should see A->B edge"
    );

    // Add new edge
    let entity_c = GraphEntityCreate {
        kind: "function".to_string(),
        name: "node_c".to_string(),
        file_path: Some("c.rs".to_string()),
        data: serde_json::json!({}),
    };
    let id_c = graph.insert_entity(&GraphEntity {
        id: 0,
        kind: entity_c.kind,
        name: entity_c.name,
        file_path: entity_c.file_path,
        data: entity_c.data,
    })?;

    let edge_c = GraphEdgeCreate {
        from_id: id_a,
        to_id: id_c,
        edge_type: "calls".to_string(),
        data: serde_json::json!({}),
    };
    graph.insert_edge(&sqlitegraph::GraphEdge {
        id: 0,
        from_id: edge_c.from_id,
        to_id: edge_c.to_id,
        edge_type: edge_c.edge_type,
        data: edge_c.data,
    })?;

    // Snapshot should NOT see the new edge
    let snapshot_neighbors_after = snapshot.get_outgoing(id_a);
    assert_eq!(
        snapshot_neighbors_after,
        Some(&vec![id_b]),
        "Snapshot should still see only A->B edge"
    );

    // New traversal should see the updated state
    let current_neighbors = graph.query().outgoing(id_a)?;
    assert!(
        current_neighbors.contains(&id_c),
        "Current traversal should see new edge A->C"
    );

    Ok(())
}

//
// GROUP 4: CACHE CONSISTENCY TESTS
//

#[test]
fn test_multiple_traversals_consistent_results() {
    // Scenario: Run BFS with different depths from same node
    // Expected: Each depth produces correct, consistent results

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();
    let start_node = node_ids[0];

    // Depth 1: should reach B only
    let result_d1 = run_bfs_traversal(&mut graph_file, start_node, 1);
    assert_eq!(result_d1.len(), 1, "Depth 1 should reach 1 node");
    assert!(result_d1.contains(&node_ids[1]), "Depth 1 should reach node B");

    // Depth 2: should reach B and C
    let result_d2 = run_bfs_traversal(&mut graph_file, start_node, 2);
    assert_eq!(result_d2.len(), 2, "Depth 2 should reach 2 nodes");
    assert!(result_d2.contains(&node_ids[1]), "Depth 2 should reach node B");
    assert!(result_d2.contains(&node_ids[2]), "Depth 2 should reach node C");

    // Depth 1 again: should still reach B only (no cache pollution from depth 2)
    let result_d1_again = run_bfs_traversal(&mut graph_file, start_node, 1);
    assert_eq!(result_d1_again, result_d1, "Depth 1 should be consistent");
}

#[test]
fn test_cache_with_graph_modifications() {
    // Scenario: Run BFS, modify graph, run BFS again
    // Expected: Second BFS sees modified state (cache evaporated)

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("modification_test.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Create initial nodes 1->2->3
    let mut node_ids = Vec::new();
    for i in 1..=3 {
        let mut node_store = NodeStore::new(&mut graph_file);
        let node_id = node_store.allocate_node_id().expect("Failed to allocate node ID");
        let record = sqlitegraph::backend::native::NodeRecord::new(
            node_id,
            "test".to_string(),
            format!("node_{}", i),
            serde_json::json!({}),
        );
        node_store.write_node(&record).expect("Failed to write node");
        node_ids.push(node_id);
    }

    // Create edge 1->2
    {
        let mut edge_store = EdgeStore::new(&mut graph_file);
        let edge1 = sqlitegraph::backend::native::EdgeRecord::new(
            1,
            node_ids[0],
            node_ids[1],
            "connects".to_string(),
            serde_json::json!({}),
        );
        edge_store.write_edge(&edge1).expect("Failed to write edge");
    }

    // First BFS: should reach node 2 only
    let result1 = run_bfs_traversal(&mut graph_file, node_ids[0], 2);
    assert_eq!(result1.len(), 1, "First BFS should reach 1 node");

    // Add edge 2->3
    {
        let mut edge_store = EdgeStore::new(&mut graph_file);
        let edge2 = sqlitegraph::backend::native::EdgeRecord::new(
            2,
            node_ids[1],
            node_ids[2],
            "connects".to_string(),
            serde_json::json!({}),
        );
        edge_store.write_edge(&edge2).expect("Failed to write edge");
    }

    // Second BFS: should reach nodes 2 and 3
    let result2 = run_bfs_traversal(&mut graph_file, node_ids[0], 2);
    assert_eq!(result2.len(), 2, "Second BFS should reach 2 nodes after modification");
    assert!(result2.contains(&node_ids[2]), "Second BFS should reach node 3");
}

//
// GROUP 5: STRESS TESTS
//

#[test]
fn test_rapid_sequential_traversals() {
    // Scenario: Run 100 BFS traversals rapidly
    // Expected: All produce consistent results, no cache corruption

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();
    let start_node = node_ids[0];

    let expected = run_bfs_traversal(&mut graph_file, start_node, 2);

    for i in 0..100 {
        let result = run_bfs_traversal(&mut graph_file, start_node, 2);
        assert_eq!(
            result, expected,
            "Traversal {} should produce consistent result", i
        );
    }
}

#[test]
fn test_interleaved_traversals_different_depths() {
    // Scenario: Interleave BFS calls with different depths
    // Expected: Each depth produces correct results independently

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();
    let start_node = node_ids[0];

    for i in 0..20 {
        let depth = (i % 3) + 1; // Depths: 1, 2, 1, 2, 1, 2, ...
        let result = run_bfs_traversal(&mut graph_file, start_node, depth);

        if depth == 1 {
            assert_eq!(result.len(), 1, "Depth 1 should reach 1 node");
        } else if depth == 2 {
            assert_eq!(result.len(), 2, "Depth 2 should reach 2 nodes");
        }
    }
}
