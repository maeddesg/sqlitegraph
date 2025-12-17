//! Phase 41: Mixed I/O Corruption Isolation Tests
//!
//! TDD tests to isolate the root cause of byte-swap and magic number corruption.
//! These tests reproduce specific corruption patterns to prove mixed I/O is the culprit.

#[cfg(feature = "v2_experimental")]
use sqlitegraph::{
    EdgeSpec, NodeSpec,
    backend::native::v2,
    backend::native::{EdgeStore, GraphFile, NodeStore},
    config::GraphConfig,
    open_graph,
};
use tempfile::TempDir;

/// TEST 1: Magic number should be stable across reopen after multi-cluster writes
/// This test proves that the magic number corruption happens during file reopen cycles
#[cfg(feature = "v2_experimental")]
#[test]
fn test_magic_number_stable_across_reopen_after_multi_cluster_writes() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create graph and write multiple clusters
    {
        let config = GraphConfig::native();
        let mut graph = open_graph(&db_path, &config).expect("Failed to create graph");

        // Create multiple nodes to trigger multiple clusters
        let node1_id = graph
            .insert_node(NodeSpec {
                kind: "Node".to_string(),
                name: "node1".to_string(),
                file_path: None,
                data: serde_json::json!({"id": 1}),
            })
            .expect("Failed to insert node1");

        let node2_id = graph
            .insert_node(NodeSpec {
                kind: "Node".to_string(),
                name: "node2".to_string(),
                file_path: None,
                data: serde_json::json!({"id": 2}),
            })
            .expect("Failed to insert node2");

        let node3_id = graph
            .insert_node(NodeSpec {
                kind: "Node".to_string(),
                name: "node3".to_string(),
                file_path: None,
                data: serde_json::json!({"id": 3}),
            })
            .expect("Failed to insert node3");

        // Create multiple edges to trigger multiple clusters
        let _edge1 = graph
            .insert_edge(EdgeSpec {
                from: node1_id,
                to: node2_id,
                edge_type: "test".to_string(),
                data: serde_json::json!({"weight": 1.0}),
            })
            .expect("Failed to insert edge 1");

        let _edge2 = graph
            .insert_edge(EdgeSpec {
                from: node1_id,
                to: node3_id,
                edge_type: "test".to_string(),
                data: serde_json::json!({"weight": 2.0}),
            })
            .expect("Failed to insert edge 2");

        let _edge3 = graph
            .insert_edge(EdgeSpec {
                from: node2_id,
                to: node3_id,
                edge_type: "bidirectional".to_string(),
                data: serde_json::json!({"weight": 1.5}),
            })
            .expect("Failed to insert edge 3");

        // Explicit close to flush all data
        drop(graph);
    }

    // Reopen and verify magic number stability
    {
        let graph_file_result = GraphFile::open(&db_path);
        match graph_file_result {
            Ok(graph_file) => {
                let header = graph_file.header();

                // Print the actual magic bytes for debugging
                println!(
                    "Magic bytes after multi-cluster writes: {:02X?}",
                    header.magic
                );
                println!("Expected magic bytes: {:02X?}", v2::V2_MAGIC);

                assert_eq!(
                    header.magic,
                    v2::V2_MAGIC,
                    "Magic number should remain stable after multi-cluster writes"
                );
            }
            Err(e) => {
                panic!("Failed to reopen after multi-cluster writes: {:?}", e);
            }
        }
    }
}

/// TEST 2: Cluster header should not be byte-swapped after two cluster writes in same run
/// This test proves that cluster headers get corrupted during multi-cluster operations
#[cfg(feature = "v2_experimental")]
#[test]
fn test_cluster_header_not_byte_swapped_after_two_cluster_writes_same_run() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).expect("Failed to create graph");

    // Create nodes for bidirectional cluster test
    let node1_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!({"id": 1}),
        })
        .expect("Failed to insert node1");

    let node2_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "node2".to_string(),
            file_path: None,
            data: serde_json::json!({"id": 2}),
        })
        .expect("Failed to insert node2");

    let node3_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "node3".to_string(),
            file_path: None,
            data: serde_json::json!({"id": 3}),
        })
        .expect("Failed to insert node3");

    // Create outgoing cluster from node1 (should create 2 edges)
    let _edge1 = graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: "outgoing".to_string(),
            data: serde_json::json!({"weight": 1.0}),
        })
        .expect("Failed to insert outgoing edge 1");

    let _edge2 = graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node3_id,
            edge_type: "outgoing".to_string(),
            data: serde_json::json!({"weight": 2.0}),
        })
        .expect("Failed to insert outgoing edge 2");

    // Create incoming cluster to node2 (should create 1 edge)
    let _edge3 = graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: "incoming_test".to_string(),
            data: serde_json::json!({"weight": 3.0}),
        })
        .expect("Failed to insert incoming edge");

    // Now read cluster headers directly to verify they're not corrupted
    let mut graph_file = GraphFile::open(&db_path).expect("Failed to open graph file");
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let mut node_store = NodeStore::new(&mut graph_file);

    // Get node1's V2 metadata to find cluster offsets
    let node1_v2 = node_store
        .read_node_v2(node1_id)
        .expect("Failed to read node1 V2 metadata");

    println!(
        "Node1 outgoing cluster: offset={}, size={}, count={}",
        node1_v2.outgoing_cluster_offset,
        node1_v2.outgoing_cluster_size,
        node1_v2.outgoing_edge_count
    );
    println!(
        "Node1 incoming cluster: offset={}, size={}, count={}",
        node1_v2.incoming_cluster_offset,
        node1_v2.incoming_cluster_size,
        node1_v2.incoming_edge_count
    );

    // Read outgoing cluster header if it exists
    if node1_v2.outgoing_cluster_offset > 0 && node1_v2.outgoing_cluster_size >= 8 {
        let mut cluster_header = vec![0u8; 8];
        graph_file
            .read_bytes(node1_v2.outgoing_cluster_offset, &mut cluster_header)
            .expect("Failed to read cluster header");

        let edge_count = u32::from_be_bytes([
            cluster_header[0],
            cluster_header[1],
            cluster_header[2],
            cluster_header[3],
        ]);
        let payload_size = u32::from_be_bytes([
            cluster_header[4],
            cluster_header[5],
            cluster_header[6],
            cluster_header[7],
        ]);

        println!("Outgoing cluster header bytes: {:02X?}", cluster_header);
        println!(
            "Outgoing cluster parsed: edge_count={}, payload_size={}",
            edge_count, payload_size
        );

        // Assert no byte-swapping corruption
        assert_ne!(
            edge_count, 33554432,
            "Edge count should not be byte-swapped (33554432 = 0x02000000)"
        );
        assert_ne!(
            edge_count, 0,
            "Edge count should not be zero after successful cluster write"
        );
        assert_eq!(edge_count, 2, "Should have exactly 2 outgoing edges");
        assert!(payload_size > 0, "Payload size should be positive");
    }

    // Read incoming cluster header if it exists
    if node1_v2.incoming_cluster_offset > 0 && node1_v2.incoming_cluster_size >= 8 {
        let mut cluster_header = vec![0u8; 8];
        graph_file
            .read_bytes(node1_v2.incoming_cluster_offset, &mut cluster_header)
            .expect("Failed to read cluster header");

        let edge_count = u32::from_be_bytes([
            cluster_header[0],
            cluster_header[1],
            cluster_header[2],
            cluster_header[3],
        ]);
        let payload_size = u32::from_be_bytes([
            cluster_header[4],
            cluster_header[5],
            cluster_header[6],
            cluster_header[7],
        ]);

        println!("Incoming cluster header bytes: {:02X?}", cluster_header);
        println!(
            "Incoming cluster parsed: edge_count={}, payload_size={}",
            edge_count, payload_size
        );

        // Assert no byte-swapping corruption
        assert_ne!(
            edge_count, 33554432,
            "Edge count should not be byte-swapped"
        );
        assert_ne!(
            edge_count, 0,
            "Edge count should not be zero after successful cluster write"
        );
        assert_eq!(edge_count, 1, "Should have exactly 1 incoming edge");
        assert!(payload_size > 0, "Payload size should be positive");
    }
}

/// TEST 3: Cluster header should not be zeroed after reopen
/// This test proves that cluster headers get zeroed out during file reopen cycles
#[cfg(feature = "v2_experimental")]
#[test]
fn test_no_zeroed_cluster_header_after_reopen() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Write a single cluster and immediately verify it's correct
    {
        let config = GraphConfig::native();
        let mut graph = open_graph(&db_path, &config).expect("Failed to create graph");

        let node1_id = graph
            .insert_node(NodeSpec {
                kind: "Node".to_string(),
                name: "node1".to_string(),
                file_path: None,
                data: serde_json::json!({"id": 1}),
            })
            .expect("Failed to insert node1");

        let node2_id = graph
            .insert_node(NodeSpec {
                kind: "Node".to_string(),
                name: "node2".to_string(),
                file_path: None,
                data: serde_json::json!({"id": 2}),
            })
            .expect("Failed to insert node2");

        // Create single edge to trigger cluster creation
        let _edge = graph
            .insert_edge(EdgeSpec {
                from: node1_id,
                to: node2_id,
                edge_type: "single_edge".to_string(),
                data: serde_json::json!({"weight": 1.0}),
            })
            .expect("Failed to insert edge");

        // Read cluster header immediately while still in same session
        let mut graph_file = GraphFile::open(&db_path).expect("Failed to open graph file");
        let mut node_store = NodeStore::new(&mut graph_file);
        let node1_v2 = node_store
            .read_node_v2(node1_id)
            .expect("Failed to read node1 V2 metadata");

        if node1_v2.outgoing_cluster_offset > 0 && node1_v2.outgoing_cluster_size >= 8 {
            let mut cluster_header = vec![0u8; 8];
            graph_file
                .read_bytes(node1_v2.outgoing_cluster_offset, &mut cluster_header)
                .expect("Failed to read cluster header");

            println!("Cluster header before close: {:02X?}", cluster_header);

            // Verify cluster header is not all zeros
            let is_all_zeros = cluster_header.iter().all(|&b| b == 0);
            assert!(
                !is_all_zeros,
                "Cluster header should not be all zeros immediately after write"
            );
        }

        // Explicit close
        drop(graph);
    }

    // Reopen and verify cluster header persists
    {
        let graph_file_result = GraphFile::open(&db_path);
        match graph_file_result {
            Ok(mut graph_file) => {
                let mut node_store = NodeStore::new(&mut graph_file);
                let node1_v2 = node_store
                    .read_node_v2(1)
                    .expect("Failed to read node1 V2 metadata after reopen");

                if node1_v2.outgoing_cluster_offset > 0 && node1_v2.outgoing_cluster_size >= 8 {
                    let mut cluster_header = vec![0u8; 8];
                    graph_file
                        .read_bytes(node1_v2.outgoing_cluster_offset, &mut cluster_header)
                        .expect("Failed to read cluster header after reopen");

                    println!("Cluster header after reopen: {:02X?}", cluster_header);

                    // Verify cluster header persists
                    let edge_count = u32::from_be_bytes([
                        cluster_header[0],
                        cluster_header[1],
                        cluster_header[2],
                        cluster_header[3],
                    ]);
                    let payload_size = u32::from_be_bytes([
                        cluster_header[4],
                        cluster_header[5],
                        cluster_header[6],
                        cluster_header[7],
                    ]);

                    assert!(
                        !cluster_header.iter().all(|&b| b == 0),
                        "Cluster header should not be all zeros after reopen"
                    );
                    assert_ne!(edge_count, 0, "Edge count should persist after reopen");
                    assert_ne!(payload_size, 0, "Payload size should persist after reopen");
                } else {
                    panic!("Cluster metadata lost after reopen");
                }
            }
            Err(e) => {
                panic!("Failed to reopen graph file: {:?}", e);
            }
        }
    }
}
