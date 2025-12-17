use sqlitegraph::backend::{EdgeSpec, NodeSpec};
use sqlitegraph::{GraphConfig, open_graph};
use std::time::Instant;
use tempfile::TempDir;

#[test]
fn test_v2_performance_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== V2 PERFORMANCE VALIDATION ===");

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("performance_validation.db");

    // Phase 1: Node insertion performance
    println!("Phase 1: Node insertion performance test...");
    let graph =
        open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

    let start = Instant::now();
    let node_ids: Vec<i64> = (1..=1000)
        .map(|i| {
            graph
                .insert_node(NodeSpec {
                    kind: "Function".to_string(),
                    name: format!("function_{}", i),
                    file_path: Some(format!("/src/module_{}.rs", i % 10)),
                    data: serde_json::json!({"lines": i * 10, "complexity": "medium"}),
                })
                .expect("Failed to insert node")
        })
        .collect();

    let node_duration = start.elapsed();
    println!("✅ Inserted 1000 nodes in {:?}", node_duration);
    assert!(
        node_duration.as_secs() < 5,
        "Node insertion too slow: {:?}",
        node_duration
    );

    // Phase 2: Edge insertion performance
    println!("Phase 2: Edge insertion performance test...");
    let start = Instant::now();
    let mut edge_count = 0;

    for (i, &from_id) in node_ids.iter().enumerate() {
        if i + 1 < node_ids.len() {
            let to_id = node_ids[i + 1];
            graph
                .insert_edge(EdgeSpec {
                    from: from_id,
                    to: to_id,
                    edge_type: "calls".to_string(),
                    data: serde_json::json!({"reason": "function call"}),
                })
                .expect("Failed to insert edge");
            edge_count += 1;
        }
    }

    let edge_duration = start.elapsed();
    println!("✅ Inserted {} edges in {:?}", edge_count, edge_duration);
    assert!(
        edge_duration.as_secs() < 5,
        "Edge insertion too slow: {:?}",
        edge_duration
    );

    // Phase 3: Traversal performance test
    println!("Phase 3: Traversal performance test...");
    let start = Instant::now();

    // Simple read performance test
    for &node_id in node_ids.iter().take(100) {
        let node = graph.get_node(node_id).expect("Failed to get node");
        assert_eq!(node.id, node_id);
    }

    let traversal_duration = start.elapsed();
    println!("✅ Traversed 100 nodes in {:?}", traversal_duration);
    assert!(
        traversal_duration.as_secs() < 2,
        "Traversal too slow: {:?}",
        traversal_duration
    );

    // Phase 4: Verify no corruption occurred
    println!("Phase 4: Corruption verification...");
    let sample_node_id = node_ids[500];
    let node = graph
        .get_node(sample_node_id)
        .expect("Failed to retrieve node");

    assert_eq!(node.id, sample_node_id);
    assert_eq!(node.kind, "Function");
    assert!(node.name.starts_with("function_"));

    println!(
        "✅ Sample node verification passed: ID={}, Kind={}, Name={}",
        node.id, node.kind, node.name
    );

    drop(graph); // Close graph

    println!("🎉 PERFORMANCE VALIDATION COMPLETE:");
    println!(
        "   • Node insertion: 1000 nodes in {:?} (< 5s target)",
        node_duration
    );
    println!(
        "   • Edge insertion: {} edges in {:?} (< 5s target)",
        edge_count, edge_duration
    );
    println!(
        "   • Traversal: 100 nodes in {:?} (< 2s target)",
        traversal_duration
    );
    println!("   • Corruption check: ✅ PASSED");
    println!("✅ V2-only backend meets performance thresholds");

    Ok(())
}
