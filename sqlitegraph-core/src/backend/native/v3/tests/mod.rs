//! V3 Backend Algorithm Integration Tests
//!
//! These tests verify that V3 Backend GraphBackend operations work correctly
//! for the primitive operations that graph algorithms depend on:
//! - entity_ids() - enumerate all nodes
//! - neighbors() - traverse edges with direction filtering
//! - insert_node() / insert_edge() - build test graphs

use crate::backend::native::v3::{V3Backend, PersistentHeaderV3, V3_HEADER_SIZE, V3_FORMAT_VERSION};

/// Verify that key V3 module exports are available
#[test]
fn test_v3_module_exports() {
    // Verify key exports are available
    let _header = PersistentHeaderV3::new_v3();
    assert_eq!(V3_HEADER_SIZE, 112);
    assert_eq!(V3_FORMAT_VERSION, 4);
}
use crate::backend::sqlite::types::{BackendDirection, EdgeSpec, NeighborQuery, NodeSpec};
use crate::backend::GraphBackend;
use tempfile::TempDir;

/// Test that V3 backend can create nodes and enumerate them
#[test]
fn test_v3_entity_ids_basic() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.graph");
    
    let backend = V3Backend::create(&db_path).unwrap();
    
    // Initially empty
    let ids = backend.entity_ids().unwrap();
    assert!(ids.is_empty(), "New database should have no entities");
    
    // Insert nodes
    let node1 = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "A".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node2 = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "B".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    // Verify enumeration
    let ids = backend.entity_ids().unwrap();
    assert_eq!(ids.len(), 2, "Should have 2 entities");
    assert!(ids.contains(&node1), "Should contain node1");
    assert!(ids.contains(&node2), "Should contain node2");
}

/// Test that V3 backend can create edges and traverse them
#[test]
fn test_v3_outgoing_edges() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.graph");
    
    let backend = V3Backend::create(&db_path).unwrap();
    
    // Create a chain: A -> B -> C
    let node_a = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "A".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node_b = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "B".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node_c = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "C".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    // Insert edges
    backend.insert_edge(EdgeSpec {
        from: node_a,
        to: node_b,
        edge_type: "links_to".to_string(),
        data: serde_json::json!({}),
    }).unwrap();
    
    backend.insert_edge(EdgeSpec {
        from: node_b,
        to: node_c,
        edge_type: "links_to".to_string(),
        data: serde_json::json!({}),
    }).unwrap();
    
    // Test outgoing edges
    let snapshot = crate::snapshot::SnapshotId::current();
    let outgoing_a = backend.neighbors(
        snapshot, 
        node_a, 
        NeighborQuery { direction: BackendDirection::Outgoing, edge_type: None }
    ).unwrap();
    assert_eq!(outgoing_a.len(), 1, "A should have 1 outgoing edge");
    assert!(outgoing_a.contains(&node_b), "A should point to B");
    
    let outgoing_b = backend.neighbors(
        snapshot, 
        node_b, 
        NeighborQuery { direction: BackendDirection::Outgoing, edge_type: None }
    ).unwrap();
    assert_eq!(outgoing_b.len(), 1, "B should have 1 outgoing edge");
    assert!(outgoing_b.contains(&node_c), "B should point to C");
    
    let outgoing_c = backend.neighbors(
        snapshot, 
        node_c, 
        NeighborQuery { direction: BackendDirection::Outgoing, edge_type: None }
    ).unwrap();
    assert!(outgoing_c.is_empty(), "C should have no outgoing edges");
}

/// Test incoming edges (reverse traversal)
#[test]
fn test_v3_incoming_edges() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.graph");
    
    let backend = V3Backend::create(&db_path).unwrap();
    
    // Create a star: A -> B, A -> C, A -> D
    let node_a = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "A".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node_b = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "B".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node_c = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "C".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node_d = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "D".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    // Insert edges from A to B, C, D
    for target in [node_b, node_c, node_d] {
        backend.insert_edge(EdgeSpec {
            from: node_a,
            to: target,
            edge_type: "links_to".to_string(),
            data: serde_json::json!({}),
        }).unwrap();
    }
    
    // Test incoming edges
    let snapshot = crate::snapshot::SnapshotId::current();
    
    let incoming_b = backend.neighbors(
        snapshot, 
        node_b, 
        NeighborQuery { direction: BackendDirection::Incoming, edge_type: None }
    ).unwrap();
    assert_eq!(incoming_b.len(), 1, "B should have 1 incoming edge");
    assert!(incoming_b.contains(&node_a), "B should be pointed to by A");
    
    let incoming_c = backend.neighbors(
        snapshot, 
        node_c, 
        NeighborQuery { direction: BackendDirection::Incoming, edge_type: None }
    ).unwrap();
    assert_eq!(incoming_c.len(), 1, "C should have 1 incoming edge");
    assert!(incoming_c.contains(&node_a), "C should be pointed to by A");
    
    let incoming_a = backend.neighbors(
        snapshot, 
        node_a, 
        NeighborQuery { direction: BackendDirection::Incoming, edge_type: None }
    ).unwrap();
    assert!(incoming_a.is_empty(), "A should have no incoming edges");
}

/// Test k-hop traversal (used by many algorithms)
#[test]
fn test_v3_k_hop_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.graph");
    
    let backend = V3Backend::create(&db_path).unwrap();
    
    // Create a binary tree: A -> B, A -> C, B -> D, B -> E
    let node_a = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "A".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node_b = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "B".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node_c = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "C".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node_d = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "D".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node_e = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "E".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    // Insert edges
    backend.insert_edge(EdgeSpec {
        from: node_a,
        to: node_b,
        edge_type: "links_to".to_string(),
        data: serde_json::json!({}),
    }).unwrap();
    
    backend.insert_edge(EdgeSpec {
        from: node_a,
        to: node_c,
        edge_type: "links_to".to_string(),
        data: serde_json::json!({}),
    }).unwrap();
    
    backend.insert_edge(EdgeSpec {
        from: node_b,
        to: node_d,
        edge_type: "links_to".to_string(),
        data: serde_json::json!({}),
    }).unwrap();
    
    backend.insert_edge(EdgeSpec {
        from: node_b,
        to: node_e,
        edge_type: "links_to".to_string(),
        data: serde_json::json!({}),
    }).unwrap();
    
    // Test k-hop from A
    let snapshot = crate::snapshot::SnapshotId::current();
    
    // k_hop returns all nodes within k hops (not exactly at k hops)
    // 1-hop: B, C (nodes within 1 hop)
    let hop1 = backend.k_hop(snapshot, node_a, 1, BackendDirection::Outgoing).unwrap();
    assert_eq!(hop1.len(), 2, "A should have 2 nodes within 1 hop");
    assert!(hop1.contains(&node_b), "B should be within 1 hop from A");
    assert!(hop1.contains(&node_c), "C should be within 1 hop from A");
    
    // 2-hop: B, C, D, E (nodes within 2 hops, includes 1-hop neighbors)
    let hop2 = backend.k_hop(snapshot, node_a, 2, BackendDirection::Outgoing).unwrap();
    assert_eq!(hop2.len(), 4, "A should have 4 nodes within 2 hops");
    assert!(hop2.contains(&node_b), "B should be within 2 hops from A");
    assert!(hop2.contains(&node_c), "C should be within 2 hops from A");
    assert!(hop2.contains(&node_d), "D should be within 2 hops from A");
    assert!(hop2.contains(&node_e), "E should be within 2 hops from A");
}

/// Test node degree (incoming + outgoing counts)
#[test]
fn test_v3_node_degree() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.graph");
    
    let backend = V3Backend::create(&db_path).unwrap();
    
    // Create a bidirectional link: A <-> B
    let node_a = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "A".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    let node_b = backend.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "B".to_string(),
        file_path: None,
        data: serde_json::json!({}),
    }).unwrap();
    
    // A -> B
    backend.insert_edge(EdgeSpec {
        from: node_a,
        to: node_b,
        edge_type: "links_to".to_string(),
        data: serde_json::json!({}),
    }).unwrap();
    
    // B -> A (making it bidirectional)
    backend.insert_edge(EdgeSpec {
        from: node_b,
        to: node_a,
        edge_type: "links_to".to_string(),
        data: serde_json::json!({}),
    }).unwrap();
    
    // Check degrees
    let snapshot = crate::snapshot::SnapshotId::current();
    
    let (out_a, in_a) = backend.node_degree(snapshot, node_a).unwrap();
    assert_eq!(out_a, 1, "A should have 1 outgoing edge");
    assert_eq!(in_a, 1, "A should have 1 incoming edge");
    
    let (out_b, in_b) = backend.node_degree(snapshot, node_b).unwrap();
    assert_eq!(out_b, 1, "B should have 1 outgoing edge");
    assert_eq!(in_b, 1, "B should have 1 incoming edge");
}

/// Test persistence - database file is created and can be reopened
/// 
/// NOTE: Full persistence (loading nodes after reopen) requires NodeStore 
/// disk loading which is not yet implemented. This test verifies basic 
/// file creation and header persistence.
#[test]
fn test_v3_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.graph");
    
    // Create and populate
    {
        let backend = V3Backend::create(&db_path).unwrap();
        let id = backend.insert_node(NodeSpec {
            kind: "Test".to_string(),
            name: "Persistent".to_string(),
            file_path: None,
            data: serde_json::json!({"key": "value"}),
        }).unwrap();
        
        // Verify before close - node exists with correct ID
        let snapshot = crate::snapshot::SnapshotId::current();
        let node = backend.get_node(snapshot, id).unwrap();
        assert_eq!(node.id, id, "Node ID should match");
    }
    
    // Verify file was created
    assert!(db_path.exists(), "Database file should exist after create");
    
    // Reopen database - header should be valid
    let backend = V3Backend::open(&db_path).unwrap();
    let header = backend.header();
    assert_eq!(header.magic, crate::backend::native::v3::V3_MAGIC, "Header magic should be valid");
    assert_eq!(header.version, crate::backend::native::v3::V3_FORMAT_VERSION, "Header version should be valid");
    // NOTE: NodeStore disk loading not yet implemented - cannot verify node data after reopen
}
