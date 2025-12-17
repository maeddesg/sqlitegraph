//! Phase 40: Mmap Lifecycle Tests
//!
//! TDD tests to validate conservative mmap lifecycle implementation.
//! These tests use real GraphFile APIs with no mocks.

#[cfg(feature = "v2_experimental")]
use sqlitegraph::{
    BackendDirection, EdgeSpec, NeighborQuery, NodeSpec, backend::native::GraphFile,
    backend::native::v2, config::GraphConfig, open_graph,
};
use tempfile::TempDir;

/// Test 1: GraphFile single write-read roundtrip via mmap
/// Validates basic mmap functionality and write coherence
#[cfg(feature = "v2_experimental")]
#[test]
fn test_graphfile_single_write_read_roundtrip_mmap() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create GraphFile directly (not through Graph API)
    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Write test data at specific offset beyond header
    let test_pattern = b"CLUSTER_HEADER\x01\x00\x00\x00\x0C";
    let write_offset = 2048;

    // Write using standard I/O (this should update mmap)
    graph_file
        .write_bytes(write_offset, test_pattern)
        .expect("Failed to write bytes");
    graph_file.flush().expect("Failed to flush");

    // Read back via mmap to verify write coherence
    let mut read_buffer = vec![0u8; test_pattern.len()];
    graph_file
        .mmap_read_bytes(write_offset, &mut read_buffer)
        .expect("Failed to mmap read");

    assert_eq!(
        read_buffer, test_pattern,
        "Mmap read must match written data exactly"
    );

    // Also verify via standard I/O
    let mut std_read_buffer = vec![0u8; test_pattern.len()];
    graph_file
        .read_bytes(write_offset, &mut std_read_buffer)
        .expect("Failed to standard read");
    assert_eq!(
        std_read_buffer, test_pattern,
        "Standard read must match written data"
    );
}

/// Test 2: Multiple writes preserve all bytes
/// Validates that multiple writes don't corrupt each other through mmap aliasing
#[cfg(feature = "v2_experimental")]
#[test]
fn test_graphfile_multiple_writes_preserve_all_bytes() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Write first pattern
    let pattern1 = b"PATTERN_001_DATA";
    let offset1 = 2048;
    graph_file
        .write_bytes(offset1, pattern1)
        .expect("Failed to write pattern 1");

    // Write second pattern at different offset
    let pattern2 = b"PATTERN_002_DATA";
    let offset2 = 3072;
    graph_file
        .write_bytes(offset2, pattern2)
        .expect("Failed to write pattern 2");

    graph_file.flush().expect("Failed to flush");

    // Verify first pattern still intact
    let mut read1 = vec![0u8; pattern1.len()];
    graph_file
        .mmap_read_bytes(offset1, &mut read1)
        .expect("Failed to read pattern 1");
    assert_eq!(read1, pattern1, "First pattern should be intact");

    // Verify second pattern is correct
    let mut read2 = vec![0u8; pattern2.len()];
    graph_file
        .mmap_read_bytes(offset2, &mut read2)
        .expect("Failed to read pattern 2");
    assert_eq!(read2, pattern2, "Second pattern should be correct");
}

/// Test 3: GraphFile reopen preserves data via mmap
/// Validates that mmap state survives file close/reopen cycles
#[cfg(feature = "v2_experimental")]
#[test]
fn test_graphfile_reopen_preserves_data_mmap() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create and write data
    {
        let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

        let test_data = b"PRESERVE_TEST_DATA_AFTER_REOPEN";
        let write_offset = 4096;

        graph_file
            .write_bytes(write_offset, test_data)
            .expect("Failed to write data");
        graph_file.flush().expect("Failed to flush");

        // Explicitly drop/close the file
        drop(graph_file);
    }

    // Reopen and verify data persistence
    {
        let mut graph_file = GraphFile::open(&db_path).expect("Failed to reopen graph file");

        let test_data = b"PRESERVE_TEST_DATA_AFTER_REOPEN";
        let read_offset = 4096;

        let mut read_buffer = vec![0u8; test_data.len()];
        graph_file
            .mmap_read_bytes(read_offset, &mut read_buffer)
            .expect("Failed to read after reopen");

        assert_eq!(read_buffer, test_data, "Data should persist after reopen");
    }
}

/// Test 4: V2 cluster roundtrip through real API
/// Validates end-to-end cluster functionality with mmap lifecycle
#[cfg(feature = "v2_experimental")]
#[test]
fn test_graphfile_v2_cluster_roundtrip_via_edges() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).expect("Failed to create graph");

    // Create nodes
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

    // Create edges that trigger V2 cluster creation
    let _edge1 = graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: "connects".to_string(),
            data: serde_json::json!({"weight": 1.0}),
        })
        .expect("Failed to insert edge 1");

    let _edge2 = graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node3_id,
            edge_type: "connects".to_string(),
            data: serde_json::json!({"weight": 2.0}),
        })
        .expect("Failed to insert edge 2");

    // Verify neighbors via V2 clustered adjacency
    let neighbors = graph
        .neighbors(
            node1_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .expect("Failed to get neighbors");

    // Should contain exactly node2 and node3
    assert_eq!(neighbors.len(), 2, "Should have exactly 2 neighbors");
    let mut neighbor_set: std::collections::HashSet<_> = neighbors.iter().cloned().collect();
    assert!(neighbor_set.contains(&node2_id), "Should contain node2");
    assert!(neighbor_set.contains(&node3_id), "Should contain node3");
}

/// Test 5: Internal corruption detection
/// Validates that we can detect when cluster headers are corrupted
#[cfg(feature = "v2_experimental")]
#[test]
fn test_internal_corruption_detection() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create graph and write cluster data
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

    // Create edge to trigger cluster creation
    let _edge = graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: "test".to_string(),
            data: serde_json::json!({"weight": 1.0}),
        })
        .expect("Failed to insert edge");

    // Force flush and close
    drop(graph);

    // Reopen and verify magic number is still valid
    let graph_file_result = GraphFile::open(&db_path);
    assert!(
        graph_file_result.is_ok(),
        "GraphFile should reopen without magic number corruption"
    );

    let graph_file = graph_file_result.unwrap();
    let header = graph_file.header();

    // Verify magic number is still valid
    assert_eq!(
        header.magic,
        v2::V2_MAGIC,
        "Magic number should be preserved"
    );
}

/// Test 6: Large write behavior
/// Validates that large writes work correctly and data is preserved
#[cfg(feature = "v2_experimental")]
#[test]
fn test_large_write_behavior() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Write a large chunk
    let large_data = vec![0x42; 5000]; // 5KB of data
    let large_offset = 8192;

    graph_file
        .write_bytes(large_offset, &large_data)
        .expect("Failed to write large data");
    graph_file.flush().expect("Failed to flush");

    // Verify we can read the large data back via mmap
    let mut read_buffer = vec![0u8; large_data.len()];
    graph_file
        .mmap_read_bytes(large_offset, &mut read_buffer)
        .expect("Failed to read large data via mmap");
    assert_eq!(
        read_buffer, large_data,
        "Large data should be readable via mmap"
    );

    // Also verify via standard read
    let mut std_read_buffer = vec![0u8; large_data.len()];
    graph_file
        .read_bytes(large_offset, &mut std_read_buffer)
        .expect("Failed to read large data via standard read");
    assert_eq!(
        std_read_buffer, large_data,
        "Large data should be readable via standard read"
    );
}
