//! Phase 30.1: V2 Edge Boundary Integration Tests
//!
//! TDD tests to reproduce the exact edge boundary failures:
//! - incorrect neighbor lists
//! - cluster offsets that don't match serialized values
//! - cluster size misalignment
//! - truncated cluster payloads
//! - cluster reading using wrong ranges

#![cfg(feature = "v2_experimental")]

use sqlitegraph::backend::native::{
    edge_store::EdgeStore,
    graph_file::GraphFile,
    node_store::NodeStore,
    types::EdgeRecord,
    v2::edge_cluster::{Direction, EdgeCluster},
    v2::node_record_v2::NodeRecordV2,
};
use tempfile::NamedTempFile;

/// Helper to create a test graph file with V2 nodes
fn create_v2_graph_file() -> (GraphFile, NamedTempFile) {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();
    let mut graph_file = GraphFile::create(path).unwrap();

    // Create two V2 nodes
    let mut node_store = NodeStore::new(&mut graph_file);

    let node1_v2 = NodeRecordV2::new(
        1,
        "Function".to_string(),
        "func1".to_string(),
        serde_json::json!({"test": "data1"}),
    );
    let node2_v2 = NodeRecordV2::new(
        2,
        "Function".to_string(),
        "func2".to_string(),
        serde_json::json!({"test": "data2"}),
    );

    node_store.write_node_v2(&node1_v2).unwrap();
    node_store.write_node_v2(&node2_v2).unwrap();

    (graph_file, temp_file)
}

/// Test 1: Verify V2 edge cluster length matches serialized bytes
/// This reproduces the bug where cluster metadata doesn't match actual serialized data
#[test]
fn test_v2_edge_cluster_length_matches_serialized_bytes() {
    let (mut graph_file, _temp_file) = create_v2_graph_file();

    // Create an edge
    let edge = EdgeRecord::new(
        1, // edge_id
        1, // from_id
        2, // to_id
        "calls".to_string(),
        serde_json::json!({"weight": 1.5}),
    );

    // Write edge using V2 clustered adjacency
    let mut edge_store = EdgeStore::new(&mut graph_file);
    edge_store.write_edge(&edge).unwrap();

    // Read back the source node to get cluster metadata
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(1).unwrap();

    println!("AFTER EDGE WRITE - Node 1 cluster metadata:");
    println!(
        "  outgoing_cluster_offset: {}",
        source_node.outgoing_cluster_offset
    );
    println!(
        "  outgoing_cluster_size: {}",
        source_node.outgoing_cluster_size
    );
    println!("  outgoing_edge_count: {}", source_node.outgoing_edge_count);

    // Verify cluster metadata is reasonable
    assert!(
        source_node.outgoing_cluster_offset > 0,
        "Cluster offset should be > 0 after edge write"
    );
    assert!(
        source_node.outgoing_cluster_size > 0,
        "Cluster size should be > 0 after edge write"
    );
    assert!(
        source_node.outgoing_edge_count > 0,
        "Edge count should be > 0 after edge write"
    );

    // Read the cluster directly and verify its size matches metadata
    if source_node.outgoing_cluster_offset > 0 && source_node.outgoing_cluster_size > 0 {
        let cluster_data = {
            let mut buffer = vec![0u8; source_node.outgoing_cluster_size as usize];
            graph_file
                .mmap_read_bytes(source_node.outgoing_cluster_offset, &mut buffer)
                .unwrap();
            buffer
        };

        println!("Actual cluster data size: {} bytes", cluster_data.len());
        println!(
            "Expected cluster size: {} bytes",
            source_node.outgoing_cluster_size
        );

        assert_eq!(
            cluster_data.len() as u32,
            source_node.outgoing_cluster_size,
            "Cluster data size must match node metadata"
        );

        // Try to deserialize the cluster
        let cluster_result = EdgeCluster::deserialize(&cluster_data);
        assert!(
            cluster_result.is_ok(),
            "Cluster should deserialize successfully: {:?}",
            cluster_result.err()
        );

        let cluster = cluster_result.unwrap();
        println!("Deserialized cluster edge count: {}", cluster.edge_count());
        println!(
            "Deserialized cluster total size: {} bytes",
            cluster.size_bytes()
        );
    }
}

/// Test 2: Verify V2 edge cluster offsets are respected
/// This reproduces the bug where cluster offsets point to wrong data
#[test]
fn test_v2_edge_cluster_offsets_are_respected() {
    let (mut graph_file, _temp_file) = create_v2_graph_file();

    // Create multiple edges to test offset calculations
    let edges = vec![
        EdgeRecord::new(1, 1, 2, "edge1".to_string(), serde_json::json!({"id": 1})),
        EdgeRecord::new(2, 1, 2, "edge2".to_string(), serde_json::json!({"id": 2})),
    ];

    let mut edge_store = EdgeStore::new(&mut graph_file);

    // Write edges - first edge creates cluster, second should update it
    for edge in &edges {
        edge_store.write_edge(edge).unwrap();
    }

    // Read back source node
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(1).unwrap();

    println!("Cluster metadata after multiple edges:");
    println!("  offset: {}", source_node.outgoing_cluster_offset);
    println!("  size: {}", source_node.outgoing_cluster_size);
    println!("  edge_count: {}", source_node.outgoing_edge_count);

    // Verify that cluster offset points to valid data
    if source_node.outgoing_cluster_offset > 0 {
        // Read cluster data from the specified offset
        let cluster_data = {
            let mut buffer = vec![0u8; source_node.outgoing_cluster_size as usize];
            let result =
                graph_file.mmap_read_bytes(source_node.outgoing_cluster_offset, &mut buffer);
            assert!(
                result.is_ok(),
                "Should be able to read cluster data from offset {}: {:?}",
                source_node.outgoing_cluster_offset,
                result.err()
            );
            buffer
        };

        // Verify the cluster contains the right number of edges
        let cluster = EdgeCluster::deserialize(&cluster_data).unwrap();
        assert_eq!(
            cluster.edge_count(),
            source_node.outgoing_edge_count,
            "Cluster edge count must match node metadata"
        );

        println!(
            "✅ Cluster offset is valid and contains {} edges",
            cluster.edge_count()
        );
    }
}

/// Test 3: Verify V2 edge boundary roundtrip neighbors are correct
/// This reproduces the main bug where neighbors() returns empty list despite successful edge write
#[test]
fn test_v2_edge_boundary_roundtrip_neighbors_correct() {
    let (mut graph_file, _temp_file) = create_v2_graph_file();

    // Create and write an edge
    let edge = EdgeRecord::new(
        1, // edge_id
        1, // from_id
        2, // to_id
        "calls".to_string(),
        serde_json::json!({"test": "data"}),
    );

    println!(
        "BEFORE: Writing edge from {} to {}",
        edge.from_id, edge.to_id
    );

    let mut edge_store = EdgeStore::new(&mut graph_file);
    edge_store.write_edge(&edge).unwrap();

    println!("AFTER: Edge written successfully");

    // Verify node was updated with cluster metadata
    let source_node = {
        let mut node_store = NodeStore::new(&mut graph_file);
        node_store.read_node_v2(1).unwrap()
    };

    println!("Source node cluster metadata:");
    println!(
        "  outgoing_cluster_offset: {}",
        source_node.outgoing_cluster_offset
    );
    println!(
        "  outgoing_cluster_size: {}",
        source_node.outgoing_cluster_size
    );
    println!("  outgoing_edge_count: {}", source_node.outgoing_edge_count);

    // Use adjacency iterator to get neighbors (this is what the failing test does)
    let neighbors = {
        use sqlitegraph::backend::native::adjacency::AdjacencyHelpers;
        AdjacencyHelpers::get_outgoing_neighbors(&mut graph_file, 1).unwrap()
    };

    println!("Neighbors returned by adjacency system: {:?}", neighbors);

    assert!(
        neighbors.contains(&2),
        "Roundtrip neighbor lookup should return node 2. Got neighbors: {:?}",
        neighbors
    );

    // Additional verification: try reading neighbors directly from cluster
    if source_node.outgoing_cluster_offset > 0 {
        // First, examine the raw cluster data
        let raw_cluster_data = {
            let mut buffer = vec![0u8; source_node.outgoing_cluster_size as usize];
            graph_file
                .mmap_read_bytes(source_node.outgoing_cluster_offset, &mut buffer)
                .unwrap();
            buffer
        };

        println!("Raw cluster data: {:02x?}", raw_cluster_data);

        // Try to deserialize the cluster
        let cluster_result = EdgeCluster::deserialize(&raw_cluster_data);
        match cluster_result {
            Ok(cluster) => {
                println!("✅ Cluster deserialized successfully");
                println!("  Cluster edge count: {}", cluster.edge_count());
                println!("  Cluster total size: {} bytes", cluster.size_bytes());
                println!("  Cluster edges: {:?}", cluster.edges());

                let direct_neighbors = {
                    let mut edge_store = EdgeStore::new(&mut graph_file);
                    edge_store
                        .get_clustered_neighbors(
                            source_node.outgoing_cluster_offset,
                            source_node.outgoing_cluster_size,
                            Direction::Outgoing,
                            1,
                        )
                        .unwrap()
                };

                println!("Direct cluster neighbors: {:?}", direct_neighbors);

                assert!(
                    direct_neighbors.contains(&2),
                    "Direct cluster neighbor lookup should return node 2. Got: {:?}",
                    direct_neighbors
                );
            }
            Err(e) => {
                println!("❌ Cluster deserialization failed: {:?}", e);
                panic!("Cluster should deserialize successfully");
            }
        }
    }
}
