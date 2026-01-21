//! Tests for adjacency iteration functionality

use super::{AdjacencyHelpers, AdjacencyIterator};
use crate::backend::native::graph_file::GraphFile;
use crate::backend::native::node_store::NodeStore;
use crate::backend::native::types::*;

#[cfg(test)]
fn create_test_graph_file() -> (GraphFile, tempfile::NamedTempFile) {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let path = temp_file.path();
    let graph_file = GraphFile::create(path).unwrap();
    (graph_file, temp_file)
}

#[test]
fn test_adjacency_iterator_empty() {
    let (mut graph_file, _temp_file) = create_test_graph_file();

    // Create a node with no edges
    let node = NodeRecord::new(
        1,
        "Test".to_string(),
        "node1".to_string(),
        serde_json::json!({}),
    );
    {
        let mut node_store = NodeStore::new(&mut graph_file);
        node_store.write_node(&node).unwrap();
    }

    // Test outgoing iterator
    let iterator = AdjacencyIterator::new_outgoing(&mut graph_file, 1).unwrap();
    assert_eq!(iterator.total_count(), 0);
    assert!(iterator.is_complete());

    // Test incoming iterator
    let iterator = AdjacencyIterator::new_incoming(&mut graph_file, 1).unwrap();
    assert_eq!(iterator.total_count(), 0);
    assert!(iterator.is_complete());
}

#[test]
fn test_adjacency_validation() {
    let (mut graph_file, _temp_file) = create_test_graph_file();
    let mut node_store = NodeStore::new(&mut graph_file);

    // Create a node
    let node = NodeRecord::new(
        1,
        "Test".to_string(),
        "node1".to_string(),
        serde_json::json!({}),
    );
    node_store.write_node(&node).unwrap();

    // Validate adjacency (should pass for node with no edges)
    let result = AdjacencyHelpers::validate_node_adjacency(&mut graph_file, 1);
    assert!(result.is_ok());
}

#[cfg(test)]
mod linear_detector_tests {
    use super::super::{LinearDetector, TraversalPattern};

    /// Chain graph confirms Linear after 3 steps.
    ///
    /// Graph structure:
    ///   1 -> 2 -> 3 -> 4 -> 5
    ///
    /// Each node has degree 1 (one outgoing edge).
    /// After 3 consecutive degree-1 observations, detector confirms Linear.
    #[test]
    fn test_linear_detector_chain() {
        let mut detector = LinearDetector::new();

        // Step 1: First degree-1 observation
        let pattern = detector.observe(1, 1);
        assert_eq!(pattern, TraversalPattern::Unknown, "Step 1 should be Unknown");
        assert_eq!(
            detector.current_pattern(),
            TraversalPattern::Unknown,
            "Current pattern at step 1"
        );
        assert!(!detector.is_linear_confirmed(), "Should not be confirmed at step 1");
        assert!((detector.confidence() - 1.0 / 3.0).abs() < f64::EPSILON);

        // Step 2: Second degree-1 observation
        let pattern = detector.observe(2, 1);
        assert_eq!(pattern, TraversalPattern::Unknown, "Step 2 should be Unknown");
        assert_eq!(
            detector.current_pattern(),
            TraversalPattern::Unknown,
            "Current pattern at step 2"
        );
        assert!(!detector.is_linear_confirmed(), "Should not be confirmed at step 2");
        assert!((detector.confidence() - 2.0 / 3.0).abs() < f64::EPSILON);

        // Step 3: Third degree-1 observation - threshold reached!
        let pattern = detector.observe(3, 1);
        assert_eq!(pattern, TraversalPattern::Linear, "Step 3 should be Linear");
        assert_eq!(
            detector.current_pattern(),
            TraversalPattern::Linear,
            "Current pattern at step 3"
        );
        assert!(detector.is_linear_confirmed(), "Should be confirmed at step 3");
        assert_eq!(detector.confidence(), 1.0, "Confidence should be 1.0 at step 3");

        // Step 4+: Remain Linear
        let pattern = detector.observe(4, 1);
        assert_eq!(pattern, TraversalPattern::Linear, "Step 4 should stay Linear");
        assert!(detector.is_linear_confirmed());

        let pattern = detector.observe(5, 1);
        assert_eq!(pattern, TraversalPattern::Linear, "Step 5 should stay Linear");
        assert!(detector.is_linear_confirmed());
    }

    /// Star graph immediately triggers Branching.
    ///
    /// Graph structure:
    ///        2
    ///        |
    ///   4 - 1 - 3
    ///        |
    ///        5
    ///
    /// Node 1 (center) has degree 4, triggering immediate Branching.
    #[test]
    fn test_linear_detector_star() {
        let mut detector = LinearDetector::new();

        // Center node with degree 4 -> immediate Branching
        let pattern = detector.observe(1, 4);
        assert_eq!(
            pattern,
            TraversalPattern::Branching,
            "Degree 4 should trigger Branching immediately"
        );
        assert_eq!(
            detector.current_pattern(),
            TraversalPattern::Branching,
            "Current pattern should be Branching"
        );
        assert_eq!(
            detector.confidence(),
            0.0,
            "Confidence should be 0.0 for Branching"
        );
        assert!(
            !detector.is_linear_confirmed(),
            "Branching pattern should never be confirmed Linear"
        );

        // Branching is terminal - subsequent observations stay Branching
        let pattern = detector.observe(2, 1);
        assert_eq!(
            pattern,
            TraversalPattern::Branching,
            "Branching is terminal state"
        );

        let pattern = detector.observe(3, 1);
        assert_eq!(pattern, TraversalPattern::Branching, "Still Branching");

        assert_eq!(detector.confidence(), 0.0, "Confidence remains 0.0");
    }

    /// Diamond graph transitions from Unknown to Branching at degree-2 node.
    ///
    /// Graph structure:
    ///   1 -> 2 -> 4
    ///   1 -> 3 -> 4
    ///
    /// Starting from leaf (degree 1), then hitting diamond join (degree 2).
    /// Diamond join triggers immediate Branching before Linear confirmation.
    #[test]
    fn test_linear_detector_diamond() {
        let mut detector = LinearDetector::new();

        // Start from leaf node (degree 1)
        let pattern = detector.observe(1, 1);
        assert_eq!(
            pattern,
            TraversalPattern::Unknown,
            "First degree-1 should be Unknown"
        );
        assert!(!detector.is_linear_confirmed());

        // Second linear step
        let pattern = detector.observe(2, 1);
        assert_eq!(
            pattern,
            TraversalPattern::Unknown,
            "Second degree-1 should still be Unknown"
        );
        assert!(!detector.is_linear_confirmed());

        // Diamond join: degree 2 -> immediate Branching
        let pattern = detector.observe(3, 2);
        assert_eq!(
            pattern,
            TraversalPattern::Branching,
            "Degree 2 should trigger Branching"
        );
        assert_eq!(
            detector.current_pattern(),
            TraversalPattern::Branching
        );
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.confidence(), 0.0);
    }

    /// Diamond variant: Start Accumulating then hit Branching.
    ///
    /// Tests the transition from Accumulating internal state to Branching.
    #[test]
    fn test_linear_detector_diamond_accumulating_then_branching() {
        let mut detector = LinearDetector::new();

        // Linear step 1: Unknown
        assert_eq!(detector.observe(1, 1), TraversalPattern::Unknown);

        // Linear step 2: Unknown (internally Accumulating)
        assert_eq!(detector.observe(2, 1), TraversalPattern::Unknown);

        // Degree 2 node: should transition to Branching from Accumulating
        assert_eq!(detector.observe(3, 2), TraversalPattern::Branching);
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.confidence(), 0.0);
    }

    /// Tree graph shows Accumulating behavior without Linear confirmation.
    ///
    /// Graph structure:
    ///       1
    ///      /|\
    ///     2 3 4
    ///     |
    ///     5
    ///     |
    ///     6
    ///
    /// Root has degree 3 -> immediate Branching.
    /// Or: following a branch shows Accumulating until hitting another branch.
    #[test]
    fn test_linear_detector_tree() {
        let mut detector = LinearDetector::new();

        // Root node with degree 3 -> immediate Branching
        let pattern = detector.observe(1, 3);
        assert_eq!(
            pattern,
            TraversalPattern::Branching,
            "Root degree 3 triggers Branching"
        );
        assert!(!detector.is_linear_confirmed());
    }

    /// Tree depth-first traversal: Accumulating then Branching at child.
    ///
    /// Simulates DFS down a linear branch that then splits.
    #[test]
    fn test_linear_detector_tree_depth_first() {
        let mut detector = LinearDetector::new();

        // Linear steps on branch
        assert_eq!(detector.observe(1, 1), TraversalPattern::Unknown);
        assert!((detector.confidence() - 1.0 / 3.0).abs() < f64::EPSILON);

        assert_eq!(detector.observe(2, 1), TraversalPattern::Unknown);
        assert!((detector.confidence() - 2.0 / 3.0).abs() < f64::EPSILON);

        // Child node has degree 2 -> Branching before Linear confirmation
        assert_eq!(detector.observe(3, 2), TraversalPattern::Branching);
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.confidence(), 0.0);
    }

    /// Confidence score progression through Linear detection.
    ///
    /// Verifies confidence increases: 0.0 -> 0.33 -> 0.67 -> 1.0
    #[test]
    fn test_linear_detector_confidence() {
        let mut detector = LinearDetector::new();

        // Initial: no observations
        assert_eq!(detector.confidence(), 0.0, "Initial confidence should be 0.0");
        assert_eq!(
            detector.current_pattern(),
            TraversalPattern::Unknown,
            "Initial pattern should be Unknown"
        );

        // Step 1: 1/3 ≈ 0.33
        let pattern = detector.observe(1, 1);
        assert_eq!(pattern, TraversalPattern::Unknown);
        assert!((detector.confidence() - 1.0 / 3.0).abs() < f64::EPSILON);
        assert_eq!(detector.current_pattern(), TraversalPattern::Unknown);

        // Step 2: 2/3 ≈ 0.67
        let pattern = detector.observe(2, 1);
        assert_eq!(pattern, TraversalPattern::Unknown);
        assert!((detector.confidence() - 2.0 / 3.0).abs() < f64::EPSILON);
        assert_eq!(detector.current_pattern(), TraversalPattern::Unknown);

        // Step 3: 3/3 = 1.0 (confirmed)
        let pattern = detector.observe(3, 1);
        assert_eq!(pattern, TraversalPattern::Linear);
        assert_eq!(detector.confidence(), 1.0);
        assert_eq!(detector.current_pattern(), TraversalPattern::Linear);

        // Step 4+: remain at 1.0
        let pattern = detector.observe(4, 1);
        assert_eq!(pattern, TraversalPattern::Linear);
        assert_eq!(detector.confidence(), 1.0);

        let pattern = detector.observe(5, 1);
        assert_eq!(pattern, TraversalPattern::Linear);
        assert_eq!(detector.confidence(), 1.0);
    }

    /// Reset clears detector state between traversals.
    ///
    /// Verifies reset() returns detector to initial Unknown state.
    #[test]
    fn test_linear_detector_reset() {
        let mut detector = LinearDetector::new();

        // Confirm Linear pattern first
        detector.observe(1, 1);
        detector.observe(2, 1);
        detector.observe(3, 1);

        assert!(
            detector.is_linear_confirmed(),
            "Should be Linear after 3 degree-1 steps"
        );
        assert_eq!(detector.confidence(), 1.0, "Confidence should be 1.0");
        assert_eq!(
            detector.current_pattern(),
            TraversalPattern::Linear,
            "Pattern should be Linear"
        );

        // Reset
        detector.reset();

        // Verify back to initial state
        assert!(
            !detector.is_linear_confirmed(),
            "Should not be confirmed after reset"
        );
        assert_eq!(detector.confidence(), 0.0, "Confidence should be 0.0 after reset");
        assert_eq!(
            detector.current_pattern(),
            TraversalPattern::Unknown,
            "Pattern should be Unknown after reset"
        );

        // Can detect again - should work identically
        let pattern = detector.observe(1, 1);
        assert_eq!(pattern, TraversalPattern::Unknown, "First observation after reset");
        assert!((detector.confidence() - 1.0 / 3.0).abs() < f64::EPSILON);

        detector.observe(2, 1);
        detector.observe(3, 1);
        assert!(detector.is_linear_confirmed(), "Should confirm Linear again");
    }

    /// Custom threshold behavior.
    ///
    /// Verifies threshold=5 requires 5 steps for Linear confirmation.
    #[test]
    fn test_linear_threshold_custom() {
        let detector = LinearDetector::with_threshold(5);
        assert_eq!(detector.confidence(), 0.0);

        let mut detector = LinearDetector::with_threshold(5);

        // Steps 1-4: Accumulating, not Linear yet
        let pattern = detector.observe(1, 1);
        assert_eq!(pattern, TraversalPattern::Unknown);
        assert!((detector.confidence() - 1.0 / 5.0).abs() < f64::EPSILON);
        assert!(!detector.is_linear_confirmed());

        let pattern = detector.observe(2, 1);
        assert_eq!(pattern, TraversalPattern::Unknown);
        assert!((detector.confidence() - 2.0 / 5.0).abs() < f64::EPSILON);
        assert!(!detector.is_linear_confirmed());

        let pattern = detector.observe(3, 1);
        assert_eq!(pattern, TraversalPattern::Unknown);
        assert!((detector.confidence() - 3.0 / 5.0).abs() < f64::EPSILON);
        assert!(!detector.is_linear_confirmed());

        let pattern = detector.observe(4, 1);
        assert_eq!(pattern, TraversalPattern::Unknown);
        assert!((detector.confidence() - 4.0 / 5.0).abs() < f64::EPSILON);
        assert!(!detector.is_linear_confirmed());

        // Step 5: Now confirmed
        let pattern = detector.observe(5, 1);
        assert_eq!(
            pattern,
            TraversalPattern::Linear,
            "Should confirm Linear at threshold=5"
        );
        assert_eq!(detector.confidence(), 1.0);
        assert!(detector.is_linear_confirmed());
    }

    /// Dead end handling: degree 0 keeps detector in Unknown.
    ///
    /// Dead ends (leaf nodes with no outgoing edges) should not
    /// contribute to Linear pattern detection.
    #[test]
    fn test_linear_detector_dead_end() {
        let mut detector = LinearDetector::new();

        // Degree 0: dead end, should stay Unknown
        let pattern = detector.observe(1, 0);
        assert_eq!(
            pattern,
            TraversalPattern::Unknown,
            "Degree 0 should be Unknown"
        );
        assert_eq!(
            detector.current_pattern(),
            TraversalPattern::Unknown,
            "Current pattern should be Unknown"
        );
        assert_eq!(detector.confidence(), 0.0, "Confidence should be 0.0");
        assert!(
            !detector.is_linear_confirmed(),
            "Should not be confirmed with degree 0"
        );

        // Another degree 0
        let pattern = detector.observe(2, 0);
        assert_eq!(pattern, TraversalPattern::Unknown);
        assert_eq!(detector.confidence(), 0.0);

        // Then degree 1 - should start counting from scratch
        let pattern = detector.observe(3, 1);
        assert_eq!(pattern, TraversalPattern::Unknown);
        assert!((detector.confidence() - 1.0 / 3.0).abs() < f64::EPSILON);
    }

    /// Dead end between linear steps does NOT reset progress (by design).
    ///
    /// Chain: 1 -> (dead end) -> 3 -> 4 -> 5
    /// Degree 0 doesn't increment consecutive_linear but also doesn't reset it.
    /// The detector maintains Accumulating state but doesn't progress toward Linear.
    #[test]
    fn test_linear_detector_dead_end_breaks_chain() {
        let mut detector = LinearDetector::new();

        // Step 1: degree 1 -> Accumulating
        assert_eq!(detector.observe(1, 1), TraversalPattern::Unknown);
        assert!((detector.confidence() - 1.0 / 3.0).abs() < f64::EPSILON);

        // Step 2: degree 0 -> stays in Accumulating (doesn't reset counter)
        // The state remains Accumulating, returns Unknown for pattern
        assert_eq!(detector.observe(2, 0), TraversalPattern::Unknown);
        // Confidence still reflects progress (doesn't reset)
        assert!((detector.confidence() - 1.0 / 3.0).abs() < f64::EPSILON);

        // Step 3: degree 1 -> increments counter to 2
        assert_eq!(detector.observe(3, 1), TraversalPattern::Unknown);
        assert!((detector.confidence() - 2.0 / 3.0).abs() < f64::EPSILON);

        // Step 4: degree 1 -> now threshold reached!
        assert_eq!(detector.observe(4, 1), TraversalPattern::Linear);
        assert!(detector.is_linear_confirmed());

        // Note: The dead end didn't reset progress, just didn't advance it.
        // This is intentional: dead ends are rare in chain traversals.
    }
}
