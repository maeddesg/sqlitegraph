//! Prefetch Window Tuning Tests for Sequential I/O Optimization
//!
//! **Purpose:** Benchmark different prefetch window sizes (4, 8, 16 slots)
//! **Focus:** Chain traversal performance with different buffer configurations
//!
//! This test module measures the impact of different prefetch window sizes on
//! chain traversal performance. The SequentialReadBuffer can be configured with
//! different window sizes:
//!
//! - 4 slots (16KB): Lower memory, less effective for long chains
//! - 8 slots (32KB): Default, balanced for most workloads
//! - 16 slots (64KB): Higher memory, better for very long chains
//!
//! **NOTE:** As of Phase 32-01, L1 buffer neighbor extraction is instrumentation-only.
//! Full neighbor extraction from buffered NodeRecordV2 is deferred to Plan 32-04.
//! Therefore, these tests measure buffer operations directly rather than end-to-end
//! traversal performance with different window sizes.

use sqlitegraph::backend::native::{
    graph_file::GraphFile,
    node_store::NodeStore,
    edge_store::EdgeStore,
    graph_ops::{native_bfs, TraversalContext, TraversalCache, TraversalCacheStats},
    adjacency::{LinearDetector, SequentialReadBuffer},
    NativeNodeId,
};
use std::time::Instant;
use tempfile::TempDir;

//
// TEST HELPERS
//

/// Helper: Create a linear chain graph with specified node count
///
/// Creates a linear chain: 0 -> 1 -> 2 -> ... -> (n-1)
/// This topology is optimal for sequential I/O optimization.
///
/// Parameters:
/// - node_count: Number of nodes in the chain
///
/// Returns:
/// - GraphFile: The native graph file
/// - Vec<NativeNodeId>: Node IDs [0, 1, 2, ..., n-1]
/// - TempDir: Temporary directory (kept for cleanup)
fn create_chain_graph(node_count: usize) -> (GraphFile, Vec<NativeNodeId>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_chain.db");

    let mut graph_file = GraphFile::create(&db_path).expect("Failed to create graph file");

    // Create nodes
    let mut node_ids = Vec::with_capacity(node_count);
    for i in 0..node_count {
        let mut node_store = NodeStore::new(&mut graph_file);
        let node_id = node_store
            .allocate_node_id()
            .expect("Failed to allocate node ID");
        let record = sqlitegraph::backend::native::NodeRecord::new(
            node_id,
            "Node".to_string(),
            format!("node_{}", i),
            serde_json::json!({"id": i}),
        );
        node_store.write_node(&record).expect("Failed to write node");
        node_ids.push(node_id);
    }

    // Create chain edges: 0->1, 1->2, ..., (n-2)->(n-1)
    let mut edge_store = EdgeStore::new(&mut graph_file);
    for i in 0..node_count.saturating_sub(1) {
        let edge = sqlitegraph::backend::native::EdgeRecord::new(
            i as i64 + 1, // edge_id
            node_ids[i],   // from node i
            node_ids[i + 1], // to node i+1
            "chain".to_string(),
            serde_json::json!({"order": i}),
        );
        edge_store
            .write_edge(&edge)
            .expect("Failed to write chain edge");
    }

    (graph_file, node_ids, temp_dir)
}

/// Helper: Measure buffer prefetch operation timing
///
/// Measures the time to prefetch a window of node slots.
/// This directly tests the SequentialReadBuffer prefetch operation
/// without relying on full neighbor extraction.
///
/// Parameters:
/// - graph_file: The native graph file
/// - start_node: Starting node ID for prefetch
/// - prefetch_window: Buffer prefetch window size (4, 8, 16, etc.)
///
/// Returns:
/// - Duration: Prefetch time
/// - usize: Number of nodes cached after prefetch
fn measure_prefetch_timing(
    graph_file: &mut GraphFile,
    start_node: NativeNodeId,
    prefetch_window: usize,
) -> (std::time::Duration, usize) {
    let mut buffer = SequentialReadBuffer::with_prefetch_window(prefetch_window);

    let start_time = Instant::now();
    let _ = buffer.prefetch_from(graph_file, start_node);
    let duration = start_time.elapsed();

    let cached_count = buffer.len();

    (duration, cached_count)
}

//
// WINDOW SIZE TESTS
//

#[test]
fn test_prefetch_window_4_chain_500() {
    let node_count = 500;
    let (mut graph_file, node_ids, _temp_dir) = create_chain_graph(node_count);

    let start_node = node_ids[0];

    println!("\n=== Prefetch Window 4: Chain({}) ===", node_count);
    let (duration, cached_count) = measure_prefetch_timing(&mut graph_file, start_node, 4);

    println!("Window 4 prefetch: {:?}", duration);
    println!("  Cached nodes: {} (expected: ~4)", cached_count);
    println!("  Throughput: {:.2} nodes/sec", cached_count as f64 / duration.as_secs_f64());

    // Verify prefetch cached nodes
    assert!(cached_count > 0, "Prefetch should cache some nodes");
    assert!(cached_count <= 4, "Window 4 should cache at most 4 nodes");
}

#[test]
fn test_prefetch_window_8_chain_500() {
    let node_count = 500;
    let (mut graph_file, node_ids, _temp_dir) = create_chain_graph(node_count);

    let start_node = node_ids[0];

    println!("\n=== Prefetch Window 8: Chain({}) ===", node_count);
    let (duration, cached_count) = measure_prefetch_timing(&mut graph_file, start_node, 8);

    println!("Window 8 prefetch: {:?}", duration);
    println!("  Cached nodes: {} (expected: ~8)", cached_count);
    println!("  Throughput: {:.2} nodes/sec", cached_count as f64 / duration.as_secs_f64());

    // Verify prefetch cached nodes
    assert!(cached_count > 0, "Prefetch should cache some nodes");
    assert!(cached_count <= 8, "Window 8 should cache at most 8 nodes");
}

#[test]
fn test_prefetch_window_16_chain_500() {
    let node_count = 500;
    let (mut graph_file, node_ids, _temp_dir) = create_chain_graph(node_count);

    let start_node = node_ids[0];

    println!("\n=== Prefetch Window 16: Chain({}) ===", node_count);
    let (duration, cached_count) = measure_prefetch_timing(&mut graph_file, start_node, 16);

    println!("Window 16 prefetch: {:?}", duration);
    println!("  Cached nodes: {} (expected: ~16)", cached_count);
    println!("  Throughput: {:.2} nodes/sec", cached_count as f64 / duration.as_secs_f64());

    // Verify prefetch cached nodes
    assert!(cached_count > 0, "Prefetch should cache some nodes");
    assert!(cached_count <= 16, "Window 16 should cache at most 16 nodes");
}

//
// COMPARISON TEST
//

#[test]
fn test_prefetch_window_comparison_chain_500() {
    let node_count = 500;

    println!("\n=== Prefetch Window Comparison: Chain({}) ===", node_count);

    // Test window 4
    let (mut graph_file_4, node_ids_4, _temp_dir_4) = create_chain_graph(node_count);
    let start_node = node_ids_4[0];
    let (duration_4, cached_4) = measure_prefetch_timing(&mut graph_file_4, start_node, 4);

    // Test window 8
    let (mut graph_file_8, node_ids_8, _temp_dir_8) = create_chain_graph(node_count);
    let start_node_8 = node_ids_8[0];
    let (duration_8, cached_8) = measure_prefetch_timing(&mut graph_file_8, start_node_8, 8);

    // Test window 16
    let (mut graph_file_16, node_ids_16, _temp_dir_16) = create_chain_graph(node_count);
    let start_node_16 = node_ids_16[0];
    let (duration_16, cached_16) =
        measure_prefetch_timing(&mut graph_file_16, start_node_16, 16);

    // Print comparison table
    println!("\n=== Timing Comparison ===");
    println!("Window 4:  {:?} (cached: {})", duration_4, cached_4);
    println!("Window 8:  {:?} (cached: {})", duration_8, cached_8);
    println!("Window 16: {:?} (cached: {})", duration_16, cached_16);

    // Calculate timing ratios
    let ratio_4_to_8 = duration_4.as_nanos() as f64 / duration_8.as_nanos() as f64;
    let ratio_16_to_8 = duration_16.as_nanos() as f64 / duration_8.as_nanos() as f64;

    println!("\n=== Timing Ratios (relative to window 8) ===");
    println!("Window 4 vs 8:  {:.3}x", ratio_4_to_8);
    println!("Window 16 vs 8: {:.3}x", ratio_16_to_8);

    // Verify all prefetch operations cached nodes
    assert!(cached_4 > 0);
    assert!(cached_8 > 0);
    assert!(cached_16 > 0);

    // Larger windows should cache more nodes
    assert!(cached_4 <= cached_8, "Window 8 should cache >= nodes as window 4");
    assert!(cached_8 <= cached_16, "Window 16 should cache >= nodes as window 8");
}

//
// MEMORY OVERHEAD TESTS
//

#[test]
fn test_memory_overhead_per_buffer_size() {
    println!("\n=== SequentialReadBuffer Memory Overhead ===");

    // Create buffers with different window sizes
    let buffer_4 = SequentialReadBuffer::with_prefetch_window(4);
    let buffer_8 = SequentialReadBuffer::with_prefetch_window(8);
    let buffer_16 = SequentialReadBuffer::with_prefetch_window(16);

    // Each buffer starts empty, so base overhead is minimal
    println!("Empty buffer overhead:");
    println!("  Window 4:  len={}, is_empty={}", buffer_4.len(), buffer_4.is_empty());
    println!("  Window 8:  len={}, is_empty={}", buffer_8.len(), buffer_8.is_empty());
    println!("  Window 16: len={}, is_empty={}", buffer_16.len(), buffer_16.is_empty());

    // Verify empty buffers have no entries
    assert_eq!(buffer_4.len(), 0);
    assert_eq!(buffer_8.len(), 0);
    assert_eq!(buffer_16.len(), 0);
}

#[test]
fn test_memory_overhead_full_buffer() {
    println!("\n=== SequentialReadBuffer Full Buffer Memory ===");

    // Create a buffer with window 8 (default)
    let mut buffer = SequentialReadBuffer::with_prefetch_window(8);

    // Simulate filling buffer with NodeRecordV2 entries
    // Each NodeRecordV2 is approximately 150-200 bytes depending on data
    use sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2;

    let mut total_estimated_bytes = 0;
    for i in 1..=8 {
        let node = NodeRecordV2::new(
            i,
            "TestNode".to_string(),
            format!("node_{}", i),
            serde_json::json!({"data": "x".repeat(50)}), // ~50 bytes of JSON
        );
        // Estimate NodeRecordV2 size: kind (20) + name (20) + data (100) + overhead
        let estimated_size = 200; // Conservative estimate
        total_estimated_bytes += estimated_size;
        buffer.insert(node);
    }

    println!("Full buffer (8 slots):");
    println!("  Entries: {}", buffer.len());
    println!("  Estimated memory: ~{} bytes", total_estimated_bytes);
    println!("  Per-slot average: ~{} bytes", total_estimated_bytes / buffer.len());

    // Verify buffer is full
    assert_eq!(buffer.len(), 8);
    assert!(!buffer.is_empty());

    // Clear and verify empty
    buffer.clear();
    assert_eq!(buffer.len(), 0);
    assert!(buffer.is_empty());
}

//
// SMALLER CHAIN TESTS
//

#[test]
fn test_prefetch_window_sizes_chain_100() {
    let node_count = 100;

    println!("\n=== Prefetch Window Sizes: Chain({}) ===", node_count);

    // For smaller chains, prefetch window has less impact
    let windows = [4, 8, 16];
    let mut results = Vec::new();

    for &window in &windows {
        let (mut graph_file, node_ids, _temp_dir) = create_chain_graph(node_count);
        let start_node = node_ids[0];

        let (duration, cached) = measure_prefetch_timing(&mut graph_file, start_node, window);
        results.push((window, duration, cached));

        println!("Window {}: {:?} (cached: {})", window, duration, cached);
    }

    // Verify all windows cached some nodes
    for (_window, _duration, cached) in &results {
        assert!(*cached > 0, "Prefetch should cache nodes");
    }

    // Larger windows should cache more nodes
    assert!(results[0].2 <= results[1].2); // 4 <= 8
    assert!(results[1].2 <= results[2].2); // 8 <= 16
}

//
// BUFFER CONTAINMENT TESTS
//

#[test]
fn test_buffer_contains_after_prefetch() {
    let node_count = 20;
    let (mut graph_file, node_ids, _temp_dir) = create_chain_graph(node_count);

    let start_node = node_ids[0];

    // Prefetch with window 8
    let mut buffer = SequentialReadBuffer::with_prefetch_window(8);
    buffer
        .prefetch_from(&mut graph_file, start_node)
        .expect("Prefetch should succeed");

    // Buffer should contain the start node
    assert!(buffer.contains(start_node), "Buffer should contain start node");

    // Buffer should have some nodes cached
    assert!(buffer.len() > 0, "Buffer should have cached nodes");
    assert!(buffer.len() <= 8, "Buffer should not exceed window size");

    println!("\n=== Buffer Containment After Prefetch ===");
    println!("  Start node: {}", start_node);
    println!("  Cached nodes: {}", buffer.len());
    println!("  Contains start: {}", buffer.contains(start_node));
}

//
// NEXT PREFETCH TRACKING TESTS
//

#[test]
fn test_next_prefetch_start_tracking() {
    let node_count = 20;
    let (mut graph_file, node_ids, _temp_dir) = create_chain_graph(node_count);

    let start_node = node_ids[0];

    // Prefetch with window 8
    let mut buffer = SequentialReadBuffer::with_prefetch_window(8);
    buffer
        .prefetch_from(&mut graph_file, start_node)
        .expect("Prefetch should succeed");

    // Check next prefetch start is tracked
    let next_start = buffer.next_prefetch_start();
    assert!(next_start.is_some(), "Next prefetch start should be tracked");

    // Next prefetch should be start_node + window
    let expected_next = start_node + 8;
    assert_eq!(
        next_start.unwrap(), expected_next,
        "Next prefetch should be at start + window"
    );

    println!("\n=== Next Prefetch Start Tracking ===");
    println!("  Start node: {}", start_node);
    println!("  Window size: 8");
    println!("  Next prefetch start: {}", next_start.unwrap());
    println!("  Expected next: {}", expected_next);
}

//
// VERIFICATION TESTS
//

#[test]
fn test_buffer_window_configuration() {
    // Verify that buffers correctly store their window size
    let buffer_4 = SequentialReadBuffer::with_prefetch_window(4);
    let buffer_8 = SequentialReadBuffer::with_prefetch_window(8);
    let buffer_16 = SequentialReadBuffer::with_prefetch_window(16);

    assert_eq!(buffer_4.prefetch_window(), 4);
    assert_eq!(buffer_8.prefetch_window(), 8);
    assert_eq!(buffer_16.prefetch_window(), 16);

    // Default buffer should have window 8
    let buffer_default = SequentialReadBuffer::new();
    assert_eq!(buffer_default.prefetch_window(), 8);

    println!("\nBuffer window configuration verified:");
    println!("  with_prefetch_window(4) -> {}", buffer_4.prefetch_window());
    println!("  with_prefetch_window(8) -> {}", buffer_8.prefetch_window());
    println!("  with_prefetch_window(16) -> {}", buffer_16.prefetch_window());
    println!("  SequentialReadBuffer::new() -> {}", buffer_default.prefetch_window());
}

#[test]
fn test_traversal_context_custom_buffer() {
    // Verify that TraversalContext can be constructed with custom buffer
    let custom_buffer = SequentialReadBuffer::with_prefetch_window(16);
    let ctx = TraversalContext {
        detector: LinearDetector::new(),
        buffer: custom_buffer,
        cache: TraversalCache::new(),
        stats: TraversalCacheStats::new(),
        buffer_hits: 0,
        buffer_misses: 0,
    };

    assert_eq!(ctx.buffer.prefetch_window(), 16);
    assert_eq!(ctx.buffer_hits, 0);
    assert_eq!(ctx.buffer_misses, 0);
    assert_eq!(ctx.combined_hit_rate(), 0.0);

    println!("\nTraversalContext custom buffer configuration verified:");
    println!("  Custom window 16 buffer works correctly");
}

//
// NATIVE BFS TEST (with default window)
//

#[test]
fn test_native_bfs_chain_500_with_default_window() {
    let node_count = 500;
    let (mut graph_file, node_ids, _temp_dir) = create_chain_graph(node_count);

    let start_node = node_ids[0];
    let depth = node_count as u32;

    println!("\n=== Native BFS with Default Window 8: Chain({}) ===", node_count);

    let start_time = Instant::now();
    let result = native_bfs(&mut graph_file, start_node, depth);
    let duration = start_time.elapsed();

    assert!(result.is_ok(), "BFS should succeed");
    let visited = result.unwrap();

    println!("Native BFS: {:?}", duration);
    println!("  Visited: {} nodes (expected: {})", visited.len(), node_count);
    println!("  Throughput: {:.2} nodes/sec", node_count as f64 / duration.as_secs_f64());

    // Note: As of Phase 32-01, L1 buffer neighbor extraction is instrumentation-only
    // This test validates that native_bfs works correctly with default window=8
    // Note: BFS excludes the start node from visited list, so we expect node_count - 1
    assert_eq!(visited.len(), node_count - 1, "BFS should visit all reachable nodes (excluding start)");
}

//
// BUFFER CLEAR TESTS
//

#[test]
fn test_buffer_clear_operations() {
    let mut buffer = SequentialReadBuffer::with_prefetch_window(8);

    // Insert some nodes
    use sqlitegraph::backend::native::v2::node_record_v2::NodeRecordV2;
    for i in 1..=5 {
        buffer.insert(NodeRecordV2::new(
            i,
            "Test".to_string(),
            format!("node_{}", i),
            serde_json::json!({}),
        ));
    }

    assert_eq!(buffer.len(), 5);

    // Clear buffer
    buffer.clear();

    assert_eq!(buffer.len(), 0);
    assert!(buffer.is_empty());
    assert!(buffer.next_prefetch_start().is_none());

    println!("\nBuffer clear operations verified:");
    println!("  Clear removes all entries and resets tracking");
}
