// Simple test to trigger rebuild_v2_index and capture the DEBUG REBUILD output

use sqlitegraph::backend::native::{
    GraphFile, NativeResult, node_store::NodeStore, v2::node_record_v2::NodeRecordV2,
};
use tempfile::tempdir;

#[test]
fn test_trigger_debug_rebuild_output() -> NativeResult<()> {
    // Create temporary directory
    let temp_dir =
        tempdir().map_err(|e| sqlitegraph::backend::native::NativeBackendError::Io(e))?;
    let db_path = temp_dir.path().join("test.db");

    // Create a fresh graph file
    let mut graph_file = GraphFile::create(&db_path)?;

    // Create a V2 node record
    let node_record_v2 = NodeRecordV2::new(
        1i64,
        "test".to_string(),
        "test_node".to_string(),
        serde_json::json!({}),
    );
    let buffer = node_record_v2.serialize();

    println!(
        "DEBUG: Created V2 node record, first 16 bytes: {:?}",
        &buffer[..16]
    );

    // Write to node data offset using GraphFile's write_bytes method
    let node_data_offset = graph_file.header().node_data_offset;
    graph_file.write_bytes(node_data_offset, &buffer)?;

    // Also write at offset 992 to ensure rebuild steps over it
    // This will make the 32-byte stepping hit our node
    let filler_offset = 992;
    let filler_buffer = vec![0u8; 32];
    graph_file.write_bytes(filler_offset, &filler_buffer)?;

    // Create NodeStore - this should automatically trigger rebuild_v2_index
    println!("DEBUG: Creating NodeStore (should trigger rebuild)...");
    let mut node_store = NodeStore::new(&mut graph_file);

    // Try to read node to force index rebuilding
    let result = node_store.read_node(1);
    println!("DEBUG: read_node(1) result: {:?}", result);

    println!("DEBUG: Test completed");

    Ok(())
}
