use sqlitegraph::backend::NodeSpec;
use sqlitegraph::{GraphConfig, open_graph};
use tempfile::TempDir;

#[test]
fn test_v2_node_slot_persistence_reopen() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new V2 native graph file in temp dir
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test_v2_persistence.db");

    // Create graph with native V2 backend
    let graph =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

    // Insert 3 nodes with non-empty data
    println!("Inserting nodes...");
    let node_id1 = graph
        .insert_node(NodeSpec {
            kind: "Function".to_string(),
            name: "main".to_string(),
            file_path: Some("/src/main.rs".to_string()),
            data: serde_json::json!({"lines": 100, "complexity": "medium"}),
        })
        .expect("Failed to insert node 1");

    let node_id2 = graph
        .insert_node(NodeSpec {
            kind: "Variable".to_string(),
            name: "config".to_string(),
            file_path: Some("/src/config.rs".to_string()),
            data: serde_json::json!({"type": "string", "mutable": false}),
        })
        .expect("Failed to insert node 2");

    let node_id3 = graph
        .insert_node(NodeSpec {
            kind: "Module".to_string(),
            name: "database".to_string(),
            file_path: Some("/src/database/mod.rs".to_string()),
            data: serde_json::json!({"dependencies": 5, "exports": ["connect", "query"]}),
        })
        .expect("Failed to insert node 3");

    println!("Inserted nodes: {}, {}, {}", node_id1, node_id2, node_id3);

    // Insert at least 1 edge between nodes
    println!("Inserting edge...");
    let edge_id = graph
        .insert_edge(sqlitegraph::backend::EdgeSpec {
            from: node_id1,
            to: node_id2,
            edge_type: "imports".to_string(),
            data: serde_json::json!({"reason": "config dependency"}),
        })
        .expect("Failed to insert edge");

    println!("Inserted edge: {}", edge_id);

    // Read all 3 nodes back directly
    println!("Reading nodes before close...");
    let read_node1 = graph.get_node(node_id1).expect("Failed to read node 1");
    let read_node2 = graph.get_node(node_id2).expect("Failed to read node 2");
    let read_node3 = graph.get_node(node_id3).expect("Failed to read node 3");

    // Verify node data integrity
    assert_eq!(read_node1.kind, "Function");
    assert_eq!(read_node1.name, "main");
    assert_eq!(read_node2.kind, "Variable");
    assert_eq!(read_node2.name, "config");
    assert_eq!(read_node3.kind, "Module");
    assert_eq!(read_node3.name, "database");

    println!("SUCCESS: All nodes read correctly before close");

    // Drop the graph handle (close the file)
    drop(graph);
    println!("Graph closed");

    // Reopen the same file
    println!("Reopening graph...");
    let graph_reopened =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to reopen V2 native graph");

    // Read nodes again after reopen
    println!("Reading nodes after reopen...");
    let reopened_node1 = graph_reopened
        .get_node(node_id1)
        .expect("Failed to read node 1 after reopen");
    let reopened_node2 = graph_reopened
        .get_node(node_id2)
        .expect("Failed to read node 2 after reopen");
    let reopened_node3 = graph_reopened
        .get_node(node_id3)
        .expect("Failed to read node 3 after reopen");

    // Verify node data integrity after reopen
    assert_eq!(reopened_node1.kind, "Function");
    assert_eq!(reopened_node1.name, "main");
    assert_eq!(reopened_node2.kind, "Variable");
    assert_eq!(reopened_node2.name, "config");
    assert_eq!(reopened_node3.kind, "Module");
    assert_eq!(reopened_node3.name, "database");

    // Verify edge persistence
    let neighbors = graph_reopened
        .neighbors(
            node_id1,
            sqlitegraph::backend::NeighborQuery {
                direction: sqlitegraph::backend::BackendDirection::Outgoing,
                edge_type: Some("imports".to_string()),
            },
        )
        .expect("Failed to get neighbors");

    assert!(!neighbors.is_empty(), "Should have at least one neighbor");
    assert!(
        neighbors.contains(&node_id2),
        "Should contain node2 as neighbor"
    );

    println!("SUCCESS: All nodes and edges persisted correctly after reopen");

    Ok(())
}
