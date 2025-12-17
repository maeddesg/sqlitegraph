//! Phase 34: V2 Cluster Pipeline TDD Tests
//!
//! Test-driven development suite for the new V2 cluster architecture.
//! These tests validate the EdgeCluster-as-single-source-of-truth pipeline.
//!
//! Key validation points:
//! • No EdgeRecord reconstruction from CompactEdgeRecord
//! • Original edge_type and edge_data preservation
//! • Direct compact edge accumulation
//! • StringTable integration consistency
//! • Cluster serialization roundtrip correctness

use sqlitegraph::{
    EdgeSpec, NodeSpec,
    backend::native::types::FileOffset,
    backend::native::v2::edge_cluster::{CompactEdgeRecord, Direction, EdgeCluster},
    backend::native::v2::node_record_v2::NodeRecordV2,
    backend::native::v2::string_table::StringTable,
    backend::native::{EdgeRecord, EdgeStore, GraphFile, NodeStore},
    config::GraphConfig,
    open_graph,
};
use tempfile::TempDir;

/// Test 1: Single edge cluster creation preserves all data
/// Verify that CompactEdgeRecord::from_edge_record preserves original edge_type and edge_data
#[test]
fn test_single_edge_cluster_data_preservation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir
        .path()
        .join("test_single_edge_data_preservation.db");

    // Create test edge with rich data
    let original_edge = EdgeRecord::new(
        1,                               // edge_id
        10,                              // from_id
        20,                              // to_id
        "complex_edge_type".to_string(), // edge_type
        serde_json::json!({                 // edge_data
            "weight": 3.14159,
            "metadata": {
                "source": "test_case_1",
                "tags": ["important", "critical"],
                "timestamp": "2024-01-15T10:30:00Z"
            },
            "properties": {
                "is_bidirectional": true,
                "priority": "high",
                "confidence": 0.95
            }
        }),
    );

    // Test CompactEdgeRecord creation preserves data
    let mut string_table = StringTable::new();
    let compact_record =
        CompactEdgeRecord::from_edge_record(&original_edge, Direction::Outgoing, &mut string_table)
            .expect("Failed to create compact record from edge");

    // Verify neighbor ID preservation
    assert_eq!(
        compact_record.neighbor_id, 20,
        "Compact record should preserve to_id as neighbor for outgoing"
    );

    // Verify edge_type preservation via string table
    let resolved_type = string_table
        .get_string(compact_record.edge_type_offset)
        .expect("Failed to resolve edge type from string table");
    assert_eq!(
        resolved_type, "complex_edge_type",
        "String table should preserve original edge type"
    );

    // Verify edge_data preservation
    let reconstructed_data: serde_json::Value =
        serde_json::from_slice(&compact_record.edge_data).expect("Failed to deserialize edge data");
    assert_eq!(
        reconstructed_data, original_edge.data,
        "Compact record should preserve original edge data exactly"
    );

    // Test cluster creation from compact edges
    let cluster = EdgeCluster::create_from_compact_edges(
        vec![compact_record],
        10, // source node ID
        Direction::Outgoing,
    )
    .expect("Failed to create cluster from compact edges");

    // Verify cluster properties
    assert_eq!(
        cluster.edge_count(),
        1,
        "Cluster should have exactly 1 edge"
    );
    assert!(
        cluster.size_bytes() > 8,
        "Cluster should be larger than header size"
    );

    // Test cluster serialization roundtrip
    let cluster_bytes = cluster.serialize();
    println!("DEBUG: cluster_bytes.len() = {}", cluster_bytes.len());
    println!("DEBUG: cluster.edge_count() = {}", cluster.edge_count());
    println!(
        "DEBUG: cluster.serialized_size = {}",
        cluster.payload_size()
    );
    println!("DEBUG: cluster.size_bytes() = {}", cluster.size_bytes());

    // Debug: Parse header manually
    if cluster_bytes.len() >= 8 {
        let edge_count = u32::from_be_bytes([
            cluster_bytes[0],
            cluster_bytes[1],
            cluster_bytes[2],
            cluster_bytes[3],
        ]);
        let payload_size = u32::from_be_bytes([
            cluster_bytes[4],
            cluster_bytes[5],
            cluster_bytes[6],
            cluster_bytes[7],
        ]);
        println!(
            "DEBUG: Header - edge_count={}, payload_size={}, total_bytes={}",
            edge_count,
            payload_size,
            cluster_bytes.len()
        );
        println!(
            "DEBUG: Expected total = 8 + {} = {}",
            payload_size,
            8 + payload_size
        );
        if cluster_bytes.len() != 8 + payload_size as usize {
            println!("ERROR: Size mismatch in header!");
        }
    }

    let deserialized_cluster =
        EdgeCluster::deserialize(&cluster_bytes).expect("Failed to deserialize cluster");

    // Verify roundtrip preservation
    assert_eq!(
        deserialized_cluster.edge_count(),
        cluster.edge_count(),
        "Edge count preserved across roundtrip"
    );
    assert_eq!(
        deserialized_cluster.edges().len(),
        cluster.edges().len(),
        "Edges count preserved across roundtrip"
    );

    let roundtrip_edge = &deserialized_cluster.edges()[0];
    assert_eq!(
        roundtrip_edge.neighbor_id, 20,
        "Neighbor ID preserved across roundtrip"
    );

    let roundtrip_type = string_table
        .get_string(roundtrip_edge.edge_type_offset)
        .expect("Failed to resolve roundtrip edge type");
    assert_eq!(
        roundtrip_type, "complex_edge_type",
        "Edge type preserved across roundtrip"
    );

    let roundtrip_data: serde_json::Value = serde_json::from_slice(&roundtrip_edge.edge_data)
        .expect("Failed to deserialize roundtrip edge data");
    assert_eq!(
        roundtrip_data, original_edge.data,
        "Edge data preserved across roundtrip"
    );
}

/// Test 2: Multi-edge cluster accumulation without data loss
/// Verify that adding edges to existing cluster preserves all original data
#[test]
fn test_multi_edge_cluster_accumulation_preservation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_multi_edge_accumulation.db");

    // Create multiple distinct edges
    let edges = vec![
        EdgeRecord::new(
            1,
            10,
            20,
            "calls".to_string(),
            serde_json::json!({"line": 42, "file": "main.rs"}),
        ),
        EdgeRecord::new(
            2,
            10,
            30,
            "imports".to_string(),
            serde_json::json!({"module": "std::collections", "alias": "collections"}),
        ),
        EdgeRecord::new(
            3,
            10,
            40,
            "defines".to_string(),
            serde_json::json!({"type": "struct", "name": "GraphData", "fields": ["nodes", "edges"]}),
        ),
    ];

    let mut string_table = StringTable::new();

    // Create compact records from edges
    let compact_records: Result<Vec<_>, _> = edges
        .iter()
        .map(|edge| {
            CompactEdgeRecord::from_edge_record(edge, Direction::Outgoing, &mut string_table)
        })
        .collect();
    let compact_records = compact_records.expect("Failed to create compact records");

    // Verify all data preserved in compact format
    for (i, (original, compact)) in edges.iter().zip(compact_records.iter()).enumerate() {
        let resolved_type = string_table
            .get_string(compact.edge_type_offset)
            .expect(&format!("Failed to resolve edge type for edge {}", i));
        assert_eq!(
            resolved_type, original.edge_type,
            "Edge type preserved for edge {}",
            i
        );

        let reconstructed_data: serde_json::Value = serde_json::from_slice(&compact.edge_data)
            .expect(&format!("Failed to reconstruct data for edge {}", i));
        assert_eq!(
            reconstructed_data, original.data,
            "Edge data preserved for edge {}",
            i
        );

        assert_eq!(
            compact.neighbor_id, original.to_id,
            "Neighbor ID preserved for edge {}",
            i
        );
    }

    // Create cluster from all compact records
    let cluster = EdgeCluster::create_from_compact_edges(compact_records, 10, Direction::Outgoing)
        .expect("Failed to create cluster from compact records");

    // Verify cluster has all edges
    assert_eq!(
        cluster.edge_count(),
        3,
        "Cluster should contain exactly 3 edges"
    );

    // Test serialization/deserialization preserves all data
    let cluster_bytes = cluster.serialize();
    println!(
        "DEBUG multi-edge: cluster_bytes.len() = {}",
        cluster_bytes.len()
    );
    println!(
        "DEBUG multi-edge: cluster.edge_count() = {}",
        cluster.edge_count()
    );
    println!(
        "DEBUG multi-edge: cluster.payload_size() = {}",
        cluster.payload_size()
    );

    // Debug: Parse header manually
    if cluster_bytes.len() >= 8 {
        let edge_count = u32::from_be_bytes([
            cluster_bytes[0],
            cluster_bytes[1],
            cluster_bytes[2],
            cluster_bytes[3],
        ]);
        let payload_size = u32::from_be_bytes([
            cluster_bytes[4],
            cluster_bytes[5],
            cluster_bytes[6],
            cluster_bytes[7],
        ]);
        println!(
            "DEBUG multi-edge header: edge_count={}, payload_size={}, total_bytes={}",
            edge_count,
            payload_size,
            cluster_bytes.len()
        );
        println!(
            "DEBUG multi-edge expected: 8 + {} = {}",
            payload_size,
            8 + payload_size
        );
        if cluster_bytes.len() != 8 + payload_size as usize {
            println!("ERROR: Multi-edge size mismatch!");
        }
    }

    let deserialized =
        EdgeCluster::deserialize(&cluster_bytes).expect("Failed to deserialize multi-edge cluster");

    assert_eq!(
        deserialized.edge_count(),
        3,
        "Deserialized cluster should have 3 edges"
    );

    // Verify each edge's data preserved through roundtrip
    let deserialized_edges = deserialized.edges();
    for (i, expected_original) in edges.iter().enumerate() {
        let deserialized_edge = &deserialized_edges[i];

        let resolved_type = string_table
            .get_string(deserialized_edge.edge_type_offset)
            .expect(&format!(
                "Failed to resolve type for deserialized edge {}",
                i
            ));
        assert_eq!(
            resolved_type, expected_original.edge_type,
            "Type preserved for edge {} through roundtrip",
            i
        );

        let roundtrip_data: serde_json::Value =
            serde_json::from_slice(&deserialized_edge.edge_data)
                .expect(&format!("Failed to deserialize data for edge {}", i));
        assert_eq!(
            roundtrip_data, expected_original.data,
            "Data preserved for edge {} through roundtrip",
            i
        );

        assert_eq!(
            deserialized_edge.neighbor_id, expected_original.to_id,
            "Neighbor ID preserved for edge {} through roundtrip",
            i
        );
    }
}

/// Test 3: Cluster update pipeline integration test
/// Test the complete cluster update flow without any EdgeRecord reconstruction
#[test]
fn test_cluster_update_pipeline_integration() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_cluster_update_pipeline.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create initial nodes
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Function".to_string(),
            name: "source_func".to_string(),
            file_path: Some("/path/to/source.rs".to_string()),
            data: serde_json::json!({"lines": 100, "complexity": "medium"}),
        })
        .unwrap();

    let target1_id = graph
        .insert_node(NodeSpec {
            kind: "Function".to_string(),
            name: "target_func_1".to_string(),
            file_path: Some("/path/to/target1.rs".to_string()),
            data: serde_json::json!({"lines": 50, "complexity": "low"}),
        })
        .unwrap();

    let target2_id = graph
        .insert_node(NodeSpec {
            kind: "Module".to_string(),
            name: "target_module".to_string(),
            file_path: Some("/path/to/target2.rs".to_string()),
            data: serde_json::json!({"exports": 5, "imports": 12}),
        })
        .unwrap();

    // Insert first edge
    graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target1_id,
            edge_type: "calls".to_string(),
            data: serde_json::json!({
                "line": 15,
                "argument_count": 3,
                "is_async": false,
                "call_type": "direct"
            }),
        })
        .unwrap();

    // Verify first edge neighbors
    let neighbors_after_first = graph
        .neighbors(
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        neighbors_after_first.len(),
        1,
        "Should have 1 neighbor after first edge"
    );
    assert_eq!(
        neighbors_after_first[0], target1_id,
        "First neighbor should be target1_id"
    );

    // Insert second edge to same source
    graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target2_id,
            edge_type: "imports".to_string(),
            data: serde_json::json!({
                "import_name": "GraphData",
                "alias": Some("Data"),
                "is_wildcard": false,
                "visibility": "public"
            }),
        })
        .unwrap();

    // Verify both edges preserved correctly
    let neighbors_after_second = graph
        .neighbors(
            source_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        neighbors_after_second.len(),
        2,
        "Should have 2 neighbors after second edge"
    );

    let mut sorted_neighbors = neighbors_after_second.clone();
    sorted_neighbors.sort();
    let mut expected_targets = vec![target1_id, target2_id];
    expected_targets.sort();
    assert_eq!(
        sorted_neighbors, expected_targets,
        "Both target IDs should be present as neighbors"
    );

    // Verify V2 cluster metadata via direct file access
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(source_id).unwrap();

    assert_eq!(
        source_node.outgoing_edge_count, 2,
        "Source should have 2 outgoing edges in metadata"
    );
    assert!(
        source_node.outgoing_cluster_offset > 0,
        "Source should have valid cluster offset"
    );
    assert!(
        source_node.outgoing_cluster_size > 0,
        "Source should have valid cluster size"
    );

    // Verify cluster contains both edges with correct data
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let cluster_edges = edge_store
        .read_clustered_edges(
            source_node.outgoing_cluster_offset,
            source_node.outgoing_cluster_size,
            Direction::Outgoing,
        )
        .unwrap();

    assert_eq!(
        cluster_edges.len(),
        2,
        "Cluster should contain exactly 2 compact edges"
    );

    // Verify edge data preservation in cluster
    let mut string_table = StringTable::new();

    // TODO: In real implementation, string table should be loaded from file
    // For now, we'll rebuild it from the cluster edges
    for edge in &cluster_edges {
        // We can't easily verify types without the original string table
        // But we can verify neighbor IDs and data structure
        assert!(
            edge.neighbor_id > 0,
            "Compact edge should have valid neighbor ID"
        );
        assert!(
            !edge.edge_data.is_empty(),
            "Compact edge should have non-empty data"
        );

        let data: serde_json::Value =
            serde_json::from_slice(&edge.edge_data).expect("Edge data should be valid JSON");
        assert!(
            data.as_object().is_some(),
            "Edge data should be a JSON object"
        );
    }
}

/// Test 4: Incoming/outgoing cluster consistency
/// Verify that both incoming and outgoing clusters work correctly
#[test]
fn test_incoming_outgoing_cluster_consistency() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir
        .path()
        .join("test_incoming_outgoing_consistency.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create nodes: 1 -> 2, 1 -> 3, 4 -> 1
    let node1_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!({"role": "central"}),
        })
        .unwrap();

    let node2_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "node2".to_string(),
            file_path: None,
            data: serde_json::json!({"role": "target"}),
        })
        .unwrap();

    let node3_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "node3".to_string(),
            file_path: None,
            data: serde_json::json!({"role": "target"}),
        })
        .unwrap();

    let node4_id = graph
        .insert_node(NodeSpec {
            kind: "Node".to_string(),
            name: "node4".to_string(),
            file_path: None,
            data: serde_json::json!({"role": "source"}),
        })
        .unwrap();

    // Create edges with distinct data
    graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node2_id,
            edge_type: "connects_to".to_string(),
            data: serde_json::json!({"type": "primary", "strength": 0.8}),
        })
        .unwrap();

    graph
        .insert_edge(EdgeSpec {
            from: node1_id,
            to: node3_id,
            edge_type: "references".to_string(),
            data: serde_json::json!({"type": "secondary", "strength": 0.6}),
        })
        .unwrap();

    graph
        .insert_edge(EdgeSpec {
            from: node4_id,
            to: node1_id,
            edge_type: "depends_on".to_string(),
            data: serde_json::json!({"type": "critical", "strength": 0.9}),
        })
        .unwrap();

    // Verify outgoing neighbors from node1
    let node1_outgoing = graph
        .neighbors(
            node1_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        node1_outgoing.len(),
        2,
        "Node1 should have 2 outgoing neighbors"
    );
    let mut sorted_outgoing = node1_outgoing.clone();
    sorted_outgoing.sort();
    let mut expected_outgoing = vec![node2_id, node3_id];
    expected_outgoing.sort();
    assert_eq!(
        sorted_outgoing, expected_outgoing,
        "Node1 outgoing should be [2, 3]"
    );

    // Verify incoming neighbors to node1
    let node1_incoming = graph
        .neighbors(
            node1_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Incoming,
                edge_type: None,
            },
        )
        .unwrap();

    assert_eq!(
        node1_incoming.len(),
        1,
        "Node1 should have 1 incoming neighbor"
    );
    assert_eq!(
        node1_incoming[0], node4_id,
        "Node1 incoming should be node4"
    );

    // Verify cluster metadata consistency
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);

    let node1 = node_store.read_node_v2(node1_id).unwrap();
    assert_eq!(
        node1.outgoing_edge_count, 2,
        "Node1 should have 2 outgoing edges"
    );
    assert_eq!(
        node1.incoming_edge_count, 1,
        "Node1 should have 1 incoming edge"
    );
    assert!(
        node1.has_outgoing_edges(),
        "Node1 should have outgoing edges"
    );
    assert!(
        node1.has_incoming_edges(),
        "Node1 should have incoming edges"
    );

    // Verify other nodes have correct cluster metadata
    let node2 = node_store.read_node_v2(node2_id).unwrap();
    assert_eq!(
        node2.incoming_edge_count, 1,
        "Node2 should have 1 incoming edge"
    );
    assert_eq!(
        node2.outgoing_edge_count, 0,
        "Node2 should have 0 outgoing edges"
    );

    let node3 = node_store.read_node_v2(node3_id).unwrap();
    assert_eq!(
        node3.incoming_edge_count, 1,
        "Node3 should have 1 incoming edge"
    );
    assert_eq!(
        node3.outgoing_edge_count, 0,
        "Node3 should have 0 outgoing edges"
    );

    let node4 = node_store.read_node_v2(node4_id).unwrap();
    assert_eq!(
        node4.outgoing_edge_count, 1,
        "Node4 should have 1 outgoing edge"
    );
    assert_eq!(
        node4.incoming_edge_count, 0,
        "Node4 should have 0 incoming edges"
    );
}

/// Test 5: Cluster corruption detection and prevention
/// Verify that the new pipeline prevents common corruption scenarios
#[test]
fn test_cluster_corruption_detection_prevention() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_corruption_prevention.db");

    let config = GraphConfig::native();
    let graph = open_graph(&db_path, &config).unwrap();

    // Create source and target nodes
    let source_id = graph
        .insert_node(NodeSpec {
            kind: "Source".to_string(),
            name: "source".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let target_id = graph
        .insert_node(NodeSpec {
            kind: "Target".to_string(),
            name: "target".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Create edge with complex data that would be lost in reconstruction
    graph
        .insert_edge(EdgeSpec {
            from: source_id,
            to: target_id,
            edge_type: "complex_relationship".to_string(),
            data: serde_json::json!({
                "metadata": {
                    "created_at": "2024-01-15T10:30:00Z",
                    "created_by": "automated_test",
                    "version": "1.2.3",
                    "confidence": 0.95
                },
                "properties": {
                    "is_bidirectional": false,
                    "weight": 2.71828,
                    "priority": "high",
                    "tags": ["important", "critical", "tested"]
                },
                "validation": {
                    "schema_version": "v2",
                    "checksum": "a1b2c3d4e5f6",
                    "is_validated": true
                }
            }),
        })
        .unwrap();

    // Read cluster data directly to verify no corruption occurred
    let mut graph_file = GraphFile::open(&db_path).unwrap();
    let mut node_store = NodeStore::new(&mut graph_file);
    let source_node = node_store.read_node_v2(source_id).unwrap();

    assert!(
        source_node.has_outgoing_edges(),
        "Source should have outgoing cluster"
    );

    // Read the cluster and verify data integrity
    let mut edge_store = EdgeStore::new(&mut graph_file);
    let cluster_edges = edge_store
        .read_clustered_edges(
            source_node.outgoing_cluster_offset,
            source_node.outgoing_cluster_size,
            Direction::Outgoing,
        )
        .unwrap();

    assert_eq!(
        cluster_edges.len(),
        1,
        "Cluster should contain exactly 1 edge"
    );

    let compact_edge = &cluster_edges[0];
    assert_eq!(
        compact_edge.neighbor_id, target_id,
        "Neighbor ID should be preserved"
    );

    // Verify edge data is not corrupted (should be complex JSON, not empty {})
    let edge_data: serde_json::Value =
        serde_json::from_slice(&compact_edge.edge_data).expect("Edge data should be valid JSON");

    assert!(
        edge_data.as_object().is_some(),
        "Edge data should be an object"
    );

    // Check for specific complex fields that would be lost in reconstruction
    if let Some(obj) = edge_data.as_object() {
        assert!(
            obj.contains_key("metadata"),
            "Edge data should contain metadata"
        );
        assert!(
            obj.contains_key("properties"),
            "Edge data should contain properties"
        );

        if let Some(metadata) = obj.get("metadata").and_then(|v| v.as_object()) {
            assert!(
                metadata.contains_key("created_at"),
                "Metadata should contain timestamp"
            );
            assert!(
                metadata.contains_key("confidence"),
                "Metadata should contain confidence"
            );
        }

        if let Some(properties) = obj.get("properties").and_then(|v| v.as_object()) {
            assert!(
                properties.contains_key("weight"),
                "Properties should contain weight"
            );
            assert!(
                properties.contains_key("tags"),
                "Properties should contain tags"
            );
        }
    }

    // Verify edge type is preserved (not "reconstructed")
    // This requires loading the string table in a real implementation
    // For now, we verify that the edge data is not the corrupted placeholder
    let edge_data_str = String::from_utf8_lossy(&compact_edge.edge_data);
    assert!(
        !edge_data_str.contains("{}"),
        "Edge data should not be empty placeholder"
    );
    assert!(
        !edge_data_str.contains("reconstructed"),
        "Edge data should not contain reconstruction artifacts"
    );
}

/// Test 6: EdgeCluster validation and consistency checks
/// Verify EdgeCluster validation works correctly with the new pipeline
#[test]
fn test_edgecluster_validation_consistency() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_edgecluster_validation.db");

    // Create valid edges
    let edges = vec![
        EdgeRecord::new(
            1,
            10,
            20,
            "valid_edge".to_string(),
            serde_json::json!({"test": true}),
        ),
        EdgeRecord::new(
            2,
            10,
            30,
            "another_edge".to_string(),
            serde_json::json!({"count": 42}),
        ),
    ];

    let mut string_table = StringTable::new();

    // Create compact records
    let compact_records: Result<Vec<_>, _> = edges
        .iter()
        .map(|edge| {
            CompactEdgeRecord::from_edge_record(edge, Direction::Outgoing, &mut string_table)
        })
        .collect();
    let compact_records = compact_records.expect("Failed to create compact records");

    // Create cluster
    let cluster = EdgeCluster::create_from_compact_edges(compact_records, 10, Direction::Outgoing)
        .expect("Failed to create cluster");

    // Validate cluster
    cluster
        .validate()
        .expect("Valid cluster should pass validation");

    // Test cluster serialization consistency
    let serialized = cluster.serialize();
    assert_eq!(
        serialized.len(),
        cluster.size_bytes(),
        "Serialized size should match size_bytes()"
    );

    // Test deserialization validation
    let deserialized = EdgeCluster::deserialize(&serialized)
        .expect("Valid serialized cluster should deserialize successfully");

    deserialized
        .validate()
        .expect("Deserialized cluster should be valid");

    // Verify consistency between original and deserialized
    assert_eq!(
        cluster.edge_count(),
        deserialized.edge_count(),
        "Edge count should be consistent"
    );
    assert_eq!(
        cluster.edges().len(),
        deserialized.edges().len(),
        "Edge list length should be consistent"
    );

    // Test efficiency heuristic
    let efficiency = cluster.is_efficient();
    println!("DEBUG: Cluster efficiency: {}", efficiency);
    // Efficiency is calculated based on average edge size, so we just verify it runs without error
}
