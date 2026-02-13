//! Phase 76 instrumentation test example
//!
//! This example demonstrates the Phase 76 byte-proof instrumentation for V2 node record persistence.

use sqlitegraph::backend::native::graph_file::GraphFile;
use sqlitegraph::backend::native::node_store::NodeStore;
use sqlitegraph::backend::native::v2::node_record_v2::{NodeRecordV2, NodeRecordV2Ext};
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable Phase 75 instrumentation to trigger Phase 76 byte-proof logging
    unsafe {
        env::set_var("PHASE75_INSTRUMENTATION", "1");
    }
    println!("=== Phase 76 Instrumentation Test ===");

    // Create temp file
    let temp_file = tempfile::NamedTempFile::new()?;
    let path = temp_file.path();

    println!("Creating GraphFile at: {:?}", path);
    let mut graph_file = GraphFile::create(path)?;

    // Create node 1
    println!("\n=== Creating Node 1 (Phase 76 instrumentation) ===");
    let node1_v2 = NodeRecordV2::new(
        1,
        "TestNode".to_string(),
        "test_node_1".to_string(),
        serde_json::json!({"test": true, "node_id": 1}),
    );

    let mut node_store = NodeStore::new(&mut graph_file);
    println!("Writing node 1 (should show [phase76] NODE_WRITE instrumentation)...");
    node_store.write_node_v2(&node1_v2)?;

    // Read node 1 back to trigger read instrumentation
    println!("Reading node 1 (should show [phase76] NODE_READ instrumentation)...");
    let _read_node = node_store.read_node_v2(1)?;

    println!("\n=== Creating Node 2 (Phase 76 instrumentation) ===");
    let node2_v2 = NodeRecordV2::new(
        2,
        "TestNode".to_string(),
        "test_node_2".to_string(),
        serde_json::json!({"test": true, "node_id": 2}),
    );

    println!("Writing node 2 (should show [phase76] NODE_WRITE instrumentation)...");
    node_store.write_node_v2(&node2_v2)?;

    // Read node 2 back to trigger read instrumentation
    println!("Reading node 2 (should show [phase76] NODE_READ instrumentation)...");
    let _read_node = node_store.read_node_v2(2)?;

    // Simulate write-set recording (this is what Phase 75 does during transactions)
    println!("\n=== Simulating Transaction Write-Set ===");
    graph_file.record_node_v2_cluster_modified(1);
    graph_file.record_node_v2_cluster_modified(2);

    // Simulate rollback to trigger Phase 76 ROLLBACK instrumentation
    println!("\n=== Simulating Rollback (Phase 76 instrumentation) ===");
    println!("This should show [phase76] ROLLBACK_BEFORE and ROLLBACK_AFTER instrumentation...");
    graph_file.rollback_transaction()?;

    println!("\n=== Phase 76 Instrumentation Test Complete ===");
    println!(
        "If you saw [phase76] instrumentation messages above, the byte-proof logging is working!"
    );

    Ok(())
}
