//! V2 Stress + Integrity Test
//!
//! Long-running stress test for V2 backend with periodic integrity validation.
//! Tests V2 behavior under heavy load and ensures no corruption occurs.
//!
//! This test runs only when explicitly enabled to avoid slowing down CI.
//! Enable with: RUST_TEST_STRESS=1 cargo test --features v2_experimental v2_stress_integrity

#[cfg(test)]
mod tests {
    use sqlitegraph::backend::native::{EdgeStore, GraphFile, NodeStore};
    use sqlitegraph::{BackendDirection, GraphConfig, NeighborQuery, SnapshotId, open_graph};
    use std::collections::HashMap;
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    /// Stress test configuration
    #[derive(Clone)]
    struct StressTestConfig {
        node_count: usize,
        edge_count: usize,
        validation_interval: usize, // Validate every N operations
        timeout_secs: u64,          // Maximum test duration
    }

    impl Default for StressTestConfig {
        fn default() -> Self {
            Self {
                node_count: 50_000,          // Conservative for CI
                edge_count: 500_000,         // 10x nodes for heavy load
                validation_interval: 25_000, // Validate every 25k inserts
                timeout_secs: 300,           // 5 minutes max
            }
        }
    }

    impl StressTestConfig {
        fn aggressive() -> Self {
            Self {
                node_count: 100_000,
                edge_count: 2_000_000,
                validation_interval: 100_000,
                timeout_secs: 1800, // 30 minutes for aggressive test
            }
        }
    }

    /// Stress test result
    struct StressTestResult {
        config: StressTestConfig,
        duration_secs: u64,
        edges_inserted: usize,
        validation_checks: usize,
        validations_passed: usize,
        file_size_bytes: u64,
        corruption_detected: bool,
        error_details: Option<String>,
    }

    /// Check if stress tests should run
    fn should_run_stress_tests() -> bool {
        env::var("RUST_TEST_STRESS").is_ok() || env::var("STRESS_TESTS").is_ok()
    }

    #[test]
    #[ignore] // Only run when explicitly enabled
    fn v2_stress_integrity_test() {
        if !should_run_stress_tests() {
            println!("Skipping V2 stress test (set RUST_TEST_STRESS=1 to enable)");
            return;
        }

        println!("Starting V2 stress and integrity test...");

        let config = StressTestConfig::default();
        let result = run_stress_test(&config);

        // Assert test passed
        assert!(
            !result.corruption_detected,
            "Corruption detected during stress test: {:?}",
            result.error_details
        );
        assert_eq!(
            result.validations_passed, result.validation_checks,
            "Some validation checks failed"
        );
        assert!(
            result.edges_inserted >= config.edge_count,
            "Not all edges were inserted"
        );

        println!("✅ Stress test completed successfully:");
        println!("   Duration: {} seconds", result.duration_secs);
        println!("   Edges inserted: {}", result.edges_inserted);
        println!(
            "   Validations: {}/{}",
            result.validations_passed, result.validation_checks
        );
        println!("   File size: {} bytes", result.file_size_bytes);
    }

    #[test]
    #[ignore] // Only run when explicitly enabled
    fn v2_aggressive_stress_test() {
        if !should_run_stress_tests() {
            println!("Skipping V2 aggressive stress test (set RUST_TEST_STRESS=1 to enable)");
            return;
        }

        println!("Starting V2 aggressive stress test...");

        let config = StressTestConfig::aggressive();
        let result = run_stress_test(&config);

        // Assert test passed
        assert!(
            !result.corruption_detected,
            "Corruption detected during aggressive stress test: {:?}",
            result.error_details
        );
        assert_eq!(
            result.validations_passed, result.validation_checks,
            "Some validation checks failed"
        );
        assert!(
            result.edges_inserted >= config.edge_count,
            "Not all edges were inserted"
        );

        println!("✅ Aggressive stress test completed successfully:");
        println!("   Duration: {} seconds", result.duration_secs);
        println!("   Edges inserted: {}", result.edges_inserted);
        println!(
            "   Validations: {}/{}",
            result.validations_passed, result.validation_checks
        );
        println!("   File size: {} bytes", result.file_size_bytes);
    }

    /// Run the actual stress test
    fn run_stress_test(config: &StressTestConfig) -> StressTestResult {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("v2_stress_test.db");
        let start_time = Instant::now();

        println!(
            "Creating V2 graph with {} nodes and {} edges",
            config.node_count, config.edge_count
        );

        // Create graph
        let graph = open_graph(&db_path, &GraphConfig::native())
            .expect("Failed to create V2 stress test graph");

        // Insert nodes
        println!("Inserting {} nodes...", config.node_count);
        let mut node_ids = Vec::with_capacity(config.node_count);
        for i in 0..config.node_count {
            let node_id = graph
                .insert_node(sqlitegraph::NodeSpec {
                    kind: "StressTestNode".to_string(),
                    name: format!("stress_node_{}", i),
                    file_path: None,
                    data: serde_json::json!({
                        "test_type": "stress",
                        "node_index": i,
                        "timestamp": "2024-01-01T00:00:00Z",
                    }),
                })
                .expect("Failed to insert stress test node");
            node_ids.push(node_id);

            // Periodic progress update
            if i > 0 && i % 10_000 == 0 {
                println!("   Inserted {}/{} nodes", i, config.node_count);
            }
        }
        println!("✅ Node insertion completed");

        // Insert edges with periodic validation
        println!(
            "Inserting {} edges with validation every {} edges...",
            config.edge_count, config.validation_interval
        );

        let mut edges_inserted = 0;
        let mut validation_checks = 0;
        let mut validations_passed = 0;
        let start_validation_time = Instant::now();
        let mut corruption_detected = false;
        let mut error_details = None;

        // Generate deterministic edges using seeded RNG
        let mut rng_state = 0xC0FFEE_u64.wrapping_add(node_ids.len() as u64);

        for edge_index in 0..config.edge_count {
            // Generate deterministic edge
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let from_idx = (rng_state as usize) % node_ids.len();

            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let mut to_idx = (rng_state as usize) % node_ids.len();

            // Avoid self-loops
            if to_idx == from_idx {
                to_idx = (to_idx + 1) % node_ids.len();
            }

            // Insert edge with realistic data
            graph
                .insert_edge(sqlitegraph::EdgeSpec {
                    from: node_ids[from_idx],
                    to: node_ids[to_idx],
                    edge_type: format!("stress_edge_{}", edge_index % 10), // Cycle through edge types
                    data: serde_json::json!({
                        "edge_index": edge_index,
                        "from_index": from_idx,
                        "to_index": to_idx,
                        "created_at": "2024-01-01T00:00:00Z",
                        "test_batch": edge_index / 1000,
                        "payload": format!("test_data_{}", edge_index % 100),
                    }),
                })
                .expect("Failed to insert stress test edge");

            edges_inserted += 1;

            // Periodic validation
            if edge_index > 0 && edge_index % config.validation_interval == 0 {
                validation_checks += 1;
                println!(
                    "   Validating after {}/{} edges (check #{})",
                    edge_index, config.edge_count, validation_checks
                );

                match run_validation_check(&db_path, &node_ids, validation_checks) {
                    Ok(_) => {
                        validations_passed += 1;
                        if validation_checks % 5 == 0 {
                            let elapsed = start_validation_time.elapsed();
                            let avg_time = elapsed.as_secs() as f64 / validation_checks as f64;
                            println!(
                                "   Validation check #{} passed (avg time: {:.2}s)",
                                validation_checks, avg_time
                            );
                        }
                    }
                    Err(e) => {
                        println!("❌ Validation check #{} failed: {}", validation_checks, e);
                        corruption_detected = true;
                        error_details = Some(format!("Validation failure: {}", e));
                        break;
                    }
                }

                // Check timeout
                if start_time.elapsed().as_secs() > config.timeout_secs {
                    println!("⏰ Stress test timeout reached");
                    break;
                }
            }

            // Progress update every 50k edges
            if edge_index > 0 && edge_index % 50_000 == 0 {
                let elapsed = start_time.elapsed();
                let rate = edges_inserted as f64 / elapsed.as_secs_f64();
                println!(
                    "   Progress: {}/{} edges ({:.0} edges/sec)",
                    edge_index, config.edge_count, rate
                );
            }
        }

        // Final validation
        if !corruption_detected {
            validation_checks += 1;
            println!("Running final validation check...");
            match run_validation_check(&db_path, &node_ids, validation_checks) {
                Ok(_) => {
                    validations_passed += 1;
                    println!("✅ Final validation passed");
                }
                Err(e) => {
                    println!("❌ Final validation failed: {}", e);
                    corruption_detected = true;
                    error_details = Some(format!("Final validation failure: {}", e));
                }
            }
        }

        let duration = start_time.elapsed();
        let file_size = fs::metadata(&db_path)
            .expect("Failed to get final file size")
            .len();

        StressTestResult {
            config: config.clone(),
            duration_secs: duration.as_secs(),
            edges_inserted,
            validation_checks,
            validations_passed,
            file_size_bytes: file_size,
            corruption_detected,
            error_details,
        }
    }

    /// Run validation check on graph file
    fn run_validation_check(
        db_path: &Path,
        node_ids: &[i64],
        check_number: usize,
    ) -> Result<(), String> {
        // 1. Reopen graph and validate basic functionality
        let graph = open_graph(db_path, &GraphConfig::native())
            .map_err(|e| format!("Failed to reopen graph: {}", e))?;

        // 2. Test neighbor queries on sample nodes
        let sample_size = std::cmp::min(100, node_ids.len() / 10).max(1);
        let start_idx = (check_number * 37) % (node_ids.len() - sample_size);

        for i in 0..sample_size {
            let node_id = node_ids[start_idx + i];

            // Test outgoing neighbors
            let outgoing = graph
                .neighbors(SnapshotId::current(), 
                    node_id,
                    NeighborQuery {
                        direction: BackendDirection::Outgoing,
                        edge_type: None,
                    },
                )
                .map_err(|e| {
                    format!(
                        "Failed to get outgoing neighbors for node {}: {}",
                        node_id, e
                    )
                })?;

            // Validate neighbor IDs are valid
            for &neighbor_id in &outgoing {
                if !node_ids.contains(&neighbor_id) {
                    return Err(format!(
                        "Invalid neighbor ID {} found for node {}",
                        neighbor_id, node_id
                    ));
                }
            }

            // Test incoming neighbors
            let incoming = graph
                .neighbors(SnapshotId::current(), 
                    node_id,
                    NeighborQuery {
                        direction: BackendDirection::Incoming,
                        edge_type: None,
                    },
                )
                .map_err(|e| {
                    format!(
                        "Failed to get incoming neighbors for node {}: {}",
                        node_id, e
                    )
                })?;

            // Validate incoming neighbor IDs are valid
            for &neighbor_id in &incoming {
                if !node_ids.contains(&neighbor_id) {
                    return Err(format!(
                        "Invalid incoming neighbor ID {} found for node {}",
                        neighbor_id, node_id
                    ));
                }
            }
        }

        // 3. Test direct file access for V2 consistency
        validate_file_consistency(db_path)?;

        Ok(())
    }

    /// Validate file consistency using low-level file access
    fn validate_file_consistency(db_path: &Path) -> Result<(), String> {
        let mut graph_file =
            GraphFile::open(db_path).map_err(|e| format!("Failed to open graph file: {}", e))?;

        // Validate header
        let header = graph_file.header();
        if header.magic != [b'S', b'Q', b'L', b'T', b'G', b'F', 0, 0] {
            return Err("Invalid magic bytes in header".to_string());
        }

        if header.node_count < 0 || header.edge_count < 0 {
            return Err("Invalid node/edge counts in header".to_string());
        }

        // Validate basic file structure
        if header.node_data_offset < 1024 {
            return Err("Node data offset too small".to_string());
        }

        // Try to read a few node records
        if header.node_count > 0 {
            let node_count = header.node_count;
            let mut node_store = NodeStore::new(&mut graph_file);

            // Test reading first few nodes
            for i in 0..std::cmp::min(5, node_count as usize) {
                let node_id = (i + 1) as sqlitegraph::backend::native::types::NativeNodeId;
                if let Err(e) = node_store.read_node_v2(node_id) {
                    return Err(format!("Failed to read node {}: {}", i, e));
                }
            }
        }

        Ok(())
    }

    #[test]
    #[ignore]
    fn test_v2_file_consistency_validation() {
        if !should_run_stress_tests() {
            return;
        }

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("consistency_test.db");

        // Create a small graph for consistency testing
        let graph =
            open_graph(&db_path, &GraphConfig::native()).expect("Failed to create test graph");

        // Insert some nodes and edges
        let node_ids: Vec<i64> = (0..100)
            .map(|i| {
                graph
                    .insert_node(sqlitegraph::NodeSpec {
                        kind: "TestNode".to_string(),
                        name: format!("node_{}", i),
                        file_path: None,
                        data: serde_json::json!({"index": i}),
                    })
                    .expect("Failed to insert node")
            })
            .collect();

        // Insert edges
        for i in 0..200 {
            let from = node_ids[i % node_ids.len()];
            let to = node_ids[(i + 1) % node_ids.len()];

            graph
                .insert_edge(sqlitegraph::EdgeSpec {
                    from,
                    to,
                    edge_type: "test_edge".to_string(),
                    data: serde_json::json!({"index": i}),
                })
                .expect("Failed to insert edge");
        }

        // Run validation
        validate_file_consistency(&db_path).expect("File consistency validation failed");
    }
}
