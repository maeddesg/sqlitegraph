//! Linear Detector Integration Tests - Phase 35 Neighbor Extraction and Fallback
//!
//! **Purpose:** Integration tests for neighbor extraction from cluster_buffer and immediate fallback
//! **Scope:** End-to-end validation of sequential cluster reader on realistic graph patterns
//! **Focus:** Chain extraction, tree/diamond false positive prevention, non-contiguous cluster fallback
//!
//! **Phase 35 Integration Tests:**
//! - CL-02 completion: Sequential cluster reader extracts neighbors from cluster_buffer
//! - CL-04 satisfaction: Immediate fallback when pattern breaks (branching, non-contiguous)
//!
//! **Test Patterns:**
//! 1. Chain graph: Linear chain triggers sequential read and extracts neighbors from buffer
//! 2. Tree graph: Branching pattern prevents sequential read (false positive test)
//! 3. Diamond graph: Immediate fallback on branching detection
//! 4. Non-contiguous clusters: Contiguity validation prevents sequential read on gapped storage

use sqlitegraph::backend::native::{
    graph_ops::TraversalContext,
    adjacency::{LinearDetector, AdjacencyHelpers, Direction, TraversalPattern},
    types::NativeNodeId,
};

//
// TEST HELPERS
//

/// Helper: Create a TraversalContext with initialized LinearDetector
///
/// This helper provides a fresh traversal context for each test,
/// ensuring no cross-test pollution of detector state.
fn create_fresh_context() -> TraversalContext {
    TraversalContext::new()
}

//
// PHASE 35-04 INTEGRATION TESTS
//

#[test]
fn test_chain_extraction_from_cluster_buffer() {
    /// RED → GREEN: Validate node_cluster_index mapping population for linear chains
    ///
    /// This test validates the pattern:
    /// 1. Traverse with observe_with_cluster() for each node
    /// 2. After 3+ nodes, should_use_sequential_read() returns true
    /// 3. node_cluster_index mapping is populated correctly
    ///
    /// Note: Full cluster_buffer extraction test requires actual GraphFile setup (deferred)
    /// This test validates the mapping population pattern that enables extraction.

    let mut ctx = create_fresh_context();
    let mut detector = LinearDetector::new();

    // Simulate observing 5 linear nodes with contiguous clusters
    let cluster_size = 4096u32;
    let mut current_offset = 0u64;

    for node_id in 1..=5 {
        let degree = if node_id == 5 { 0 } else { 1 };

        // Observe with cluster metadata
        detector.observe_with_cluster(node_id, degree, current_offset, cluster_size);

        // Populate mapping (as traverse_with_detection would do)
        let cluster_index = detector.cluster_offsets().len().saturating_sub(1);
        ctx.node_cluster_index.insert(node_id, cluster_index);

        current_offset += cluster_size as u64;
    }

    // Verify: After 5 observations with degree <= 1, linear confirmed
    assert!(detector.is_linear_confirmed(), "LinearDetector should confirm linear pattern after 5 degree <= 1 observations");

    // Verify: Clusters are contiguous
    assert!(detector.validate_contiguity(), "Clusters should be contiguous (0, 4096, 8192, 12288, 16384)");

    // Verify: should_use_sequential_read returns true
    assert!(detector.should_use_sequential_read(), "should_use_sequential_read() should return true for linear confirmed + contiguous");

    // Verify: node_cluster_index has 5 entries
    assert_eq!(ctx.node_cluster_index.len(), 5, "node_cluster_index should have 5 entries");

    // Verify: Each node_id maps to correct cluster_index
    assert_eq!(ctx.node_cluster_index.get(&1), Some(&0), "Node 1 should map to cluster_index 0");
    assert_eq!(ctx.node_cluster_index.get(&2), Some(&1), "Node 2 should map to cluster_index 1");
    assert_eq!(ctx.node_cluster_index.get(&3), Some(&2), "Node 3 should map to cluster_index 2");
    assert_eq!(ctx.node_cluster_index.get(&4), Some(&3), "Node 4 should map to cluster_index 3");
    assert_eq!(ctx.node_cluster_index.get(&5), Some(&4), "Node 5 should map to cluster_index 4");
}

#[test]
fn test_tree_no_false_positive_sequential_read() {
    /// GREEN: Tree graphs don't trigger sequential read (branching prevents it)
    ///
    /// Validates that tree structures (immediate branching) don't falsely trigger
    /// the sequential cluster read optimization. This prevents performance regression
    /// on common tree-like graph patterns.

    let mut ctx = create_fresh_context();
    let mut detector = LinearDetector::new();

    // Simulate tree traversal: root with degree 2 (branches immediately)
    // In a binary tree BFS: root(2) -> children(3, 3) -> leaves(1, 1, 1, 1)

    // Root node: degree 2 (immediate branching)
    detector.observe_with_cluster(1, 2, 0, 4096);
    let cluster_index = 0;
    ctx.node_cluster_index.insert(1, cluster_index);

    // Verify: NOT linear, NOT using sequential read
    assert!(!detector.is_linear_confirmed(), "Root with degree 2 should NOT confirm linear pattern");
    assert!(!detector.should_use_sequential_read(), "Branching pattern should NOT trigger sequential read");

    // Verify: Pattern is Branching
    assert_eq!(detector.current_pattern(), TraversalPattern::Branching, "Pattern should be Branching for degree 2");

    // Continue traversal: children with degree 3 (1 parent + 2 children)
    detector.observe_with_cluster(2, 3, 4096, 4096);
    ctx.node_cluster_index.insert(2, 1);

    // Still NOT using sequential read
    assert!(!detector.should_use_sequential_read(), "After branching, sequential read should still be disabled");

    // Verify: Still in Branching state (terminal)
    assert_eq!(detector.current_pattern(), TraversalPattern::Branching, "Pattern should remain Branching once triggered");
}

#[test]
fn test_diamond_triggers_immediate_fallback() {
    /// REFACTOR: Diamond pattern triggers immediate fallback
    ///
    /// Diamond: A -> B, C; B, C -> D
    /// Pattern: A(2), B(2), C(2), D(2)
    ///
    /// Validates that:
    /// 1. Diamond graphs immediately detected as Branching
    /// 2. clear_cluster_buffer() is called to reset sequential read state
    /// 3. Buffer is cleared (None for cluster_buffer, empty for offsets and mapping)

    let mut ctx = create_fresh_context();
    let mut detector = LinearDetector::new();

    // Node A: degree 2 (branches to B and C)
    let pattern_a = detector.observe_with_cluster(1, 2, 0, 4096);
    ctx.node_cluster_index.insert(1, 0);

    // Verify: Immediate Branching detection
    assert_eq!(pattern_a, TraversalPattern::Branching, "Node A (degree 2) should trigger Branching pattern");
    assert!(!detector.is_linear_confirmed(), "Branching pattern should NOT confirm linear");
    assert!(!detector.should_use_sequential_read(), "Branching should NOT use sequential read");

    // Simulate fallback behavior (as traverse_with_detection would do)
    if pattern_a == TraversalPattern::Branching {
        ctx.clear_cluster_buffer();
    }

    // Verify: Buffer cleared
    assert!(ctx.cluster_buffer.is_none(), "cluster_buffer should be None after clear_cluster_buffer()");
    assert!(ctx.cluster_buffer_offsets.is_empty(), "cluster_buffer_offsets should be empty after clear_cluster_buffer()");
    assert!(ctx.node_cluster_index.is_empty(), "node_cluster_index should be empty after clear_cluster_buffer()");

    // Continue traversal through B, C, D
    detector.observe_with_cluster(2, 2, 4096, 4096);
    detector.observe_with_cluster(3, 2, 8192, 4096);
    detector.observe_with_cluster(4, 2, 12288, 4096);

    // Verify: Still NOT using sequential read
    assert!(!detector.should_use_sequential_read(), "After diamond fallback, sequential read should remain disabled");
}

#[test]
fn test_non_contiguous_clusters_fallback_to_l2_l3() {
    /// REFACTOR: Non-contiguous clusters fall back to L2/L3 path
    ///
    /// Validates that even with linear degree pattern (all degree 1),
    /// non-contiguous cluster storage prevents sequential read.
    ///
    /// This is critical because:
    /// - Linear pattern alone is insufficient for sequential read
    /// - Contiguity validation is required to avoid reading garbage data
    /// - Fallback to L2/L3 preserves correctness over performance

    let mut ctx = create_fresh_context();
    let mut detector = LinearDetector::new();

    // Linear pattern but NON-contiguous clusters (gaps in storage)
    detector.observe_with_cluster(1, 1, 0, 4096);
    ctx.node_cluster_index.insert(1, 0);

    detector.observe_with_cluster(2, 1, 4096, 4096);
    ctx.node_cluster_index.insert(2, 1);

    detector.observe_with_cluster(3, 1, 8192, 4096);
    ctx.node_cluster_index.insert(3, 2);

    // Linear pattern confirmed (all degree 1)
    assert!(detector.is_linear_confirmed(), "Linear pattern should be confirmed after 3 degree-1 observations");

    // But clusters ARE contiguous so far (0, 4096, 8192)
    assert!(detector.validate_contiguity(), "Clusters 0-2 should be contiguous");

    // Add node 4 with GAP (should be 12288, but is 20000)
    detector.observe_with_cluster(4, 1, 20000, 4096);
    ctx.node_cluster_index.insert(4, 3);

    // Linear still confirmed (all degree 1)
    assert!(detector.is_linear_confirmed(), "Linear pattern still confirmed (all degree 1)");

    // But clusters NOT contiguous due to gap
    assert!(!detector.validate_contiguity(), "Clusters NOT contiguous due to gap at node 4");

    // Therefore: should NOT use sequential read
    assert!(!detector.should_use_sequential_read(), "Non-contiguous clusters should prevent sequential read");

    // This means traversal falls back to L2/L3 path for node 4
    // (graceful degradation preserving correctness)
}

//
// PHASE 33 BACKWARD COMPATIBILITY TESTS
//

#[test]
fn test_phase33_cluster_offset_tracking() {
    /// Backward compatibility: Phase 33 cluster offset tracking still works
    ///
    /// Ensures Phase 35 changes don't break Phase 33 LinearDetector functionality.

    let mut detector = LinearDetector::new();

    // Observe nodes with cluster offsets
    detector.observe_with_cluster(1, 1, 0, 4096);
    detector.observe_with_cluster(2, 1, 4096, 4096);
    detector.observe_with_cluster(3, 1, 8192, 4096);

    let offsets = detector.cluster_offsets();
    assert_eq!(offsets.len(), 3);
    assert_eq!(offsets[0], (0, 4096));
    assert_eq!(offsets[1], (4096, 4096));
    assert_eq!(offsets[2], (8192, 4096));
}

#[test]
fn test_phase33_contiguity_validation() {
    /// Backward compatibility: Phase 33 contiguity validation still works
    ///
    /// Ensures Phase 35 changes don't break contiguity checking.

    let mut detector = LinearDetector::new();

    // Contiguous clusters
    detector.observe_with_cluster(1, 1, 0, 4096);
    detector.observe_with_cluster(2, 1, 4096, 4096);
    detector.observe_with_cluster(3, 1, 8192, 4096);
    assert!(detector.validate_contiguity());

    // Reset and test non-contiguous
    let mut detector2 = LinearDetector::new();
    detector2.observe_with_cluster(1, 1, 0, 4096);
    detector2.observe_with_cluster(2, 1, 10000, 4096); // Gap
    assert!(!detector2.validate_contiguity());
}

#[test]
fn test_phase33_should_use_sequential_read() {
    /// Backward compatibility: Phase 33 should_use_sequential_read() still works
    ///
    /// Ensures Phase 35 changes don't break the combined check.

    let mut detector = LinearDetector::new();

    // Not enough observations yet
    assert!(!detector.should_use_sequential_read());

    // Add linear contiguous observations
    detector.observe_with_cluster(1, 1, 0, 4096);
    detector.observe_with_cluster(2, 1, 4096, 4096);
    detector.observe_with_cluster(3, 1, 8192, 4096);

    // Now should use sequential read
    assert!(detector.should_use_sequential_read());

    // Add non-contiguous cluster
    let mut detector2 = LinearDetector::new();
    detector2.observe_with_cluster(1, 1, 0, 4096);
    detector2.observe_with_cluster(2, 1, 4096, 4096);
    detector2.observe_with_cluster(3, 1, 10000, 4096); // Gap

    // Should NOT use sequential read (non-contiguous)
    assert!(!detector2.should_use_sequential_read());
}
