//! Diagnostic test to capture exact JSON parsing failure location
//! This test asserts that JSON parsing errors include the JSONCTX marker

use sqlitegraph::{EdgeSpec, GraphConfig, NodeSpec, open_graph};
use tempfile::tempdir;

#[test]
fn test_json_parsing_diagnostics_capture() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("diagnostics.db");

    let graph = open_graph(&db_path, &GraphConfig::native())?;

    // Create nodes successfully
    let node1 = graph.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "node1".to_string(),
        file_path: None,
        data: serde_json::json!({"id": 1}),
    })?;

    let node2 = graph.insert_node(NodeSpec {
        kind: "Test".to_string(),
        name: "node2".to_string(),
        file_path: None,
        data: serde_json::json!({"id": 2}),
    })?;

    // This edge insertion should fail and produce our diagnostic marker
    let result = graph.insert_edge(EdgeSpec {
        from: node1,
        to: node2,
        edge_type: "test".to_string(),
        data: serde_json::json!({"order": 1}),
    });

    match result {
        Ok(_) => panic!("Expected edge insertion to fail with JSON parsing error"),
        Err(e) => {
            let error_string = e.to_string();

            // Assert that our diagnostic marker is present in the error
            assert!(
                error_string.contains("JSONCTX"),
                "Error should contain JSONCTX marker. Got: {}",
                error_string
            );

            // Assert that the error contains expected diagnostic fields
            assert!(
                error_string.contains("file="),
                "Error should contain file field. Got: {}",
                error_string
            );

            assert!(
                error_string.contains("len="),
                "Error should contain buffer length. Got: {}",
                error_string
            );

            assert!(
                error_string.contains("head="),
                "Error should contain buffer head hex. Got: {}",
                error_string
            );

            println!("✅ JSON diagnostic captured: {}", error_string);
        }
    }

    Ok(())
}
