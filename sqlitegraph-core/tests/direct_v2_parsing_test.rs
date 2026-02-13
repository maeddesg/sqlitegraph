// Direct test to read V2 node at offset 1024 and verify parsing

use sqlitegraph::backend::native::{GraphFile, NativeResult, v2::node_record_v2::NodeRecordV2};
use tempfile::tempdir;

#[test]
fn test_direct_v2_parsing_at_offset_1024() -> NativeResult<()> {
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

    // Write to node data offset
    let node_data_offset = graph_file.header().node_data_offset;
    graph_file.write_bytes(node_data_offset, &buffer)?;

    // Create NodeStore to trigger rebuild
    // Now directly read from offset 1024 to see what's there
    let mut read_buffer = vec![0u8; 32];
    graph_file.read_bytes(1024, &mut read_buffer)?;

    println!(
        "DEBUG: Direct read from offset 1024: {:?}",
        &read_buffer[..16]
    );
    println!(
        "DEBUG: check_buffer[0] = {}, check_buffer[2] = {}",
        read_buffer[0], read_buffer[2]
    );

    // Test the problematic parsing path directly
    if read_buffer[0] == 2 {
        println!("DEBUG: Would take check_buffer[0] == 2 path");
        let node_id = i64::from_be_bytes([
            read_buffer[5],  // version + flags (1+4) = 5
            read_buffer[6],  // 6
            read_buffer[7],  // 7
            read_buffer[8],  // 8
            read_buffer[9],  // 9
            read_buffer[10], // 10
            read_buffer[11], // 11
            read_buffer[12], // 12
        ]);
        println!("DEBUG: Parsed node ID from positions 5-12: {}", node_id);
    } else if read_buffer.len() > 2 && read_buffer[2] == 2 {
        println!("DEBUG: Would take check_buffer[2] == 2 path");
        let node_id = i64::from_be_bytes([
            read_buffer[7],  // position 2 + 5 (version + flags + 2 reserved bytes)
            read_buffer[8],  // position 2 + 6
            read_buffer[9],  // position 2 + 7
            read_buffer[10], // position 2 + 8
            read_buffer[11], // position 2 + 9
            read_buffer[12], // position 2 + 10
            read_buffer[13], // position 2 + 11
            read_buffer[14], // position 2 + 12
        ]);
        println!("DEBUG: Parsed node ID from positions 7-14: {}", node_id);
    } else {
        println!("DEBUG: Neither V2 parsing path matched");
    }

    Ok(())
}
