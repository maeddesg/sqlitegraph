use sqlitegraph::backend::NodeSpec;
use sqlitegraph::{GraphConfig, open_graph};
use tempfile::TempDir;

#[test]
fn regression_v2_node_version_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    // Create a native graph using the same method as the benchmark
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("benchmark.db");

    let graph = open_graph(&db_path, &GraphConfig::native()).expect("Failed to create graph");

    // Create nodes like the benchmark does
    let mut node_ids = Vec::new();
    for i in 0..3 {
        let node_id = graph
            .insert_node(NodeSpec {
                kind: "Node".to_string(),
                name: format!("node_{}", i),
                file_path: None,
                data: serde_json::json!({"id": i}),
            })
            .expect("Failed to insert node");
        println!("Inserted node {} with ID {}", i, node_id);
        node_ids.push(node_id);
    }

    // Use the actual allocated node IDs to create an edge
    let result = graph.insert_edge(sqlitegraph::backend::EdgeSpec {
        from: node_ids[0],
        to: node_ids[1],
        edge_type: "test_edge".to_string(),
        data: serde_json::json!({"order": 1}),
    });

    match result {
        Ok(_) => panic!("Expected V2 node version error, but edge insertion succeeded"),
        Err(e) => {
            let error_msg = e.to_string();
            if !error_msg.contains("uninitialized slot") {
                panic!("Expected 'uninitialized slot' error, got: {}", error_msg);
            }
            println!("REGRESSION TEST CAPTURED EXPECTED ERROR: {}", error_msg);
        }
    }

    Ok(())
}
