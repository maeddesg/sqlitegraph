//! L1 Buffer Neighbor Extraction Tests - SequentialReadBuffer L1 cache validation
//!
//! **Purpose:** Validate that L1 buffer neighbor extraction works correctly
//! **Scope:** Unit tests for get_neighbors_optimized() L1 extraction path
//! **Focus:** Buffer hit returns neighbors from buffer, buffer miss falls through to L2/L3
//!
//! **Key Design Principle:**
//! When a node is in the SequentialReadBuffer and has valid cluster metadata,
//! get_neighbors_optimized() should extract neighbors directly from the buffered
//! NodeRecordV2 by:
//! 1. Extracting cluster_offset and cluster_size based on direction
//! 2. Reading cluster data from file at cluster_offset
//! 3. Deserializing EdgeCluster and extracting neighbors via iter_neighbors()
//! 4. Returning neighbors immediately (early return, no L2/L3 fallback)
//!
//! These tests verify that the L1 buffer extraction implementation (Phase 32-04)
//! correctly handles outgoing/incoming directions, buffer misses, and empty clusters.

use sqlitegraph::{
    backend::native::{
        adjacency::{Direction, SequentialReadBuffer},
        graph_file::GraphFile,
        graph_ops::{get_neighbors_optimized, TraversalContext},
        node_store::NodeStore,
        edge_store::EdgeStore,
        v2::node_record_v2::NodeRecordV2,
        NativeNodeId,
    },
    backend::native::EdgeRecord,
};
use tempfile::TempDir;

//
// TEST HELPERS
//

/// Helper: Create a simple Native graph with A->B->C->D->E chain
///
/// Creates a linear chain graph with V2 clustered adjacency:
/// A -> B -> C -> D -> E
///
/// Returns:
/// - GraphFile: The native graph file
/// - Vec<NativeNodeId>: Node IDs [A, B, C, D, E]
/// - TempDir: Temporary directory (kept for cleanup)
fn create_test_chain_graph() -> (GraphFile, Vec<NativeNodeId>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_chain.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Create nodes A, B, C, D, E
    let mut node_ids = Vec::new();
    for i in 1..=5 {
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

    // Create edges A->B, B->C, C->D, D->E using EdgeStore (creates V2 clusters)
    let mut edge_store = EdgeStore::new(&mut graph_file);
    for i in 0..4 {
        let edge = EdgeRecord::new(
            (i + 1) as i64, // edge_id
            node_ids[i],    // from
            node_ids[i + 1], // to
            "connects".to_string(),
            serde_json::json!({}),
        );
        edge_store.write_edge(&edge).expect("Failed to write edge");
    }

    (graph_file, node_ids, temp_dir)
}

/// Helper: Manually populate SequentialReadBuffer with a NodeRecordV2
///
/// This creates a NodeRecordV2 with specified cluster metadata and inserts
/// it into the buffer for testing L1 extraction without going through
/// the full edge cluster creation process.
///
/// Parameters:
/// - buffer: Mutable reference to SequentialReadBuffer
/// - node_id: Node ID to insert
/// - outgoing_offset: Outgoing cluster offset (0 if no cluster)
/// - outgoing_size: Outgoing cluster size
/// - incoming_offset: Incoming cluster offset (0 if no cluster)
/// - incoming_size: Incoming cluster size
fn populate_buffer_with_node(
    buffer: &mut SequentialReadBuffer,
    node_id: NativeNodeId,
    outgoing_offset: u64,
    outgoing_size: u32,
    incoming_offset: u64,
    incoming_size: u32,
) {
    let node_record = NodeRecordV2 {
        id: node_id,
        flags: sqlitegraph::backend::native::NodeFlags::empty(),
        kind: "Test".to_string(),
        name: format!("node_{}", node_id),
        data: serde_json::json!({}),
        outgoing_cluster_offset: outgoing_offset,
        outgoing_cluster_size: outgoing_size,
        outgoing_edge_count: if outgoing_size > 0 { 1 } else { 0 },
        incoming_cluster_offset: incoming_offset,
        incoming_cluster_size: incoming_size,
        incoming_edge_count: if incoming_size > 0 { 1 } else { 0 },
    };
    buffer.insert(node_record);
}

/// Helper: Create a TraversalContext with linear pattern already confirmed
///
/// This simulates the state after LinearDetector has confirmed a linear pattern,
/// which enables L1 buffer lookup.
///
/// Returns:
/// - TraversalContext with detector in LINEAR_CONFIRMED state
fn create_linear_context() -> TraversalContext {
    let mut ctx = TraversalContext::new();

    // Simulate linear pattern detection by observing 3+ steps with degree <= 1
    // This triggers the LinearDetector to enter LINEAR_CONFIRMED state
    ctx.detector.observe(1, 1); // node 1, degree 1
    ctx.detector.observe(2, 1); // node 2, degree 1
    ctx.detector.observe(3, 1); // node 3, degree 1

    // After 3 observations with degree <= 1, detector should be linear confirmed
    assert!(ctx.detector.is_linear_confirmed(), "LinearDetector should be confirmed after 3 linear steps");

    ctx
}

//
// GROUP 1: L1 BUFFER EXTRACTION TESTS
//
// These tests verify that get_neighbors_optimized() correctly extracts
// neighbors from the SequentialReadBuffer when buffer hit occurs.
//

#[test]
fn test_l1_buffer_returns_neighbors_from_buffer() {
    // Scenario: Node is in SequentialReadBuffer with valid cluster metadata
    // Expected: get_neighbors_optimized() returns neighbors from buffer (not L2/L3)
    //           buffer_hits > 0

    let (mut graph_file, node_ids, _temp_dir) = create_test_chain_graph();
    let mut ctx = create_linear_context();

    // Prefetch node 1 into buffer (this reads and decodes the slot)
    ctx.buffer.prefetch_from(&mut graph_file, node_ids[0])
        .expect("Prefetch should succeed");

    // Verify node is in buffer
    assert!(ctx.buffer.contains(node_ids[0]), "Node should be in buffer after prefetch");

    // Get neighbors via optimized path
    let neighbors = get_neighbors_optimized(
        &mut graph_file,
        node_ids[0],
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    // Should find node B (single outgoing neighbor)
    assert_eq!(neighbors.len(), 1, "Node A should have 1 outgoing neighbor");
    assert_eq!(neighbors[0], node_ids[1], "Neighbor should be node B");

    // Buffer hit should be recorded
    assert_eq!(ctx.buffer_hits, 1, "Should have 1 buffer hit");
    assert_eq!(ctx.buffer_misses, 0, "Should have 0 buffer misses");

    // L2 cache should be populated with result
    assert!(ctx.cache.contains_key(&(node_ids[0], Direction::Outgoing)),
            "L2 cache should contain the result");
}

#[test]
fn test_l1_buffer_outgoing_direction() {
    // Scenario: Node with outgoing edges: 1 -> {2, 3, 4}
    // Expected: get_neighbors_optimized() returns correct neighbors [2, 3, 4]

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_multi_outgoing.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Create node 1 with multiple outgoing edges
    let mut node_store = NodeStore::new(&mut graph_file);
    let node1 = node_store.allocate_node_id().expect("Failed to allocate node ID");
    let record = sqlitegraph::backend::native::NodeRecord::new(
        node1,
        "hub".to_string(),
        "hub_node".to_string(),
        serde_json::json!({}),
    );
    node_store.write_node(&record).expect("Failed to write node");

    // Create nodes 2, 3, 4
    let mut targets = Vec::new();
    for i in 2..=4 {
        let mut node_store = NodeStore::new(&mut graph_file);
        let node_id = node_store.allocate_node_id().expect("Failed to allocate node ID");
        let record = sqlitegraph::backend::native::NodeRecord::new(
            node_id,
            "target".to_string(),
            format!("target_{}", i),
            serde_json::json!({}),
        );
        node_store.write_node(&record).expect("Failed to write node");
        targets.push(node_id);
    }

    // Create edges 1->2, 1->3, 1->4
    let mut edge_store = EdgeStore::new(&mut graph_file);
    for &target in &targets {
        let edge = EdgeRecord::new(
            target, // edge_id (use target as unique ID)
            node1,  // from
            target, // to
            "connects".to_string(),
            serde_json::json!({}),
        );
        edge_store.write_edge(&edge).expect("Failed to write edge");
    }

    // Create traversal context with linear pattern confirmed
    let mut ctx = create_linear_context();

    // Prefetch hub node into buffer
    ctx.buffer.prefetch_from(&mut graph_file, node1)
        .expect("Prefetch should succeed");

    // Get outgoing neighbors
    let neighbors = get_neighbors_optimized(
        &mut graph_file,
        node1,
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    // Should find all 3 targets
    assert_eq!(neighbors.len(), 3, "Hub node should have 3 outgoing neighbors");

    // All targets should be present
    for target in &targets {
        assert!(neighbors.contains(target), "Neighbors should contain target {}", target);
    }

    // Buffer hit should be recorded
    assert_eq!(ctx.buffer_hits, 1, "Should have 1 buffer hit");
}

#[test]
fn test_l1_buffer_incoming_direction() {
    // Scenario: Node with incoming edges: {1, 2, 3} -> 4
    // Expected: get_neighbors_optimized() returns correct neighbors [1, 2, 3]

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_incoming.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Create nodes 1, 2, 3 (sources)
    let mut sources = Vec::new();
    for i in 1..=3 {
        let mut node_store = NodeStore::new(&mut graph_file);
        let node_id = node_store.allocate_node_id().expect("Failed to allocate node ID");
        let record = sqlitegraph::backend::native::NodeRecord::new(
            node_id,
            "source".to_string(),
            format!("source_{}", i),
            serde_json::json!({}),
        );
        node_store.write_node(&record).expect("Failed to write node");
        sources.push(node_id);
    }

    // Create node 4 (target)
    let mut node_store = NodeStore::new(&mut graph_file);
    let node4 = node_store.allocate_node_id().expect("Failed to allocate node ID");
    let record = sqlitegraph::backend::native::NodeRecord::new(
        node4,
        "target".to_string(),
        "target_node".to_string(),
        serde_json::json!({}),
    );
    node_store.write_node(&record).expect("Failed to write node");

    // Create edges 1->4, 2->4, 3->4 (so node 4 has 3 incoming edges)
    let mut edge_store = EdgeStore::new(&mut graph_file);
    for &source in &sources {
        let edge = EdgeRecord::new(
            source, // edge_id
            source, // from
            node4,  // to
            "connects".to_string(),
            serde_json::json!({}),
        );
        edge_store.write_edge(&edge).expect("Failed to write edge");
    }

    // Create traversal context with linear pattern confirmed
    let mut ctx = create_linear_context();

    // Prefetch target node into buffer
    ctx.buffer.prefetch_from(&mut graph_file, node4)
        .expect("Prefetch should succeed");

    // Get incoming neighbors
    let neighbors = get_neighbors_optimized(
        &mut graph_file,
        node4,
        Direction::Incoming,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    // Should find all 3 sources
    assert_eq!(neighbors.len(), 3, "Target node should have 3 incoming neighbors");

    // All sources should be present
    for source in &sources {
        assert!(neighbors.contains(source), "Neighbors should contain source {}", source);
    }

    // Buffer hit should be recorded
    assert_eq!(ctx.buffer_hits, 1, "Should have 1 buffer hit");
}

#[test]
fn test_l1_buffer_fallback_to_l2_on_miss() {
    // Scenario: Call get_neighbors_optimized() for node NOT in buffer
    // Expected: Neighbors returned from L2/L3 (buffer_misses > 0)

    let (mut graph_file, node_ids, _temp_dir) = create_test_chain_graph();
    let mut ctx = create_linear_context();

    // Do NOT prefetch node - it won't be in buffer
    assert!(!ctx.buffer.contains(node_ids[0]), "Node should not be in buffer");

    // Get neighbors - should fall through to L2/L3
    let neighbors = get_neighbors_optimized(
        &mut graph_file,
        node_ids[0],
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    // Should still find node B (via L2/L3 fallback)
    assert_eq!(neighbors.len(), 1, "Node A should have 1 outgoing neighbor");
    assert_eq!(neighbors[0], node_ids[1], "Neighbor should be node B");

    // Buffer miss should be recorded
    assert_eq!(ctx.buffer_hits, 0, "Should have 0 buffer hits");
    assert_eq!(ctx.buffer_misses, 1, "Should have 1 buffer miss");

    // L2 cache should be populated with result
    assert!(ctx.cache.contains_key(&(node_ids[0], Direction::Outgoing)),
            "L2 cache should contain the result");

    // Subsequent call should hit L2 cache
    let neighbors2 = get_neighbors_optimized(
        &mut graph_file,
        node_ids[0],
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    assert_eq!(neighbors2, neighbors, "Second call should return same result");
    assert_eq!(ctx.stats.hits, 1, "Should have 1 L2 cache hit on second call");
}

#[test]
fn test_l1_buffer_empty_cluster() {
    // Scenario: NodeRecordV2 with cluster_offset = 0 (no cluster)
    // Expected: Returns empty neighbors (no panic, no L2/L3 needed)

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_empty_cluster.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Create a simple node
    let mut node_store = NodeStore::new(&mut graph_file);
    let node1 = node_store.allocate_node_id().expect("Failed to allocate node ID");
    let record = sqlitegraph::backend::native::NodeRecord::new(
        node1,
        "empty".to_string(),
        "empty_node".to_string(),
        serde_json::json!({}),
    );
    node_store.write_node(&record).expect("Failed to write node");

    // Create traversal context with linear pattern confirmed
    let mut ctx = create_linear_context();

    // Manually insert node with cluster_offset = 0 into buffer
    populate_buffer_with_node(
        &mut ctx.buffer,
        node1,
        0, // outgoing_cluster_offset = 0 (no cluster)
        0, // outgoing_cluster_size = 0
        0, // incoming_cluster_offset = 0
        0, // incoming_cluster_size = 0
    );

    // Verify node is in buffer
    assert!(ctx.buffer.contains(node1), "Node should be in buffer");

    // Get neighbors - should return empty immediately
    let neighbors = get_neighbors_optimized(
        &mut graph_file,
        node1,
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    // Should return empty neighbors
    assert_eq!(neighbors.len(), 0, "Node with no cluster should have 0 neighbors");

    // Buffer hit should be recorded (node was in buffer)
    assert_eq!(ctx.buffer_hits, 1, "Should have 1 buffer hit");
    assert_eq!(ctx.buffer_misses, 0, "Should have 0 buffer misses");

    // L2 cache should have empty result cached
    assert!(ctx.cache.contains_key(&(node1, Direction::Outgoing)),
            "L2 cache should contain empty result");
}

//
// GROUP 2: L1 + L2 CACHE INTERACTION TESTS
//
// These tests verify that L1 buffer extraction correctly populates L2 cache.
//

#[test]
fn test_l1_buffer_populates_l2_cache() {
    // Scenario: L1 buffer hit should also populate L2 cache
    // Expected: After L1 hit, L2 cache contains the result

    let (mut graph_file, node_ids, _temp_dir) = create_test_chain_graph();
    let mut ctx = create_linear_context();

    // Prefetch node into buffer
    ctx.buffer.prefetch_from(&mut graph_file, node_ids[0])
        .expect("Prefetch should succeed");

    // First call via L1
    let neighbors1 = get_neighbors_optimized(
        &mut graph_file,
        node_ids[0],
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    // L2 cache should be populated
    assert!(ctx.cache.contains_key(&(node_ids[0], Direction::Outgoing)),
            "L2 cache should be populated after L1 hit");

    // Remove node from buffer (simulate buffer eviction)
    ctx.buffer.clear();

    // Second call should hit L2 cache
    let neighbors2 = get_neighbors_optimized(
        &mut graph_file,
        node_ids[0],
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    assert_eq!(neighbors2, neighbors1, "L2 result should match L1 result");
    assert_eq!(ctx.stats.hits, 1, "Should have 1 L2 cache hit");
}

//
// GROUP 3: DIRECTION HANDLING TESTS
//
// These tests verify that L1 buffer extraction correctly handles
// Direction::Outgoing and Direction::Incoming separately.
//

#[test]
fn test_l1_buffer_directions_are_independent() {
    // Scenario: Query both outgoing and incoming directions
    // Expected: Each direction uses correct cluster offset/size

    let (mut graph_file, node_ids, _temp_dir) = create_test_chain_graph();
    let mut ctx = create_linear_context();

    // Node B (index 1) has both incoming (from A) and outgoing (to C)
    let node_b = node_ids[1];

    // Prefetch node B into buffer
    ctx.buffer.prefetch_from(&mut graph_file, node_b)
        .expect("Prefetch should succeed");

    // Get outgoing neighbors (should be C)
    let outgoing = get_neighbors_optimized(
        &mut graph_file,
        node_b,
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    assert_eq!(outgoing.len(), 1, "Node B should have 1 outgoing neighbor");
    assert_eq!(outgoing[0], node_ids[2], "Outgoing neighbor should be C");

    // Get incoming neighbors (should be A)
    let incoming = get_neighbors_optimized(
        &mut graph_file,
        node_b,
        Direction::Incoming,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    assert_eq!(incoming.len(), 1, "Node B should have 1 incoming neighbor");
    assert_eq!(incoming[0], node_ids[0], "Incoming neighbor should be A");

    // Both buffer hits should be recorded
    assert_eq!(ctx.buffer_hits, 2, "Should have 2 buffer hits");

    // Both directions should be cached separately
    assert!(ctx.cache.contains_key(&(node_b, Direction::Outgoing)),
            "L2 cache should contain outgoing result");
    assert!(ctx.cache.contains_key(&(node_b, Direction::Incoming)),
            "L2 cache should contain incoming result");
}

//
// GROUP 4: LINEAR DETECTOR INTERACTION TESTS
//
// These tests verify that L1 buffer lookup only happens after
// LinearDetector confirms linear pattern.
//

#[test]
fn test_l1_buffer_only_checked_after_linear_confirmed() {
    // Scenario: LinearDetector not yet confirmed
    // Expected: L1 buffer lookup skipped, falls through to L2/L3

    let (mut graph_file, node_ids, _temp_dir) = create_test_chain_graph();
    let mut ctx = TraversalContext::new(); // Not linear confirmed yet

    // Manually insert node into buffer
    ctx.buffer.prefetch_from(&mut graph_file, node_ids[0])
        .expect("Prefetch should succeed");

    // Verify node is in buffer
    assert!(ctx.buffer.contains(node_ids[0]), "Node should be in buffer");

    // Verify linear detector is NOT confirmed
    assert!(!ctx.detector.is_linear_confirmed(), "LinearDetector should not be confirmed yet");

    // Get neighbors - should skip L1 and fall through to L2/L3
    let neighbors = get_neighbors_optimized(
        &mut graph_file,
        node_ids[0],
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    // Should still find neighbor
    assert_eq!(neighbors.len(), 1, "Node A should have 1 outgoing neighbor");

    // No buffer hit/miss should be recorded (L1 not checked)
    assert_eq!(ctx.buffer_hits, 0, "Should have 0 buffer hits when not linear confirmed");
    assert_eq!(ctx.buffer_misses, 0, "Should have 0 buffer misses when not linear confirmed");

    // Should have L2 cache miss
    assert_eq!(ctx.stats.misses, 1, "Should have 1 L2 cache miss");
}

//
// GROUP 5: CLUSTER CACHE TESTS (Phase 32-05)
//
// These tests verify that edge cluster caching works correctly when using
// prefetch_clusters_from() to prefetch both node slots and edge clusters.
//

#[test]
fn test_cluster_cache_hit() {
    // Scenario: Cluster is prefetched using prefetch_clusters_from()
    // Expected: get_neighbors_optimized() returns neighbors from cached cluster (no I/O)

    let (mut graph_file, node_ids, _temp_dir) = create_test_chain_graph();
    let mut ctx = create_linear_context();

    // Prefetch node slots AND edge clusters
    ctx.buffer.prefetch_clusters_from(&mut graph_file, node_ids[0])
        .expect("prefetch_clusters_from should succeed");

    // Verify node is in buffer
    assert!(ctx.buffer.contains(node_ids[0]), "Node should be in buffer after prefetch");

    // Get neighbors via optimized path
    let neighbors = get_neighbors_optimized(
        &mut graph_file,
        node_ids[0],
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    // Should find node B (single outgoing neighbor)
    assert_eq!(neighbors.len(), 1, "Node A should have 1 outgoing neighbor");
    assert_eq!(neighbors[0], node_ids[1], "Neighbor should be node B");

    // Buffer hit should be recorded
    assert_eq!(ctx.buffer_hits, 1, "Should have 1 buffer hit");
    assert_eq!(ctx.buffer_misses, 0, "Should have 0 buffer misses");
}

#[test]
fn test_cluster_cache_miss() {
    // Scenario: Node is in buffer but cluster was NOT prefetched
    // Expected: Falls back to file I/O for cluster read

    let (mut graph_file, node_ids, _temp_dir) = create_test_chain_graph();
    let mut ctx = create_linear_context();

    // Prefetch ONLY node slots (not clusters)
    ctx.buffer.prefetch_from(&mut graph_file, node_ids[0])
        .expect("prefetch_from should succeed");

    // Verify node is in buffer
    assert!(ctx.buffer.contains(node_ids[0]), "Node should be in buffer");

    // Verify cluster is NOT cached (we used prefetch_from, not prefetch_clusters_from)
    let node_record = ctx.buffer.get(node_ids[0]).unwrap();
    assert!(!ctx.buffer.has_cluster(node_record.outgoing_cluster_offset),
            "Cluster should not be cached when using prefetch_from");

    // Get neighbors - should fall back to file I/O for cluster
    let neighbors = get_neighbors_optimized(
        &mut graph_file,
        node_ids[0],
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized should succeed");

    // Should still find node B (via file I/O)
    assert_eq!(neighbors.len(), 1, "Node A should have 1 outgoing neighbor");
    assert_eq!(neighbors[0], node_ids[1], "Neighbor should be node B");

    // Buffer hit should be recorded (node was in buffer)
    assert_eq!(ctx.buffer_hits, 1, "Should have 1 buffer hit");
}

#[test]
fn test_cluster_prefetch_batch() {
    // Scenario: prefetch_clusters_from() loads clusters for all buffered nodes
    // Expected: Multiple nodes and their clusters are prefetched

    let (mut graph_file, node_ids, _temp_dir) = create_test_chain_graph();
    let mut ctx = create_linear_context();

    // Prefetch starting from node 1 (should prefetch nodes 1-8 in window)
    ctx.buffer.prefetch_clusters_from(&mut graph_file, node_ids[0])
        .expect("prefetch_clusters_from should succeed");

    // Verify all nodes in the chain are in buffer
    for (i, &node_id) in node_ids.iter().enumerate() {
        assert!(ctx.buffer.contains(node_id),
                "Node {} should be in buffer", i + 1);
    }

    // Get the first node to check its cluster
    let node_record = ctx.buffer.get(node_ids[0]).unwrap();

    // Verify outgoing cluster is cached
    if node_record.outgoing_cluster_offset > 0 {
        assert!(ctx.buffer.has_cluster(node_record.outgoing_cluster_offset),
                "Outgoing cluster should be cached");
        assert!(ctx.buffer.get_cluster(node_record.outgoing_cluster_offset).is_some(),
                "Should be able to get cached cluster");
    }

    // Get the second node to check its clusters
    let node_record = ctx.buffer.get(node_ids[1]).unwrap();

    // Verify both outgoing and incoming clusters are cached (if non-zero)
    if node_record.outgoing_cluster_offset > 0 {
        assert!(ctx.buffer.has_cluster(node_record.outgoing_cluster_offset),
                "Outgoing cluster for node 2 should be cached");
    }
    if node_record.incoming_cluster_offset > 0 {
        assert!(ctx.buffer.has_cluster(node_record.incoming_cluster_offset),
                "Incoming cluster for node 2 should be cached");
    }
}

#[test]
fn test_cluster_cache_clear() {
    // Scenario: buffer.clear() should clear both node slots and cluster cache
    // Expected: After clear, both slots and clusters are empty

    let (mut graph_file, node_ids, _temp_dir) = create_test_chain_graph();
    let mut ctx = create_linear_context();

    // Prefetch nodes AND clusters
    ctx.buffer.prefetch_clusters_from(&mut graph_file, node_ids[0])
        .expect("prefetch_clusters_from should succeed");

    // Verify nodes and clusters are cached
    assert!(ctx.buffer.contains(node_ids[0]), "Node should be in buffer");
    let node_record = ctx.buffer.get(node_ids[0]).unwrap();
    if node_record.outgoing_cluster_offset > 0 {
        assert!(ctx.buffer.has_cluster(node_record.outgoing_cluster_offset),
                "Cluster should be cached");
    }

    // Clear the buffer
    ctx.buffer.clear();

    // Verify both nodes and clusters are cleared
    assert!(!ctx.buffer.contains(node_ids[0]), "Node should be cleared");
    assert_eq!(ctx.buffer.len(), 0, "Buffer should be empty");
    assert_eq!(ctx.buffer.cluster_cache_len(), 0, "Cluster cache should be empty");
}

#[test]
fn test_cluster_cache_with_linear_traversal() {
    // Scenario: Simulate a linear traversal using prefetch_clusters_from
    // Expected: All nodes accessible from cached data

    let (mut graph_file, node_ids, _temp_dir) = create_test_chain_graph();
    let mut ctx = create_linear_context();

    // Simulate chain traversal: A -> B -> C -> D -> E
    // Start by prefetching from node A
    ctx.buffer.prefetch_clusters_from(&mut graph_file, node_ids[0])
        .expect("First prefetch should succeed");

    // Get neighbors of A (should be B)
    let neighbors_a = get_neighbors_optimized(
        &mut graph_file,
        node_ids[0],
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized for A should succeed");

    assert_eq!(neighbors_a.len(), 1);
    assert_eq!(neighbors_a[0], node_ids[1]);

    // Get neighbors of B (should be C)
    let neighbors_b = get_neighbors_optimized(
        &mut graph_file,
        node_ids[1],
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized for B should succeed");

    assert_eq!(neighbors_b.len(), 1);
    assert_eq!(neighbors_b[0], node_ids[2]);

    // Get neighbors of C (should be D)
    let neighbors_c = get_neighbors_optimized(
        &mut graph_file,
        node_ids[2],
        Direction::Outgoing,
        &mut ctx,
    ).expect("get_neighbors_optimized for C should succeed");

    assert_eq!(neighbors_c.len(), 1);
    assert_eq!(neighbors_c[0], node_ids[3]);

    // Most lookups should be buffer hits (nodes were in prefetch window)
    assert!(ctx.buffer_hits >= 2, "Should have at least 2 buffer hits");
}
