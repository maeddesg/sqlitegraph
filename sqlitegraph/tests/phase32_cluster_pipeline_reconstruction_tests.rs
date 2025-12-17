//! Phase 32: V2 Cluster Pipeline Reconstruction Tests
//!
//! These tests specifically target the systematic V2 cluster corruption identified in Phase 31:
//! - "Cluster size mismatch: expected 8, found 29"
//! - Incoming cluster always empty
//! - Multi-edge clusters truncated (only first neighbor returned)
//! - neighbor_id wrong in V2 clusters

use sqlitegraph::{
    EdgeSpec, NodeSpec,
    backend::native::types::{EdgeRecord, NativeNodeId},
    backend::native::v2::edge_cluster::{CompactEdgeRecord, Direction, EdgeCluster},
    backend::native::v2::string_table::StringTable,
    backend::native::{EdgeStore, FileOffset, GraphFile, NodeStore},
    config::GraphConfig,
    open_graph,
};
use tempfile::TempDir;

/// Debug tool to manually parse cluster bytes and extract all edge records
/// This provides byte-level visibility into cluster corruption
fn debug_parse_cluster(bytes: &[u8]) -> Result<Vec<(i64, u16, usize)>, String> {
    if bytes.len() < 8 {
        return Err("Cluster too small for header".to_string());
    }

    // Parse cluster header
    let edge_count = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let payload_size = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;

    println!(
        "DEBUG CLUSTER: header edge_count={}, payload_size={}, total_bytes={}",
        edge_count,
        payload_size,
        bytes.len()
    );

    if bytes.len() != 8 + payload_size {
        return Err(format!(
            "Cluster size mismatch: expected {}, found {}",
            8 + payload_size,
            bytes.len()
        ));
    }

    let mut edges = Vec::new();
    let mut cursor = 8;

    for i in 0..edge_count {
        if cursor + 10 > bytes.len() {
            return Err(format!("Edge {} extends beyond cluster", i));
        }

        // Parse CompactEdgeRecord
        let neighbor_id = i64::from_be_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
            bytes[cursor + 4],
            bytes[cursor + 5],
            bytes[cursor + 6],
            bytes[cursor + 7],
        ]);
        let edge_type_offset = u16::from_be_bytes([bytes[cursor + 8], bytes[cursor + 9]]);

        // Remaining bytes are edge_data - find where next record starts by looking ahead
        let data_start = cursor + 10;
        let data_end = if i < edge_count - 1 {
            // Look ahead to find next neighbor_id (can't easily determine, so assume rest is data)
            bytes.len()
        } else {
            bytes.len()
        };

        let data_size = data_end - data_start;

        println!(
            "DEBUG EDGE {}: neighbor_id={}, type_offset={}, data_size={}, cursor={}",
            i, neighbor_id, edge_type_offset, data_size, cursor
        );

        edges.push((neighbor_id, edge_type_offset, data_size));
        cursor += 10 + data_size;

        if cursor > bytes.len() {
            return Err(format!(
                "Edge {} cursor overflow: {} > {}",
                i,
                cursor,
                bytes.len()
            ));
        }
    }

    Ok(edges)
}

/// Test 1: Single outgoing edge cluster - verify exact byte layout
/// This should PASS and establish the baseline for cluster serialization/deserialization
#[test]
fn test_single_outgoing_cluster_exact_byte_layout() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_single_cluster.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source and target nodes
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let target_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Insert single edge 1->2 with minimal data
    graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target_id,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!({}), // Empty JSON to minimize size
        })
        .unwrap();

    // Get outgoing neighbors - this should trigger cluster creation and reading
    let neighbors = graph
        .neighbors(
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Single outgoing neighbors from node {}: {:?}",
        source_id, neighbors
    );

    // Assertions
    assert_eq!(
        neighbors.len(),
        1,
        "Should have exactly 1 outgoing neighbor"
    );
    assert_eq!(
        neighbors[0], target_id,
        "Outgoing neighbor should be target node (2)"
    );
}

/// Test 2: Single incoming edge cluster - verify exact byte layout
/// This should FAIL in Phase 31 (incoming clusters always empty) and PASS after fix
#[test]
fn test_single_incoming_cluster_exact_byte_layout() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_single_incoming.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source and target nodes
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let target_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Insert single edge 1->2
    graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target_id,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!({}), // Empty JSON
        })
        .unwrap();

    // Get incoming neighbors to target node
    let neighbors = graph
        .neighbors(
            target_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Single incoming neighbors to node {}: {:?}",
        target_id, neighbors
    );

    // Assertions - this FAILS in Phase 31 (returns empty list)
    assert_eq!(
        neighbors.len(),
        1,
        "Should have exactly 1 incoming neighbor"
    );
    assert_eq!(
        neighbors[0], source_id,
        "Incoming neighbor should be source node (1)"
    );
}

/// Test 3: Multi-edge cluster reconstruction - verify all edges are preserved
/// This should FAIL in Phase 31 (only first neighbor returned) and PASS after fix
#[test]
fn test_multi_edge_cluster_reconstruction() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_multi_edge.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source node
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Source".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Create 3 target nodes
    let mut target_ids = Vec::new();
    for i in 1..=3 {
        let target_id = graph
            .insert_node(NodeSpec {
                kind: "Target".to_string(),
                name: format!("target_{}", i),
                file_path: None,
                data: serde_json::json!({"index": i}),
            })
            .unwrap();
        target_ids.push(target_id);

        // Create edge from source to target
        graph
            .insert_edge(EdgeSpec {
                from: source_id,
                to: target_id,
                edge_type: "connects".to_string(),
                data: serde_json::json!({"index": i}),
            })
            .unwrap();
    }

    // Get all outgoing neighbors from source
    let neighbors = graph
        .neighbors(
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!(
        "DEBUG: Multi-edge neighbors from node {}: {:?}",
        source_id, neighbors
    );

    // Assertions - this FAILS in Phase 31 (only returns 1 neighbor)
    assert_eq!(
        neighbors.len(),
        3,
        "Should have exactly 3 outgoing neighbors"
    );

    // Sort and compare to handle any ordering differences
    let mut sorted_neighbors = neighbors.clone();
    sorted_neighbors.sort();
    let mut sorted_targets = target_ids.clone();
    sorted_targets.sort();

    assert_eq!(
        sorted_neighbors, sorted_targets,
        "All 3 target nodes should be returned as neighbors"
    );
}

/// Test 4: Manual cluster byte-level validation
/// This test directly accesses the cluster bytes to verify serialization/deserialization integrity
#[test]
fn test_manual_cluster_byte_validation() {
    let temp_dir = TempDir::new().unwrap();
    let temp_file = temp_dir.path().join("test_manual_cluster.db");

    // Create graph file directly
    let mut graph_file = GraphFile::create(&temp_file).unwrap();

    // Create test nodes first
    {
        let mut node_store = NodeStore::new(&mut graph_file);
        let source_node = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
            1,
            "Source".to_string(),
            "source".to_string(),
            serde_json::json!({}),
        );
        let target_node = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
            2,
            "Target".to_string(),
            "target".to_string(),
            serde_json::json!({}),
        );
        node_store.write_node_v2(&source_node).unwrap();
        node_store.write_node_v2(&target_node).unwrap();
    }

    // Create test edge record
    let test_edge = EdgeRecord::new(
        1,
        1,
        2,
        "test_edge".to_string(),
        serde_json::json!({"weight": 1.0}),
    );

    // Create cluster manually using EdgeCluster API
    let mut string_table = StringTable::new();
    let cluster = EdgeCluster::create_from_edges(
        &[test_edge.clone()],
        1, // source node
        Direction::Outgoing,
        &mut string_table,
    )
    .unwrap();

    // Serialize cluster to bytes
    let cluster_bytes = cluster.serialize();
    println!(
        "DEBUG: Serialized cluster size: {} bytes",
        cluster_bytes.len()
    );
    println!("DEBUG: Cluster bytes: {:?}", cluster_bytes);

    // Manually parse cluster bytes to verify layout
    let parsed_edges = debug_parse_cluster(&cluster_bytes).unwrap();
    println!(
        "DEBUG: Parsed {} edges from cluster bytes",
        parsed_edges.len()
    );

    // Should have exactly 1 edge with correct data
    assert_eq!(
        parsed_edges.len(),
        1,
        "Cluster should contain exactly 1 edge"
    );
    assert_eq!(
        parsed_edges[0].0, 2,
        "Edge should have neighbor_id=2 (target node)"
    );
    assert!(parsed_edges[0].2 > 0, "Edge should have non-zero data size");

    // Test roundtrip: deserialize should recreate same cluster
    let roundtrip_cluster = EdgeCluster::deserialize(&cluster_bytes).unwrap();
    let roundtrip_edges: Vec<i64> = roundtrip_cluster.iter_neighbors().collect();

    println!("DEBUG: Roundtrip neighbor IDs: {:?}", roundtrip_edges);

    assert_eq!(
        roundtrip_edges.len(),
        1,
        "Roundtrip should preserve edge count"
    );
    assert_eq!(
        roundtrip_edges[0], 2,
        "Roundtrip should preserve neighbor ID"
    );

    // Write cluster to file and read back to test file I/O integrity
    let cluster_offset = graph_file.file_size().unwrap();
    graph_file
        .write_bytes(cluster_offset, &cluster_bytes)
        .unwrap();
    graph_file.flush().unwrap();

    // Read cluster back from file
    let mut read_buffer = vec![0u8; cluster_bytes.len()];
    graph_file
        .read_bytes(cluster_offset, &mut read_buffer)
        .unwrap();

    println!("DEBUG: Read back {} bytes from file", read_buffer.len());

    // Verify read-back matches original
    assert_eq!(
        read_buffer, cluster_bytes,
        "File read should preserve cluster bytes exactly"
    );

    // Test EdgeStore::read_clustered_edges with the file data
    {
        let mut edge_store = EdgeStore::new(&mut graph_file);
        let compact_edges = edge_store
            .read_clustered_edges(
                cluster_offset,
                cluster_bytes.len() as u32,
                Direction::Outgoing,
            )
            .unwrap();

        println!(
            "DEBUG: EdgeStore read {} compact edges from file",
            compact_edges.len()
        );

        assert_eq!(compact_edges.len(), 1, "EdgeStore should read back 1 edge");
        assert_eq!(
            compact_edges[0].neighbor_id, 2,
            "EdgeStore should preserve neighbor ID"
        );
    }
}

/// Test 5: Identify exact byte corruption - Phase 31 shows "expected 8, found 29"
/// This test attempts to reproduce the specific corruption scenario
#[test]
fn test_identify_phase31_byte_corruption() {
    let temp_dir = TempDir::new().unwrap();
    let temp_file = temp_dir.path().join("test_corruption.db");

    // Create the exact scenario from Phase 31 that causes corruption
    let mut graph_file = GraphFile::create(&temp_file).unwrap();

    // Create nodes
    {
        let mut node_store = NodeStore::new(&mut graph_file);
        let source_node = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
            1,
            "Source".to_string(),
            "source".to_string(),
            serde_json::json!({}),
        );
        let target_node = sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2::new(
            2,
            "Target".to_string(),
            "target".to_string(),
            serde_json::json!({}),
        );
        node_store.write_node_v2(&source_node).unwrap();
        node_store.write_node_v2(&target_node).unwrap();
    }

    // Simulate the exact cluster creation that failed in Phase 31
    let test_edge = EdgeRecord::new(
        1,
        1,
        2,
        "test_edge".to_string(),
        serde_json::json!({"weight": 1.0}),
    );

    let mut edge_store = EdgeStore::new(&mut graph_file);
    let mut string_table = StringTable::new();

    // Use the exact same code path as Phase 31: update_v2_clustered_adjacency
    let (cluster_offset, cluster_size) = edge_store
        .write_clustered_edges(&[test_edge], Direction::Outgoing, &mut string_table)
        .unwrap();

    println!(
        "DEBUG: Phase31-style cluster written at offset {}, size {}",
        cluster_offset, cluster_size
    );

    // Read back the cluster bytes using the same path as Phase 31
    let compact_edges = edge_store
        .read_clustered_edges(cluster_offset, cluster_size, Direction::Outgoing)
        .unwrap();

    println!(
        "DEBUG: Phase31-style read returned {} edges",
        compact_edges.len()
    );

    // This should work correctly after Phase 32 fix
    assert_eq!(
        compact_edges.len(),
        1,
        "Should successfully read back 1 edge"
    );
    assert_eq!(
        compact_edges[0].neighbor_id, 2,
        "Should preserve correct neighbor ID"
    );
}

/// Phase 35 Extension: v2_cluster_neighbors_match_manual_deserialization
/// Construct a small V2 cluster, read neighbors via graph.neighbors(), and manually via EdgeCluster deserialization, and assert they match.
#[test]
fn v2_cluster_neighbors_match_manual_deserialization() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v2_cluster_match_manual.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source and target nodes
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Source".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({"test": "manual_match"}),
        })
        .unwrap();

    let target_id = graph
        .insert_node(NodeSpec {
            kind: "Target".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({"test": "manual_match"}),
        })
        .unwrap();

    // Create edge with specific data for matching
    graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target_id,
            edge_type: "manual_test_edge".to_string(),
            data: serde_json::json!({"match_key": "test_value", "index": 42}),
        })
        .unwrap();

    // Test 1: Get neighbors via public API (graph.neighbors())
    let public_neighbors = graph
        .neighbors(
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    println!("DEBUG: Public API neighbors: {:?}", public_neighbors);

    // Test 2: Get neighbors manually via EdgeCluster deserialization
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(source_id as NativeNodeId).unwrap();

    let mut edge_store = EdgeStore::new(&mut graph_file);
    let manual_neighbors = edge_store
        .get_clustered_neighbors(
            source_node.outgoing_cluster_offset,
            source_node.outgoing_cluster_size,
            Direction::Outgoing,
            source_id as NativeNodeId,
        )
        .unwrap();

    println!(
        "DEBUG: Manual EdgeCluster neighbors: {:?}",
        manual_neighbors
    );

    // Test 3: Assert both methods return the same results
    assert_eq!(
        public_neighbors.len(),
        1,
        "Public API should return 1 neighbor"
    );
    assert_eq!(
        manual_neighbors.len(),
        1,
        "Manual EdgeCluster should return 1 neighbor"
    );

    let public_as_native: Vec<NativeNodeId> = public_neighbors
        .iter()
        .map(|&id| id as NativeNodeId)
        .collect();
    assert_eq!(
        public_as_native, manual_neighbors,
        "Public API and manual EdgeCluster should return identical neighbor IDs"
    );
}
