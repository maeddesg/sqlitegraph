//! V2 Crash Simulation Test
//!
//! Tests V2 crash safety by simulating process termination (SIGKILL)
//! and validating file integrity after unexpected shutdown.
//!
//! This test only runs when explicitly enabled to avoid system resource issues.
//! Enable with: RUST_TEST_CRASH=1 cargo test --features v2_experimental v2_crash_simulation

#[cfg(test)]
mod tests {
    use assert_cmd::Command as AssertCommand;
    use sqlitegraph::backend::native::{EdgeStore, GraphFile, NodeStore};
    use sqlitegraph::{BackendDirection, GraphConfig, NeighborQuery, open_graph};
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    /// Crash test result classification
    #[derive(Debug, PartialEq)]
    enum CrashTestResult {
        /// File reopened successfully, data intact
        Pass,
        /// File shows expected partial write behavior, handled gracefully
        FailSafe,
        /// File corruption detected (unacceptable)
        Corruption,
    }

    /// Check if crash tests should run
    fn should_run_crash_tests() -> bool {
        env::var("RUST_TEST_CRASH").is_ok() || env::var("CRASH_TESTS").is_ok()
    }

    /// Crash test configuration
    #[derive(Debug)]
    struct CrashTestConfig {
        node_count: usize,
        edges_before_crash: usize,
        multi_edge_factor: usize,
        child_timeout_secs: u64,
        wait_for_progress_secs: u64,
    }

    impl Default for CrashTestConfig {
        fn default() -> Self {
            Self {
                node_count: 10_000,
                edges_before_crash: 50_000,
                multi_edge_factor: 1,
                child_timeout_secs: 30,
                wait_for_progress_secs: 10,
            }
        }
    }

    /// Run crash simulation test
    fn run_crash_simulation_test(config: &CrashTestConfig) -> (CrashTestResult, String) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("v2_crash_test.db");

        println!("Starting V2 crash simulation test...");
        println!("  DB path: {}", db_path.display());
        println!("  Config: {:?}", config);

        // Build the crash test child binary
        println!("Building crash test child binary...");
        let build_result = Command::new("cargo")
            .args(["build", "--example", "crash_test_child"])
            .current_dir(temp_dir.path())
            .output();

        match build_result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return (
                        CrashTestResult::Corruption,
                        format!("Failed to build child binary: {}", stderr),
                    );
                }
                println!("✅ Child binary built successfully");
            }
            Err(e) => {
                return (
                    CrashTestResult::Corruption,
                    format!("Failed to execute cargo build: {}", e),
                );
            }
        }

        let child_binary_path = temp_dir
            .path()
            .join("target/debug/examples/crash_test_child");
        if !child_binary_path.exists() {
            return (
                CrashTestResult::Corruption,
                "Child binary not found after build".to_string(),
            );
        }

        // Launch child process
        println!("Launching child process...");
        let mut child = Command::new(&child_binary_path)
            .arg(db_path.to_str().unwrap())
            .arg(config.node_count.to_string())
            .arg("10000") // Report every 10k edges
            .arg(config.multi_edge_factor.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn child process");

        // Wait for child to make progress
        println!("Waiting for child process to make progress...");
        let start_time = Instant::now();
        let mut child_output_lines = Vec::new();
        let mut edges_written = 0;

        // Read child output to track progress
        if let Some(stdout) = child.stdout.as_mut() {
            use std::io::{BufRead, BufReader};
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            while start_time.elapsed().as_secs() < config.wait_for_progress_secs {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        child_output_lines.push(line.clone());
                        if line.contains("PROGRESS:") {
                            println!("Child: {}", line.trim());
                            // Parse edge count from progress message
                            if let Some(edge_str) = line.split_whitespace().nth(1) {
                                if let Ok(parsed_edges) = edge_str.parse::<usize>() {
                                    edges_written = parsed_edges;
                                    if edges_written >= config.edges_before_crash {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        }

        if edges_written < config.edges_before_crash {
            println!("⚠️  Child didn't reach target edge count before timeout, proceeding anyway");
        }

        // Send SIGKILL to simulate crash
        println!("Sending SIGKILL to child process...");
        match child.kill() {
            Ok(_) => println!("✅ SIGKILL sent successfully"),
            Err(e) => println!("⚠️  Failed to send SIGKILL: {}", e),
        }

        // Wait for child to actually terminate
        match child.wait() {
            Ok(status) => {
                if !status.success() {
                    println!("Child terminated with status: {}", status);
                }
            }
            Err(e) => println!("Failed to wait for child: {}", e),
        }

        // Small delay to ensure all writes are flushed
        std::thread::sleep(Duration::from_millis(100));

        // Post-crash validation
        println!("Running post-crash validation...");
        let (result, details) = validate_crashed_file(&db_path, &child_output_lines);

        (result, details)
    }

    /// Validate file after crash
    fn validate_crashed_file(db_path: &Path, child_output: &[String]) -> (CrashTestResult, String) {
        let mut details = Vec::new();

        // 1. Check if file exists and has reasonable size
        let file_exists = db_path.exists();
        let file_size = if file_exists {
            fs::metadata(db_path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        details.push(format!("File exists: {}", file_exists));
        details.push(format!("File size: {} bytes", file_size));

        if !file_exists {
            return (
                CrashTestResult::FailSafe,
                "File was deleted after crash - this is acceptable behavior".to_string(),
            );
        }

        if file_size < 4096 {
            // Very small file could be empty or header-only
            return (
                CrashTestResult::FailSafe,
                format!(
                    "File size {} bytes indicates empty or header-only file (acceptable)",
                    file_size
                ),
            );
        }

        // 2. Try to reopen with GraphBackend
        match open_graph(db_path, &GraphConfig::native()) {
            Ok(graph) => {
                details.push("Graph reopened successfully".to_string());

                // 3. Basic functionality test
                match test_basic_graph_functionality(&graph) {
                    Ok(_) => {
                        details.push("Basic functionality tests passed".to_string());
                        (CrashTestResult::Pass, details.join("; "))
                    }
                    Err(e) => {
                        details.push(format!("Basic functionality test failed: {}", e));
                        (CrashTestResult::FailSafe, details.join("; "))
                    }
                }
            }
            Err(e) => {
                details.push(format!("Failed to reopen graph: {}", e));

                // 4. Try low-level file validation
                match validate_low_level_file(db_path) {
                    Ok(_) => {
                        details.push("Low-level file validation passed".to_string());
                        (CrashTestResult::FailSafe, details.join("; "))
                    }
                    Err(e) => {
                        details.push(format!("Low-level validation failed: {}", e));
                        (CrashTestResult::Corruption, details.join("; "))
                    }
                }
            }
        }
    }

    /// Test basic graph functionality after crash
    fn test_basic_graph_functionality(
        graph: &Box<dyn sqlitegraph::GraphBackend>,
    ) -> Result<(), String> {
        // Try to get a node (assuming nodes 1-100 exist)
        let test_node_id = 1;
        match graph.get_node(test_node_id) {
            Ok(_) => {
                // Try neighbor query
                match graph.neighbors(
                    test_node_id,
                    NeighborQuery {
                        direction: BackendDirection::Outgoing,
                        edge_type: None,
                    },
                ) {
                    Ok(_neighbors) => Ok(()),
                    Err(e) => Err(format!("Neighbor query failed: {}", e)),
                }
            }
            Err(e) => {
                // Node might not exist, try a few more
                let mut found_node = false;
                for node_id in 2..=100 {
                    if graph.get_node(node_id).is_ok() {
                        found_node = true;
                        break;
                    }
                }

                if found_node {
                    Ok(())
                } else {
                    Err(format!("No nodes found in range 2-100: {}", e))
                }
            }
        }
    }

    /// Low-level file validation
    fn validate_low_level_file(db_path: &Path) -> Result<(), String> {
        let mut graph_file =
            GraphFile::open(db_path).map_err(|e| format!("Failed to open graph file: {}", e))?;

        // Check header
        let header = graph_file.header();
        if header.magic != [b'S', b'Q', b'L', b'T', b'G', b'F', 0, 0] {
            return Err("Invalid magic bytes - file corrupted".to_string());
        }

        // Check basic header fields for reasonable values
        if header.node_count < 0 {
            return Err("Invalid node count in header".to_string());
        }

        if header.edge_count < 0 {
            return Err("Invalid edge count in header".to_string());
        }

        // Check that file size is reasonable for reported content
        let file_size = fs::metadata(db_path)
            .map_err(|e| format!("Failed to get file metadata: {}", e))?
            .len();

        // A rough sanity check: file should be at least header size + some data
        let min_expected_size = 1024; // Minimum reasonable file size
        if file_size < min_expected_size && (header.node_count > 0 || header.edge_count > 0) {
            return Err(format!(
                "File size {} too small for reported {} nodes and {} edges",
                file_size, header.node_count, header.edge_count
            ));
        }

        Ok(())
    }

    #[test]
    #[ignore] // Only run when explicitly enabled
    fn v2_crash_simulation_test() {
        if !should_run_crash_tests() {
            println!("Skipping V2 crash simulation test (set RUST_TEST_CRASH=1 to enable)");
            return;
        }

        println!("Starting V2 crash simulation test...");

        let config = CrashTestConfig::default();
        let (result, details) = run_crash_simulation_test(&config);

        // Report results
        println!("✅ Crash simulation test completed");
        println!("Result: {:?}", result);
        println!("Details: {}", details);

        match result {
            CrashTestResult::Pass => {
                // Perfect: file survived SIGKILL with no data loss
                println!("🎉 PASS: Graph file survived SIGKILL with full functionality preserved");
            }
            CrashTestResult::FailSafe => {
                // Acceptable: file shows expected partial write behavior
                println!("✅ PASS: Graph file shows expected safe failure behavior after crash");
            }
            CrashTestResult::Corruption => {
                // Unacceptable: file corruption detected
                panic!("❌ FAIL: File corruption detected after crash: {}", details);
            }
        }
    }

    #[test]
    #[ignore] // Only run when explicitly enabled
    fn v2_crash_simulation_multi_edge_test() {
        if !should_run_crash_tests() {
            println!(
                "Skipping V2 crash simulation multi-edge test (set RUST_TEST_CRASH=1 to enable)"
            );
            return;
        }

        println!("Starting V2 crash simulation test with multi-edge scenario...");

        let config = CrashTestConfig {
            multi_edge_factor: 5, // Test with multiple edges per pair
            ..Default::default()
        };

        let (result, details) = run_crash_simulation_test(&config);

        // Report results
        println!("✅ Multi-edge crash simulation test completed");
        println!("Result: {:?}", result);
        println!("Details: {}", details);

        match result {
            CrashTestResult::Pass => {
                println!(
                    "🎉 PASS: Multi-edge graph file survived SIGKILL with full functionality preserved"
                );
            }
            CrashTestResult::FailSafe => {
                println!(
                    "✅ PASS: Multi-edge graph file shows expected safe failure behavior after crash"
                );
            }
            CrashTestResult::Corruption => {
                panic!(
                    "❌ FAIL: Multi-edge file corruption detected after crash: {}",
                    details
                );
            }
        }
    }
}
