//! Phase 38: GraphFile I/O Layer Tests
//!
//! Test-driven development for the GraphFile read_bytes/write_bytes corruption bug.
//! These tests must fail BEFORE implementation and PASS AFTER the fix.

#[cfg(feature = "v2_experimental")]
use sqlitegraph::{
    BackendDirection, EdgeSpec, GraphBackend, NeighborQuery, NodeSpec,
    backend::native::{EdgeStore, GraphFile, NodeStore},
    config::GraphConfig,
    open_graph,
};
use std::path::Path;
use tempfile::TempDir;

/// Test 1: Write then read exact bytes roundtrip
/// This tests the fundamental GraphFile I/O that's failing in cluster tests
#[cfg(feature = "v2_experimental")]
#[test]
fn test_write_then_read_exact_bytes_roundtrip() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create graph file
    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Write test data at specific offset
    let test_data = b"CLUSTER_HEADER\x01\x00\x00\x00\x0C";
    let write_offset = 2048; // Safe offset after header

    graph_file
        .write_bytes(write_offset, test_data)
        .expect("Failed to write bytes");
    graph_file.flush().expect("Failed to flush");

    // Read back the exact bytes
    let mut read_buffer = vec![0u8; test_data.len()];
    graph_file
        .read_bytes(write_offset, &mut read_buffer)
        .expect("Failed to read bytes");

    // Verify exact match
    assert_eq!(
        read_buffer, test_data,
        "Read bytes must match written bytes exactly"
    );
}

/// Test 2: mmap region reads after multiple writes
/// Tests mmap corruption when multiple writes occur
#[cfg(feature = "v2_experimental")]
#[test]
fn test_mmap_region_reads_after_multiple_writes() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create graph file
    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Write multiple cluster data blocks
    let cluster1 = b"\x00\x00\x00\x01\x00\x00\x00\x0C\x01\x02\x03\x04"; // First cluster
    let cluster2 = b"\x00\x00\x00\x02\x00\x00\x00\x10\x05\x06\x07\x08\x09\x0A\x0B\x0C"; // Second cluster

    let offset1 = 2048;
    let offset2 = offset1 + cluster1.len() as u64;

    // Write both clusters
    graph_file
        .write_bytes(offset1, cluster1)
        .expect("Failed to write cluster 1");
    graph_file
        .write_bytes(offset2, cluster2)
        .expect("Failed to write cluster 2");
    graph_file.flush().expect("Failed to flush");

    // Test mmap reads (this is what node_store.rs uses)
    let mut read_buffer1 = vec![0u8; cluster1.len()];
    let mut read_buffer2 = vec![0u8; cluster2.len()];

    // These should pass after the fix
    graph_file
        .mmap_read_bytes(offset1, &mut read_buffer1)
        .expect("Failed to mmap read cluster 1");
    graph_file
        .mmap_read_bytes(offset2, &mut read_buffer2)
        .expect("Failed to mmap read cluster 2");

    assert_eq!(
        read_buffer1, cluster1,
        "mmap read 1 must match written data"
    );
    assert_eq!(
        read_buffer2, cluster2,
        "mmap read 2 must match written data"
    );
}

/// Test 3: Cluster bytes persist after flush
/// Tests that cluster data persists after explicit flush
#[cfg(feature = "v2_experimental")]
#[test]
fn test_cluster_bytes_persist_after_flush() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create and setup graph using real API
    let config = GraphConfig::native();
    let mut graph = open_graph(&db_path, &config).expect("Failed to create graph");

    // Create nodes and edges that trigger cluster creation
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({"type": "source"}),
        })
        .expect("Failed to insert source node");

    let target_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({"type": "target"}),
        })
        .expect("Failed to insert target node");

    // Create edge (this triggers cluster creation)
    let _edge_id = graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target_id,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!({"weight": 1.0}),
        })
        .expect("Failed to insert edge");

    // Verify through direct GraphFile access that cluster data exists
    let mut graph_file = GraphFile::open(&db_path).expect("Failed to reopen graph file");

    // Read the cluster metadata (this should work after our fix)
    let cluster_offset = graph_file.header().outgoing_cluster_offset;
    assert!(
        cluster_offset > 0,
        "Cluster should be written after edge creation"
    );

    // Read the raw cluster bytes
    let cluster_size = 20; // Known size for single edge cluster
    let mut cluster_bytes = vec![0u8; cluster_size];

    // This test demonstrates the exact corruption we need to fix
    graph_file
        .read_bytes(cluster_offset, &mut cluster_bytes)
        .expect("Failed to read cluster bytes");

    // Verify header is not all zeros (this will fail before fix)
    assert_ne!(
        cluster_bytes[0], 0,
        "Cluster header edge_count should not be zero"
    );
    assert_ne!(
        cluster_bytes[4], 0,
        "Cluster header payload_size should not be zero"
    );

    // Verify actual cluster content
    let expected_edge_count = 1u32.to_be_bytes();
    let expected_payload_size = 12u32.to_be_bytes();

    assert_eq!(
        &cluster_bytes[0..4],
        &expected_edge_count,
        "Edge count should match"
    );
    assert_eq!(
        &cluster_bytes[4..8],
        &expected_payload_size,
        "Payload size should match"
    );
}

/// Test 4: Cluster bytes persist after reopen
/// Tests that cluster data survives file close/reopen
#[cfg(feature = "v2_experimental")]
#[test]
fn test_cluster_bytes_persist_after_reopen() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create graph and cluster data
    {
        let config = GraphConfig::native();
        let mut graph = open_graph(&db_path, &config).expect("Failed to create graph");

        let source_id = graph
            .insert_node(NodeSpec {
                kind: "Node".to_string(),
                name: "source".to_string(),
                file_path: None,
                data: serde_json::json!({"type": "source"}),
            })
            .expect("Failed to insert source node");

        let target_id = graph
            .insert_node(NodeSpec {
                kind: "Node".to_string(),
                name: "target".to_string(),
                file_path: None,
                data: serde_json::json!({"type": "target"}),
            })
            .expect("Failed to insert target node");

        let _edge_id = graph
            .insert_edge(EdgeSpec {
                from: source_id,
                to: target_id,
                edge_type: "test_edge".to_string(),
                data: serde_json::json!({"weight": 1.0}),
            })
            .expect("Failed to insert edge");

        // Explicitly close graph to flush everything
        drop(graph);
    }

    // Reopen and verify cluster persists
    let mut graph_file = GraphFile::open(&db_path).expect("Failed to reopen graph file");
    let cluster_offset = graph_file.header().outgoing_cluster_offset;

    assert!(
        cluster_offset > 0,
        "Cluster offset should persist after reopen"
    );

    // Verify cluster data is still valid
    let cluster_size = 20;
    let mut cluster_bytes = vec![0u8; cluster_size];
    graph_file
        .read_bytes(cluster_offset, &mut cluster_bytes)
        .expect("Failed to read cluster after reopen");

    // Header should still be valid after reopen
    assert_ne!(
        cluster_bytes[0], 0,
        "Cluster header should persist after reopen"
    );
    assert_ne!(
        cluster_bytes[4], 0,
        "Cluster payload size should persist after reopen"
    );
}

/// Test 5: Partial write then read range
/// Tests partial writes and range reads
#[cfg(feature = "v2_experimental")]
#[test]
fn test_partial_write_then_read_range() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Write partial cluster data in multiple operations
    let base_offset = 4096;
    let header = b"\x00\x00\x00\x02\x00\x00\x00\x20"; // edge_count=2, payload_size=32
    let payload1 = b"\x00\x00\x00\x00\x00\x00\x00\x01"; // edge 1
    let payload2 = b"\x00\x00\x00\x00\x00\x00\x00\x02"; // edge 2

    // Write in stages
    graph_file
        .write_bytes(base_offset, header)
        .expect("Failed to write header");
    graph_file
        .write_bytes(base_offset + 8, payload1)
        .expect("Failed to write payload1");
    graph_file
        .write_bytes(base_offset + 16, payload2)
        .expect("Failed to write payload2");
    graph_file.flush().expect("Failed to flush");

    // Read the full cluster
    let total_size = header.len() + payload1.len() + payload2.len();
    let mut read_buffer = vec![0u8; total_size];
    graph_file
        .read_bytes(base_offset, &mut read_buffer)
        .expect("Failed to read cluster");

    // Verify all parts match
    assert_eq!(&read_buffer[..8], header, "Header should match");
    assert_eq!(&read_buffer[8..16], payload1, "Payload1 should match");
    assert_eq!(&read_buffer[16..], payload2, "Payload2 should match");
}

/// Test 6: Flush required for visibility
/// Tests that flush is required for write visibility
#[cfg(feature = "v2_experimental")]
#[test]
fn test_flush_required_for_visibility() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    let test_data = b"FLUSH_TEST_DATA";
    let offset = 8192;

    // Write without flush first
    graph_file
        .write_bytes(offset, test_data)
        .expect("Failed to write without flush");

    // Read without flush (this may read stale data)
    let mut read_buffer = vec![0u8; test_data.len()];
    let read_result = graph_file.read_bytes(offset, &mut read_buffer);

    // Before the fix: reading without flush may return zeros
    // After the fix: should read correctly regardless of flush
    assert!(read_result.is_ok(), "Read should not fail");

    // Explicit flush
    graph_file.flush().expect("Failed to flush");

    // Read after flush (must succeed)
    let mut final_buffer = vec![0u8; test_data.len()];
    graph_file
        .read_bytes(offset, &mut final_buffer)
        .expect("Failed to read after flush");

    assert_eq!(final_buffer, test_data, "Data must be readable after flush");
}
