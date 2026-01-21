//! Telemetry integration tests for Phase 37 gap analysis instrumentation.
//!
//! Tests validate that telemetry JSON export works correctly and includes
//! all required diagnostic fields for Chain(500) performance analysis.

use sqlitegraph::backend::native::graph_ops::TraversalContext;
use sqlitegraph::backend::native::types::NativeNodeId;

/// Test that telemetry JSON is valid and parseable.
#[test]
fn test_telemetry_json_format_valid() {
    let mut ctx = TraversalContext::new();

    // Simulate some traversal activity
    ctx.record_node_visit();
    ctx.record_node_visit();
    ctx.record_buffer_hit();
    ctx.record_buffer_miss();

    // Export telemetry
    let telemetry_json = ctx.export_telemetry();

    // Parse JSON to verify it's valid
    let telemetry: serde_json::Value = serde_json::from_str(&telemetry_json)
        .expect("Telemetry JSON should be valid");

    // Verify required fields are present
    assert!(telemetry.get("time_total_ms").is_some());
    assert!(telemetry.get("nodes_visited").is_some());
    assert!(telemetry.get("cluster_hits").is_some());
    assert!(telemetry.get("cluster_misses").is_some());
    assert!(telemetry.get("fragmentation_score").is_some());

    // Verify values match what we recorded
    assert_eq!(telemetry["nodes_visited"], 2);
    assert_eq!(telemetry["cluster_hits"], 1);
    assert_eq!(telemetry["cluster_misses"], 1);
}

/// Test that chain detection telemetry is populated correctly.
#[test]
fn test_telemetry_chain_detection() {
    let mut ctx = TraversalContext::new();

    // Simulate linear chain with contiguous clusters
    // Each cluster is 4096 bytes, starting at offset 0
    let cluster_size = 4096u64;
    for i in 0..10 {
        let offset = i * cluster_size;
        ctx.detector.observe_with_cluster(
            i as NativeNodeId,
            1, // degree 1 = linear
            offset,
            cluster_size as u32,
        );
    }

    // Record the detected chain
    ctx.detector.record_chain(10);

    // Export telemetry
    let telemetry_json = ctx.export_telemetry();
    let telemetry: serde_json::Value = serde_json::from_str(&telemetry_json)
        .expect("Telemetry JSON should be valid");

    // Verify chain detection metrics
    assert_eq!(telemetry["chains_detected"], 1);
    assert_eq!(telemetry["average_chain_length"], 10.0);

    // Contiguous clusters should have zero fragmentation
    assert_eq!(telemetry["fragmentation_score"], 0.0);
}

/// Test that fragmentation calculation works correctly for gaps.
#[test]
fn test_telemetry_fragmentation_calculation() {
    let mut ctx = TraversalContext::new();

    // Create clusters with gaps
    // Cluster 1: offset 0, size 100
    // Cluster 2: offset 200 (gap of 100 from expected offset 100)
    // Cluster 3: offset 300 (contiguous from cluster 2)
    ctx.detector.observe_with_cluster(1, 1, 0, 100);
    ctx.detector.observe_with_cluster(2, 1, 200, 100); // Gap!
    ctx.detector.observe_with_cluster(3, 1, 300, 100); // Contiguous

    // Export telemetry
    let telemetry_json = ctx.export_telemetry();
    let telemetry: serde_json::Value = serde_json::from_str(&telemetry_json)
        .expect("Telemetry JSON should be valid");

    // Verify fragmentation is > 0
    let fragmentation = telemetry["fragmentation_score"].as_f64()
        .expect("fragmentation_score should be a number");
    assert!(fragmentation > 0.0, "Fragmentation should be > 0 for gapped clusters");

    // Verify gap bytes matches expected gap
    // Gap between cluster 1 (ends at 100) and cluster 2 (starts at 200) = 100 bytes
    assert_eq!(telemetry["gap_bytes"], 100);
}

/// Test that timing fields are populated and numeric.
#[test]
fn test_telemetry_timing_fields_populated() {
    let mut ctx = TraversalContext::new();

    // Perform operations that trigger timing
    for i in 0i32..10 {
        ctx.detector.observe_with_cluster(i as NativeNodeId, 1, (i * 100) as u64, 100);
    }

    // Trigger contiguity validation
    let _ = ctx.detector.should_use_sequential_read();

    // Export telemetry
    let telemetry_json = ctx.export_telemetry();
    let telemetry: serde_json::Value = serde_json::from_str(&telemetry_json)
        .expect("Telemetry JSON should be valid");

    // Verify timing fields are present and numeric
    let linear_detection_ms = telemetry["linear_detection_ms"]
        .as_f64()
        .expect("linear_detection_ms should be a number");
    assert!(linear_detection_ms >= 0.0);

    let contiguity_validation_ms = telemetry["contiguity_validation_ms"]
        .as_f64()
        .expect("contiguity_validation_ms should be a number");
    assert!(contiguity_validation_ms >= 0.0);
}

/// Test telemetry for empty traversal (no activity).
#[test]
fn test_telemetry_empty_traversal() {
    let ctx = TraversalContext::new();

    // Export telemetry without any activity
    let telemetry_json = ctx.export_telemetry();
    let telemetry: serde_json::Value = serde_json::from_str(&telemetry_json)
        .expect("Telemetry JSON should be valid even for empty traversal");

    // Verify all fields are present with expected defaults
    assert_eq!(telemetry["nodes_visited"], 0);
    assert_eq!(telemetry["cluster_hits"], 0);
    assert_eq!(telemetry["cluster_misses"], 0);
    assert_eq!(telemetry["chains_detected"], 0);
    assert_eq!(telemetry["average_chain_length"], 0.0);
    assert_eq!(telemetry["fragmentation_score"], 0.0);
    assert_eq!(telemetry["gap_bytes"], 0);
    assert_eq!(telemetry["cluster_offsets_count"], 0);
}

/// Test combined hit rate calculation in telemetry.
#[test]
fn test_telemetry_combined_hit_rate() {
    let mut ctx = TraversalContext::new();

    // Record some hits and misses
    ctx.record_buffer_hit();
    ctx.record_buffer_hit();
    ctx.record_buffer_hit();
    ctx.record_buffer_miss();

    // Manually set L2 cache stats for testing
    ctx.stats.hits = 5;
    ctx.stats.misses = 2;

    // Export telemetry
    let telemetry_json = ctx.export_telemetry();
    let telemetry: serde_json::Value = serde_json::from_str(&telemetry_json)
        .expect("Telemetry JSON should be valid");

    // Combined hit rate: (3 L1 hits + 5 L2 hits) / (3 L1 hits + 1 L1 miss + 5 L2 hits + 2 L2 misses)
    // = 8 / 11 ≈ 0.727
    let combined_hit_rate = telemetry["combined_hit_rate"]
        .as_f64()
        .expect("combined_hit_rate should be a number");

    assert!((combined_hit_rate - 8.0 / 11.0).abs() < f64::EPSILON);
}

/// Test telemetry includes cluster buffer statistics.
#[test]
fn test_telemetry_cluster_buffer_stats() {
    let mut ctx = TraversalContext::new();

    // Record buffer corrections
    ctx.record_overshoot();
    ctx.record_undershoot();
    ctx.record_undershoot();
    ctx.record_buffer_realloc();

    // Export telemetry
    let telemetry_json = ctx.export_telemetry();
    let telemetry: serde_json::Value = serde_json::from_str(&telemetry_json)
        .expect("Telemetry JSON should be valid");

    // Verify buffer correction counts
    assert_eq!(telemetry["overshoot_count"], 1);
    assert_eq!(telemetry["undershoot_count"], 2);
    assert_eq!(telemetry["cluster_buffer_reallocs"], 1);
}

/// Test telemetry preserves L2 cache statistics.
#[test]
fn test_telemetry_l2_cache_stats() {
    let mut ctx = TraversalContext::new();

    // Manually set L2 cache stats for testing
    ctx.stats.hits = 42;
    ctx.stats.misses = 13;

    // Export telemetry
    let telemetry_json = ctx.export_telemetry();
    let telemetry: serde_json::Value = serde_json::from_str(&telemetry_json)
        .expect("Telemetry JSON should be valid");

    // Verify L2 cache stats are preserved
    assert_eq!(telemetry["l2_cache_hits"], 42);
    assert_eq!(telemetry["l2_cache_misses"], 13);
}
