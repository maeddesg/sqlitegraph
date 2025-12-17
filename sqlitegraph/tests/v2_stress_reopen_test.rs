use sqlitegraph::backend::{EdgeSpec, NodeSpec};
use sqlitegraph::{GraphConfig, open_graph};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_v2_stress_reopen_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== V2 STRESS & REOPEN VALIDATION ===");

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("stress_reopen.db");

    // Phase 1: High-volume node creation
    println!("Phase 1: Creating high volume of nodes...");
    {
        let graph =
            open_graph(&db_path, &GraphConfig::native()).expect("Failed to create V2 native graph");

        for i in 1..=500 {
            graph
                .insert_node(NodeSpec {
                    kind: "Function".to_string(),
                    name: format!("stress_function_{}", i),
                    file_path: Some(format!("/src/stress_module_{}.rs", i % 20)),
                    data: serde_json::json!({
                        "lines": i * 5,
                        "complexity": "high",
                        "test_id": i
                    }),
                })
                .expect("Failed to insert node");
        }

        println!("✅ Created 500 nodes successfully");
        drop(graph);
    }

    // Phase 2: File persistence verification
    println!("Phase 2: Verifying file persistence...");
    assert!(
        db_path.exists(),
        "Database file should exist after node creation"
    );
    let file_size = fs::metadata(&db_path)?.len();
    println!("✅ Database file size: {} bytes", file_size);
    assert!(file_size > 1024, "Database should be larger than header");

    // Phase 3: Reopen and verify nodes
    println!("Phase 3: Reopening graph and verifying node integrity...");
    {
        let graph =
            open_graph(&db_path, &GraphConfig::native()).expect("Failed to reopen V2 native graph");

        // Verify sample nodes exist with correct data
        for test_id in [1, 100, 250, 500] {
            let node = graph
                .get_node(test_id)
                .expect("Failed to retrieve node after reopen");

            assert_eq!(node.id, test_id);
            assert_eq!(node.kind, "Function");
            assert!(node.name.starts_with("stress_function_"));

            let lines = node.data.get("lines").and_then(|v| v.as_i64()).unwrap_or(0);
            assert_eq!(lines, test_id as i64 * 5);

            println!(
                "✅ Node {} verified: ID={}, Kind={}, Name={}, Lines={}",
                test_id, node.id, node.kind, node.name, lines
            );
        }

        // Phase 4: High-volume edge creation after reopen
        println!("Phase 4: Creating high volume of edges after reopen...");
        for i in 1..=200 {
            let from_id = (i - 1) % 500 + 1; // Ensure valid node IDs
            let to_id = (i % 500) + 1;

            if from_id != to_id {
                // Avoid self-edges
                graph
                    .insert_edge(EdgeSpec {
                        from: from_id,
                        to: to_id,
                        edge_type: "calls".to_string(),
                        data: serde_json::json!({
                            "edge_id": i,
                            "relationship": "function_call"
                        }),
                    })
                    .expect("Failed to insert edge after reopen");
            }
        }

        println!("✅ Created 200 edges successfully after reopen");
        drop(graph);
    }

    // Phase 5: Final reopen verification
    println!("Phase 5: Final reopen and comprehensive verification...");
    {
        let graph =
            open_graph(&db_path, &GraphConfig::native()).expect("Failed to perform final reopen");

        // Verify final file size growth
        let final_size = fs::metadata(&db_path)?.len();
        println!(
            "✅ Final database file size: {} bytes (growth: {} bytes)",
            final_size,
            final_size - file_size
        );
        assert!(
            final_size > file_size,
            "File should grow after edge insertion"
        );

        // Verify nodes are still intact after multiple reopens
        let test_node = graph
            .get_node(300)
            .expect("Failed to retrieve node after multiple reopens");
        assert_eq!(test_node.id, 300);
        assert_eq!(test_node.kind, "Function");

        println!(
            "✅ Node integrity maintained after multiple reopens: ID={}, Kind={}",
            test_node.id, test_node.kind
        );

        drop(graph);
    }

    println!("🎉 STRESS & REOPEN VALIDATION COMPLETE:");
    println!("   • Created 500 nodes with complex data");
    println!("   • Database file persisted and grew correctly");
    println!("   • Successfully reopened graph multiple times");
    println!("   • Node integrity maintained across reopens");
    println!("   • Created 200 edges after reopen");
    println!("   • No corruption detected in V2 backend");
    println!("✅ V2-only backend passes stress and reopen tests");

    Ok(())
}
