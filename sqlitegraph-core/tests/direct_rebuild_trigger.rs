#![cfg(all(feature = "v2_experimental", feature = "v2_dev_tdd"))]
// Direct test to trigger rebuild_v2_index and capture check_buffer content.
// This fixture exercises APIs that do not exist in the current snapshot.

use sqlitegraph::backend::native::{
    GraphFile, NativeResult, node_store::NodeStore, v2::node_record_v2::NodeRecordV2,
};
use tempfile::tempdir;

#[test]
fn test_direct_rebuild_v2_index_capture() -> NativeResult<()> {
    // Create temporary directory
    let temp_dir =
        tempdir().map_err(|e| sqlitegraph::backend::native::NativeBackendError::Io(e))?;
    let db_path = temp_dir.path().join("test.db");

    // Create a fresh graph file
    let mut graph_file = GraphFile::create(&db_path)?;

    // Manually write some V2 node records to disk to simulate what we're debugging
    let node_id_1 = 1i64;
    let node_record_v2_1 = NodeRecordV2::new(
        node_id_1,
        "test".to_string(),
        "test_node".to_string(),
        serde_json::json!({}),
    );
    let buffer_1 = node_record_v2_1.serialize();

    println!(
        "DEBUG: Manually created V2 node record buffer: {:?}",
        &buffer_1[..buffer_1.len().min(16)]
    );

    // Write directly to file at node data offset using the file handle directly
    let node_data_offset = graph_file.header().node_data_offset as u64;
    use std::io::Seek;
    use std::io::SeekFrom;
    use std::io::Write;

    graph_file.file.seek(SeekFrom::Start(node_data_offset))?;
    graph_file.file.write_all(&buffer_1)?;
    graph_file.file.flush()?;

    // Create NodeStore and trigger rebuild_v2_index
    let mut node_store = NodeStore::new(&mut graph_file);

    println!("DEBUG: About to call rebuild_v2_index...");

    // This should trigger the DEBUG REBUILD output we need
    let result = node_store.rebuild_v2_index(&mut graph_file)?;

    println!("DEBUG: rebuild_v2_index completed, found {} nodes", result);

    Ok(())
}
