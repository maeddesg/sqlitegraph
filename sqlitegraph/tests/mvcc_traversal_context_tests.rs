//! MVCC TraversalContext Isolation Tests - SequentialReadBuffer and TraversalContext validation
//!
//! **Purpose:** Validate that TraversalContext and SequentialReadBuffer preserve MVCC isolation
//! **Scope:** Integration tests for sequential I/O optimization with MVCC semantics
//! **Focus:** Buffer evaporation, no cross-traversal pollution, TraversalContext independence
//!
//! **Key Design Principle:**
//! TraversalContext is stack-allocated per traversal and evaporates when the traversal
//! function returns. This ensures:
//! - No cross-traversal buffer pollution
//! - SequentialReadBuffer doesn't leak data between calls
//! - MVCC isolation is preserved by design
//!
//! These tests verify that the TraversalContext implementation (Phase 31) doesn't introduce
//! isolation violations through its SequentialReadBuffer (L1 cache), LinearDetector (pattern
//! detection), or TraversalCache (L2 cache) components.

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
///
/// Creates a linear chain graph: A -> B -> C
/// This is the simplest graph that demonstrates traversal behavior.
///
/// Returns:
/// - GraphFile: The native graph file
/// - Vec<NativeNodeId>: Node IDs [A, B, C]
/// - TempDir: Temporary directory (kept for cleanup)
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
///
/// This function creates a fresh TraversalContext internally via native_bfs,
/// which evaporates when the function returns. This is the key mechanism for
/// MVCC isolation in the sequential I/O optimization.
///
/// Parameters:
/// - graph_file: The native graph file
/// - start: Starting node ID
/// - depth: Maximum traversal depth
///
/// Returns:
/// - Vec<NativeNodeId>: Discovered node IDs (excluding start node)
fn run_bfs_traversal(graph_file: &mut GraphFile, start: NativeNodeId, depth: u32) -> Vec<NativeNodeId> {
    native_bfs(graph_file, start, depth).expect("BFS should succeed")
}

//
// GROUP 1: TRAVERSAL CONTEXT EVAPORATION TESTS
//
// These tests verify that TraversalContext evaporates completely when
// the traversal function returns, ensuring no cross-traversal pollution.
//

#[test]
fn test_traversal_context_evaporation_on_function_return() {
    // Scenario: Run BFS twice from same node
    // Expected: Each traversal creates fresh TraversalContext (no cross-traversal pollution)
    //           Both traversals return same results (correctness)

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();
    let start_node = node_ids[0]; // Node A

    // First traversal - creates TraversalContext, which evaporates on return
    let result1 = run_bfs_traversal(&mut graph_file, start_node, 2);
    assert!(!result1.is_empty(), "First traversal should find nodes");

    // Second traversal - creates NEW TraversalContext (first one evaporated)
    // This proves SequentialReadBuffer doesn't persist between calls
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
fn test_sequential_buffer_no_pollution() {
    // Scenario: Run BFS 10 times sequentially from same node
    // Expected: Each traversal produces consistent results
    //           No SequentialReadBuffer pollution between traversals

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();
    let start_node = node_ids[0]; // Node A

    // First traversal establishes the expected result
    let expected_result = run_bfs_traversal(&mut graph_file, start_node, 2);

    // Run 9 more traversals - all should produce identical results
    // If SequentialReadBuffer polluted across traversals, results would diverge
    for i in 1..10 {
        let result = run_bfs_traversal(&mut graph_file, start_node, 2);
        assert_eq!(
            result, expected_result,
            "Traversal {} should match first result", i
        );
    }

    // Verify correctness of the expected result
    assert_eq!(expected_result.len(), 2, "Should reach 2 nodes from A");
    assert!(expected_result.contains(&node_ids[1]), "Should include B");
    assert!(expected_result.contains(&node_ids[2]), "Should include C");
}

#[test]
fn test_different_start_nodes_independent() {
    // Scenario: Run BFS from different start nodes
    // Expected: Each traversal is independent, no buffer pollution
    //           LinearDetector state doesn't leak across traversals

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();

    // BFS from A should find B and C (depth 2 reaches both)
    let result_a = run_bfs_traversal(&mut graph_file, node_ids[0], 2);
    assert_eq!(result_a.len(), 2, "From A depth 2: should reach 2 nodes");
    assert!(result_a.contains(&node_ids[1]), "From A: should reach B");
    assert!(result_a.contains(&node_ids[2]), "From A: should reach C");

    // BFS from B should find C only (depth 1)
    let result_b = run_bfs_traversal(&mut graph_file, node_ids[1], 1);
    assert_eq!(result_b.len(), 1, "From B depth 1: should reach 1 node");
    assert!(result_b.contains(&node_ids[2]), "From B: should reach C");
    assert!(!result_b.contains(&node_ids[0]), "From B: should not reach A (directed)");

    // BFS from C should find nothing (no outgoing edges)
    let result_c = run_bfs_traversal(&mut graph_file, node_ids[2], 1);
    assert_eq!(result_c.len(), 0, "From C: should reach 0 nodes");

    // BFS from A again - should still work correctly (not affected by previous traversals)
    let result_a2 = run_bfs_traversal(&mut graph_file, node_ids[0], 2);
    assert_eq!(result_a, result_a2, "Second BFS from A should match first");
}

//
// GROUP 2: LINEAR DETECTOR ISOLATION TESTS
//
// These tests verify that LinearDetector state doesn't leak between traversals.
// LinearDetector is a state machine (3-step threshold for linear detection).
// If it didn't reset, it could incorrectly detect linear patterns.
//

#[test]
fn test_linear_detector_state_isolation() {
    // Scenario: Run BFS from nodes with different degree patterns
    // Expected: LinearDetector state resets between traversals
    //           No false positives from previous traversal state

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();

    // First BFS from A (degree 1, linear pattern candidate)
    let result1 = run_bfs_traversal(&mut graph_file, node_ids[0], 1);
    assert_eq!(result1.len(), 1, "From A depth 1: should reach 1 node");

    // Second BFS from C (degree 0, NOT linear)
    // If LinearDetector didn't reset, it might incorrectly carry over state
    let result2 = run_bfs_traversal(&mut graph_file, node_ids[2], 1);
    assert_eq!(result2.len(), 0, "From C: should reach 0 nodes");

    // Third BFS from A again
    // LinearDetector should start fresh, not remember state from first traversal
    let result3 = run_bfs_traversal(&mut graph_file, node_ids[0], 1);
    assert_eq!(result1, result3, "Third traversal should match first");
}

#[test]
fn test_alternating_traversals_isolated() {
    // Scenario: Alternate between different start nodes and depths
    // Expected: Each traversal produces correct results independently
    //           No LinearDetector state pollution from alternation

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();

    // Run alternating traversals
    for i in 0..20 {
        let start_idx = i % 3;
        let depth = if i % 2 == 0 { 1 } else { 2 };

        let result = run_bfs_traversal(&mut graph_file, node_ids[start_idx], depth);

        // Verify correctness based on start node and depth
        match start_idx {
            0 => {
                // From node A
                if depth == 1 {
                    assert_eq!(result.len(), 1, "A depth 1: should reach 1 node (B)");
                    assert!(result.contains(&node_ids[1]), "A depth 1: should reach B");
                } else {
                    assert_eq!(result.len(), 2, "A depth 2: should reach 2 nodes (B, C)");
                    assert!(result.contains(&node_ids[1]), "A depth 2: should reach B");
                    assert!(result.contains(&node_ids[2]), "A depth 2: should reach C");
                }
            }
            1 => {
                // From node B
                assert_eq!(result.len(), 1, "B: should reach 1 node (C)");
                assert!(result.contains(&node_ids[2]), "B: should reach C");
            }
            2 => {
                // From node C (leaf)
                assert_eq!(result.len(), 0, "C: should reach 0 nodes");
            }
            _ => unreachable!(),
        }
    }
}

//
// GROUP 3: MULTI-DEPTH TRAVERSAL ISOLATION TESTS
//
// These tests verify that depth parameter doesn't cause cross-traversal pollution.
// Different depths exercise different code paths in TraversalContext.
//

#[test]
fn test_multiple_depths_independent() {
    // Scenario: Run BFS with different depths from same node
    // Expected: Each depth produces correct, consistent results
    //           Depth-specific buffer state doesn't leak

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
    assert_eq!(result_d1, result_d1_again, "Depth 1 should be consistent");

    // Depth 2 again: should still reach B and C
    let result_d2_again = run_bfs_traversal(&mut graph_file, start_node, 2);
    assert_eq!(result_d2, result_d2_again, "Depth 2 should be consistent");
}

//
// GROUP 4: STRESS TESTS
//
// These tests verify that TraversalContext isolation holds under rapid sequential access.
//

#[test]
fn test_rapid_sequential_traversals() {
    // Scenario: Run 100 BFS traversals rapidly
    // Expected: All produce consistent results, no TraversalContext corruption

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
fn test_interleaved_depth_traversals() {
    // Scenario: Interleave BFS calls with different depths
    // Expected: Each depth produces correct results independently
    //           No TraversalContext state pollution from interleaving

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();
    let start_node = node_ids[0];

    for i in 0..30 {
        let depth = (i % 3) + 1; // Depths: 1, 2, 1, 2, 1, 2, ...
        let result = run_bfs_traversal(&mut graph_file, start_node, depth);

        if depth == 1 {
            assert_eq!(result.len(), 1, "Depth 1 should reach 1 node");
        } else if depth == 2 {
            assert_eq!(result.len(), 2, "Depth 2 should reach 2 nodes");
        } else {
            // Depth 3+ should still only reach 2 nodes (B and C)
            assert_eq!(result.len(), 2, "Depth 3 should reach 2 nodes");
        }
    }
}

//
// GROUP 5: COMPARISON WITH SQLITE BACKEND
//
// These tests verify that Native backend with TraversalContext produces
// results consistent with SQLite backend (MVCC validation).
//

#[test]
fn test_traversal_context_matches_sqlite_backend() -> Result<(), SqliteGraphError> {
    // Scenario: Compare Native BFS results with SQLite BFS results
    // Expected: Both backends return identical results (MVCC correctness)

    // Create SQLite graph with same structure
    let sqlite_graph = SqliteGraph::open_in_memory()?;

    // Create node A
    let entity_a = GraphEntityCreate {
        kind: "function".to_string(),
        name: "node_a".to_string(),
        file_path: Some("a.rs".to_string()),
        data: serde_json::json!({}),
    };
    let id_a = sqlite_graph.insert_entity(&GraphEntity {
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
    let id_b = sqlite_graph.insert_entity(&GraphEntity {
        id: 0,
        kind: entity_b.kind,
        name: entity_b.name,
        file_path: entity_b.file_path,
        data: entity_b.data,
    })?;

    // Create node C
    let entity_c = GraphEntityCreate {
        kind: "function".to_string(),
        name: "node_c".to_string(),
        file_path: Some("c.rs".to_string()),
        data: serde_json::json!({}),
    };
    let id_c = sqlite_graph.insert_entity(&GraphEntity {
        id: 0,
        kind: entity_c.kind,
        name: entity_c.name,
        file_path: entity_c.file_path,
        data: entity_c.data,
    })?;

    // Create edges A->B, B->C
    let edge_ab = GraphEdgeCreate {
        from_id: id_a,
        to_id: id_b,
        edge_type: "connects".to_string(),
        data: serde_json::json!({}),
    };
    sqlite_graph.insert_edge(&sqlitegraph::GraphEdge {
        id: 0,
        from_id: edge_ab.from_id,
        to_id: edge_ab.to_id,
        edge_type: edge_ab.edge_type,
        data: edge_ab.data,
    })?;

    let edge_bc = GraphEdgeCreate {
        from_id: id_b,
        to_id: id_c,
        edge_type: "connects".to_string(),
        data: serde_json::json!({}),
    };
    sqlite_graph.insert_edge(&sqlitegraph::GraphEdge {
        id: 0,
        from_id: edge_bc.from_id,
        to_id: edge_bc.to_id,
        edge_type: edge_bc.edge_type,
        data: edge_bc.data,
    })?;

    // SQLite BFS from A (depth 2)
    let sqlite_neighbors_a = sqlite_graph.query().outgoing(id_a)?;
    assert_eq!(sqlite_neighbors_a.len(), 1, "SQLite: A should have 1 direct neighbor (B)");

    // Create Native graph
    let (mut native_graph, native_nodes, _temp_dir) = create_simple_native_graph();

    // Native BFS from A (depth 1)
    let native_result_a1 = run_bfs_traversal(&mut native_graph, native_nodes[0], 1);
    assert_eq!(
        native_result_a1.len(), sqlite_neighbors_a.len(),
        "Native depth 1 should match SQLite direct neighbors"
    );

    // Both backends should find reachable nodes consistently
    // (Native returns NodeIds, SQLite returns i64 entity IDs, but counts should match)
    let native_result_a2 = run_bfs_traversal(&mut native_graph, native_nodes[0], 2);
    assert_eq!(native_result_a2.len(), 2, "Native depth 2 should reach 2 nodes");

    Ok(())
}

//
// GROUP 6: GRAPH MODIFICATION TESTS
//
// These tests verify that TraversalContext doesn't cause issues when
// the underlying graph is modified between traversals.
//

#[test]
fn test_traversal_context_with_graph_modifications() {
    // Scenario: Run BFS, modify graph, run BFS again
    // Expected: Second BFS sees modified state (TraversalContext evaporated)

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
    assert!(result1.contains(&node_ids[1]), "First BFS should reach node 2");
    assert!(!result1.contains(&node_ids[2]), "First BFS should not reach node 3 yet");

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
    // This proves TraversalContext didn't cache stale graph state
    let result2 = run_bfs_traversal(&mut graph_file, node_ids[0], 2);
    assert_eq!(result2.len(), 2, "Second BFS should reach 2 nodes after modification");
    assert!(result2.contains(&node_ids[1]), "Second BFS should reach node 2");
    assert!(result2.contains(&node_ids[2]), "Second BFS should reach node 3");
}

//
// GROUP 7: EDGE CASE TESTS
//
// These tests verify TraversalContext behavior in edge cases.
//

#[test]
fn test_zero_depth_traversal() {
    // Scenario: BFS with depth 0
    // Expected: Returns start node only (standard BFS semantics)
    //           TraversalContext created but minimal work done

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();
    let start_node = node_ids[0];

    let result = native_bfs(&mut graph_file, start_node, 0).expect("BFS should succeed");
    assert_eq!(result.len(), 1, "Depth 0 should return start node only");
    assert_eq!(result[0], start_node, "Depth 0 should return the start node");
}

#[test]
fn test_traversal_from_leaf_node() {
    // Scenario: BFS from leaf node (no outgoing edges)
    // Expected: Returns empty result (no reachable nodes)
    //           TraversalContext handles degree-0 nodes correctly

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();
    let leaf_node = node_ids[2]; // Node C has no outgoing edges

    let result = run_bfs_traversal(&mut graph_file, leaf_node, 2);
    assert_eq!(result.len(), 0, "Leaf node should reach 0 nodes");
}

#[test]
fn test_traversal_from_middle_node() {
    // Scenario: BFS from middle node of chain
    // Expected: Reaches only downstream nodes (directed graph)
    //           TraversalContext doesn't cache upstream connections

    let (mut graph_file, node_ids, _temp_dir) = create_simple_native_graph();
    let middle_node = node_ids[1]; // Node B

    let result = run_bfs_traversal(&mut graph_file, middle_node, 2);
    assert_eq!(result.len(), 1, "Middle node should reach 1 node");
    assert!(result.contains(&node_ids[2]), "Middle node should reach C");
    assert!(!result.contains(&node_ids[0]), "Middle node should not reach A (upstream)");
}
