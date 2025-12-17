#![cfg(feature = "v2_experimental")]
// Test specifically to trigger rebuild_v2_index() and capture check_buffer content

use sqlitegraph::backend::native::{
    NativeResult, NodeRecord, graph_file::GraphFile, node_store::NodeStore,
};
use tempfile::NamedTempFile;

#[test]
fn test_trigger_rebuild_v2_index_capture_buffer() -> NativeResult<()> {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");

    // Create a fresh database and write a node using normal NodeStore operations
    {
        let mut graph_file = GraphFile::create(temp_file.path()).unwrap();
        let mut node_store = NodeStore::new(&mut graph_file);

        // Write a node using normal operations - this will create V2 format
        let node = NodeRecord::new(
            1,
            "TestNode".to_string(),
            "test_node".to_string(),
            serde_json::json!({"key": "value"}),
        );

        node_store.write_node(&node)?;

        eprintln!("DEBUG: Wrote node successfully");
    }

    // Now reopen and try to read - this should trigger rebuild_v2_index()
    let mut graph_file = GraphFile::open(temp_file.path())?;
    let mut node_store = NodeStore::new(&mut graph_file);

    eprintln!("DEBUG: About to call read_node(1) - should trigger rebuild_v2_index()");

    // This should trigger rebuild_v2_index() since index_built is false
    let result = node_store.read_node(1);

    match result {
        Ok(node) => {
            eprintln!("DEBUG: Successfully read node: {:?}", node);
        }
        Err(e) => {
            eprintln!("DEBUG: Error reading node: {:?}", e);
        }
    }

    Ok(())
}
