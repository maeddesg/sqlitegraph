//! Linear pattern detection state machine for graph traversals.
//!
//! This module provides a state machine that detects linear traversal patterns
//! (consecutive nodes with degree <= 1) to enable sequential I/O optimization
//! in Phase 30-31.
//!
//! # Design: 4-State Finite State Machine
//!
//! The detector tracks consecutive degree-1 steps during graph traversal:
//!
//! - **Unknown**: Initial state, insufficient observations (0-2 steps)
//! - **Accumulating**: Observing linear pattern (1 to threshold-1 steps)
//! - **Linear**: Confirmed linear pattern (threshold+ consecutive steps)
//! - **Branching**: Branching detected (terminal state, degree > 1 at any point)
//!
//! # Why 3-Step Threshold?
//!
//! The threshold of 3 consecutive degree-1 steps prevents false positives on tree graphs.
//! Trees often have 1-2 linear segments before branching, but rarely 3+ without branching.
//! This threshold was chosen based on STATE.md v1.4 research to avoid triggering
//! sequential I/O optimization incorrectly on branching traversals.
//!
//! # Per-Traversal Design
//!
//! The detector is designed to be per-traversal, not global. Each traversal operation
//! (BFS, k-hop, shortest path) creates its own detector instance or calls `reset()`
//! before starting. This preserves MVCC isolation and prevents cross-traversal state leakage.
//!
//! # Usage Pattern
//!
//! ```rust
//! use crate::backend::native::adjacency::{LinearDetector, AdjacencyHelpers};
//!
//! // Create detector at traversal start
//! let mut detector = LinearDetector::new();
//!
//! // During traversal loop
//! for node_id in visited_nodes {
//!     // Get degree (O(1) via AdjacencyHelpers)
//!     let degree = AdjacencyHelpers::outgoing_degree(graph_file, node_id)?;
//!
//!     // Observe node for pattern detection
//!     let pattern = detector.observe(node_id, degree);
//!
//!     // Check if linear pattern is confirmed
//!     if detector.is_linear_confirmed() {
//!         // Enable sequential I/O optimization (Phase 31)
//!     }
//! }
//!
//! // Detector evaporates when traversal ends
//! // OR call reset() for reuse in same function
//! detector.reset();
//! ```
//!
//! # Phase 29: Read-Only Instrumentation
//!
//! In Phase 29, the detector is read-only instrumentation. It observes degrees and
//! classifies patterns but does not modify I/O behavior. Phase 31 integrates the
//! detector into traversal hot paths to trigger sequential I/O optimization.

use crate::backend::native::types::NativeNodeId;
use std::time::Instant;

/// Check if cluster offsets form a contiguous sequence on disk.
///
/// Contiguity is required for sequential I/O to provide benefit. Non-contiguous
/// clusters read sequentially are still random I/O from the disk's perspective.
///
/// # Parameters
///
/// - **offsets**: Slice of (cluster_offset, cluster_size) tuples
///
/// # Returns
///
/// `true` if clusters are contiguous, `false` otherwise. Returns `false` if
/// fewer than 2 clusters are provided (contiguity is meaningless for a single cluster).
///
/// # Contiguity Definition
///
/// Clusters are contiguous if each cluster starts immediately after the
/// previous one ends: `offsets[i+1] == offsets[i] + sizes[i]`
///
/// # Examples
///
/// ```
/// use sqlitegraph::backend::native::adjacency::are_clusters_contiguous;
///
/// // Empty: not contiguous
/// assert!(!are_clusters_contiguous(&[]));
///
/// // Single cluster: not contiguous (need >=2)
/// assert!(!are_clusters_contiguous(&[(1024, 4096)]));
///
/// // Two contiguous clusters: 1024 + 4096 = 5120
/// assert!(are_clusters_contiguous(&[(1024, 4096), (5120, 4096)]));
///
/// // Gap between clusters
/// assert!(!are_clusters_contiguous(&[(1024, 4096), (6000, 4096)]));
///
/// // Overlapping clusters
/// assert!(!are_clusters_contiguous(&[(1024, 4096), (4000, 4096)]));
/// ```
pub fn are_clusters_contiguous(offsets: &[(u64, u32)]) -> bool {
    // Need at least 2 clusters to check contiguity
    if offsets.len() < 2 {
        return false;
    }

    // Check that each cluster starts where the previous one ended
    for i in 0..offsets.len() - 1 {
        let (current_offset, current_size) = offsets[i];
        let (next_offset, _) = offsets[i + 1];

        // Compute expected next offset (watch for overflow)
        let expected_next = current_offset.saturating_add(current_size as u64);

        // Next cluster must start exactly where current ends
        if next_offset != expected_next {
            return false;
        }
    }

    true
}

/// Traversal pattern classification.
///
/// Represents the detected traversal pattern based on observed node degrees.
/// This classification determines whether sequential I/O optimization should be applied.
///
/// # Variants
///
/// - **Unknown**: Not enough data to classify (0-2 observations, or degree 0 encountered)
/// - **Linear**: Confirmed linear pattern (3+ consecutive degree-1 steps)
/// - **Branching**: Branching detected (degree > 1 at any point)
///
/// # Example
///
/// ```rust
/// use crate::backend::native::adjacency::{LinearDetector, TraversalPattern};
///
/// let mut detector = LinearDetector::new();
///
/// // First observation: degree 1
/// assert_eq!(detector.observe(1, 1), TraversalPattern::Unknown);
///
/// // Second observation: degree 1
/// assert_eq!(detector.observe(2, 1), TraversalPattern::Unknown);
///
/// // Third observation: degree 1 - threshold reached!
/// assert_eq!(detector.observe(3, 1), TraversalPattern::Linear);
///
/// // Fourth observation: degree 2 - branching detected
/// assert_eq!(detector.observe(4, 2), TraversalPattern::Branching);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalPattern {
    /// Not enough data to classify pattern
    Unknown,
    /// Confirmed linear: 3+ consecutive degree-1 steps
    Linear,
    /// Branching detected: degree > 1 at any point
    Branching,
}

/// Internal detector state for the 4-state finite state machine.
///
/// This is the internal state representation, separate from the public
/// `TraversalPattern` enum. The state machine transitions are:
///
/// ```text
///     degree == 1              degree == 1, count >= threshold
/// Unknown ---------> Accumulating ------------------------> Linear
///    |  ^                  |  ^  |
///    |  |                  |  |  |
///    |  | degree == 0      |  |  | degree == 1, count < threshold
///    |  |                  |  |  |
///    v  |                  v  |  v
///   Unknown <---------------   |
///
///     degree > 1 (any state)
///     ------------------------> Branching (terminal)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetectorState {
    /// Initial state, insufficient observations (0-2 steps or degree 0)
    Unknown,
    /// Observing linear pattern (1 to threshold-1 consecutive degree-1 steps)
    Accumulating,
    /// Confirmed linear (threshold+ consecutive degree-1 steps)
    Linear,
    /// Branching detected (terminal state)
    Branching,
}

/// Linear pattern detection state machine.
///
/// Tracks consecutive degree-1 steps during graph traversal to detect linear
/// access patterns. Once linear pattern is confirmed (3+ consecutive steps),
/// the detector can trigger sequential I/O optimization.
///
/// # Fields
///
/// - **state**: Current detector state (Unknown, Accumulating, Linear, Branching)
/// - **consecutive_linear**: Count of consecutive degree-1 steps observed
/// - **threshold**: Number of consecutive degree-1 steps required to confirm Linear (default: 3)
/// - **cluster_offsets**: History of (cluster_offset, cluster_size) tuples observed during traversal
/// - **chains_detected**: Number of chains detected during traversal (Phase 33)
/// - **total_chain_length**: Cumulative length of all detected chains (Phase 33)
/// - **time_linear_detection_ns**: Total time spent in pattern detection (nanoseconds)
/// - **time_contiguity_validation_ns**: Total time spent in contiguity validation (nanoseconds)
///
/// # Cluster Offset Tracking (Phase 33)
///
/// The `cluster_offsets` field stores the offset and size of each edge cluster
/// visited during traversal. This enables contiguity validation in Phase 34-35:
///
/// - Sequential cluster reads require clusters to be contiguous on disk
/// - Tracking offsets during traversal avoids additional I/O for validation
/// - Offsets are cleared on `reset()` to maintain per-traversal isolation
///
/// # Timing Instrumentation (Phase 37)
///
/// The `time_linear_detection_ns` and `time_contiguity_validation_ns` fields accumulate
/// timing information for diagnostic telemetry. These help identify performance bottlenecks
/// during Chain(500) traversal analysis.
///
/// # Example
///
/// ```rust
/// use crate::backend::native::adjacency::LinearDetector;
///
/// let mut detector = LinearDetector::new();
///
/// // Observe nodes during traversal
/// detector.observe(1, 1); // degree 1
/// detector.observe(2, 1); // degree 1
/// detector.observe(3, 1); // degree 1 -> Linear confirmed!
///
/// assert!(detector.is_linear_confirmed());
/// assert_eq!(detector.confidence(), 1.0);
///
/// // Reset for new traversal
/// detector.reset();
/// assert!(!detector.is_linear_confirmed());
/// assert_eq!(detector.confidence(), 0.0);
/// ```
pub struct LinearDetector {
    /// Current detector state
    state: DetectorState,
    /// Consecutive linear steps count
    consecutive_linear: u32,
    /// Confidence threshold (configurable, default: 3)
    threshold: u32,
    /// Cluster offset history: (offset, size) tuples for contiguity validation
    cluster_offsets: Vec<(u64, u32)>,
    /// Number of chains detected during traversal (Phase 33)
    chains_detected: u64,
    /// Cumulative length of all detected chains (Phase 33)
    total_chain_length: u64,
    /// Total time spent in pattern detection (Phase 37 instrumentation)
    time_linear_detection_ns: u64,
    /// Total time spent in contiguity validation (Phase 37 instrumentation)
    time_contiguity_validation_ns: u64,
}

impl LinearDetector {
    /// Create new detector with default threshold (3 steps).
    ///
    /// The threshold of 3 consecutive degree-1 steps prevents false positives
    /// on tree graphs which often have 1-2 linear segments before branching.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let detector = LinearDetector::new();
    /// assert_eq!(detector.confidence(), 0.0);
    /// assert!(!detector.is_linear_confirmed());
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self {
            state: DetectorState::Unknown,
            consecutive_linear: 0,
            threshold: 3,
            cluster_offsets: Vec::new(),
            chains_detected: 0,
            total_chain_length: 0,
            time_linear_detection_ns: 0,
            time_contiguity_validation_ns: 0,
        }
    }

    /// Create new detector with custom threshold.
    ///
    /// Useful for testing with different threshold values. Lower thresholds
    /// increase false positive rate on trees; higher thresholds may miss
    /// legitimate linear patterns.
    ///
    /// # Parameters
    ///
    /// - **threshold**: Minimum consecutive degree-1 steps to confirm Linear pattern
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// // Detector with threshold of 5 (more conservative)
    /// let detector = LinearDetector::with_threshold(5);
    /// ```
    #[inline]
    pub fn with_threshold(threshold: u32) -> Self {
        Self {
            state: DetectorState::Unknown,
            consecutive_linear: 0,
            threshold,
            cluster_offsets: Vec::new(),
            chains_detected: 0,
            total_chain_length: 0,
            time_linear_detection_ns: 0,
            time_contiguity_validation_ns: 0,
        }
    }

    /// Observe a node during traversal.
    ///
    /// This is the core state machine method. It takes a node ID and its degree,
    /// updates internal state, and returns the current pattern classification.
    ///
    /// # State Machine Logic
    ///
    /// - **Branching state**: Immediately return Branching (terminal state)
    /// - **Linear state with degree > 1**: Transition to Branching, return Branching
    /// - **Linear state with degree <= 1**: Stay in Linear, return Linear
    /// - **Unknown/Accumulating with degree > 1**: Transition to Branching, return Branching
    /// - **Unknown/Accumulating with degree == 1**: Increment counter, check threshold
    /// - **Unknown/Accumulating with degree == 0**: Stay in Unknown, return Unknown
    ///
    /// # Parameters
    ///
    /// - **node_id**: The node being observed (for debugging/logging, not used in state logic)
    /// - **degree**: The node's degree (typically from `AdjacencyHelpers::outgoing_degree()`)
    ///
    /// # Returns
    ///
    /// The current `TraversalPattern` classification after this observation.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::{LinearDetector, TraversalPattern};
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// // Chain graph: 1 -> 2 -> 3 -> 4
    /// assert_eq!(detector.observe(1, 1), TraversalPattern::Unknown);
    /// assert_eq!(detector.observe(2, 1), TraversalPattern::Unknown);
    /// assert_eq!(detector.observe(3, 1), TraversalPattern::Linear); // threshold reached
    /// assert_eq!(detector.observe(4, 1), TraversalPattern::Linear);  // stays Linear
    /// ```
    #[inline]
    pub fn observe(&mut self, node_id: NativeNodeId, degree: u32) -> TraversalPattern {
        let start = Instant::now();
        let result = match self.state {
            DetectorState::Branching => {
                // Terminal state: once branching, always branching
                return TraversalPattern::Branching;
            }
            DetectorState::Linear => {
                if degree > 1 {
                    // Exit linear pattern on first branch
                    self.state = DetectorState::Branching;
                    return TraversalPattern::Branching;
                }
                // Stay in Linear state for degree <= 1
                return TraversalPattern::Linear;
            }
            DetectorState::Unknown | DetectorState::Accumulating => {
                if degree > 1 {
                    // Immediate branching detection
                    self.state = DetectorState::Branching;
                    return TraversalPattern::Branching;
                } else if degree == 1 {
                    // Linear step: increment counter
                    self.consecutive_linear += 1;
                    if self.consecutive_linear >= self.threshold {
                        self.state = DetectorState::Linear;
                        return TraversalPattern::Linear;
                    } else {
                        self.state = DetectorState::Accumulating;
                        return TraversalPattern::Unknown;
                    }
                }
                // degree == 0: dead end, stay Unknown
                TraversalPattern::Unknown
            }
        };
        self.time_linear_detection_ns += start.elapsed().as_nanos() as u64;
        result
    }

    /// Observe a node with its cluster information.
    ///
    /// This method extends `observe()` by also recording cluster offset and size
    /// for contiguity validation in Phase 34. It performs the same degree-based
    /// pattern detection as `observe()` while building a history of cluster locations.
    ///
    /// # Parameters
    ///
    /// - **node_id**: The node being observed
    /// - **degree**: The node's degree (typically from `AdjacencyHelpers::outgoing_degree()`)
    /// - **cluster_offset**: Byte offset of the node's edge cluster in the graph file
    /// - **cluster_size**: Size of the edge cluster in bytes
    ///
    /// # Returns
    ///
    /// The current `TraversalPattern` classification after this observation.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::{LinearDetector, TraversalPattern};
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// // Observe nodes with cluster information
    /// assert_eq!(
    ///     detector.observe_with_cluster(1, 1, 1024, 4096),
    ///     TraversalPattern::Unknown
    /// );
    /// assert_eq!(
    ///     detector.observe_with_cluster(2, 1, 5120, 4096),
    ///     TraversalPattern::Unknown
    /// );
    /// assert_eq!(
    ///     detector.observe_with_cluster(3, 1, 9216, 4096),
    ///     TraversalPattern::Linear
    /// );
    ///
    /// // Cluster offsets are recorded for contiguity validation
    /// let offsets = detector.cluster_offsets();
    /// assert_eq!(offsets.len(), 3);
    /// assert_eq!(offsets[0], (1024, 4096));
    /// assert_eq!(offsets[1], (5120, 4096));
    /// assert_eq!(offsets[2], (9216, 4096));
    /// ```
    #[inline]
    pub fn observe_with_cluster(
        &mut self,
        node_id: NativeNodeId,
        degree: u32,
        cluster_offset: u64,
        cluster_size: u32,
    ) -> TraversalPattern {
        // Record cluster offset before pattern detection
        self.cluster_offsets.push((cluster_offset, cluster_size));

        // Delegate to existing observe() for pattern detection
        // Note: timing is already accumulated by observe()
        self.observe(node_id, degree)
    }

    /// Get the recorded cluster offsets.
    ///
    /// Returns a slice of (offset, size) tuples representing the clusters
    /// observed during traversal. This enables contiguity validation in Phase 34.
    ///
    /// # Returns
    ///
    /// Slice of (cluster_offset, cluster_size) tuples.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// detector.observe_with_cluster(1, 1, 1024, 4096);
    /// detector.observe_with_cluster(2, 1, 5120, 4096);
    ///
    /// let offsets = detector.cluster_offsets();
    /// assert_eq!(offsets.len(), 2);
    /// assert_eq!(offsets[0], (1024, 4096));
    /// assert_eq!(offsets[1], (5120, 4096));
    /// ```
    #[inline]
    pub fn cluster_offsets(&self) -> &[(u64, u32)] {
        &self.cluster_offsets
    }

    /// Record a detected chain for instrumentation.
    ///
    /// This method is called when a linear chain is detected during traversal.
    /// It increments the chain counter and accumulates the chain length for
    /// average chain length calculation.
    ///
    /// # Parameters
    ///
    /// - **length**: The length of the detected chain (number of nodes/edges)
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// // Record a chain of length 10
    /// detector.record_chain(10);
    /// assert_eq!(detector.chain_count(), 1);
    /// assert_eq!(detector.total_chain_length(), 10);
    ///
    /// // Record another chain of length 5
    /// detector.record_chain(5);
    /// assert_eq!(detector.chain_count(), 2);
    /// assert_eq!(detector.total_chain_length(), 15);
    /// ```
    #[inline]
    pub fn record_chain(&mut self, length: u32) {
        self.chains_detected += 1;
        self.total_chain_length += length as u64;
    }

    /// Get confidence score (0.0 to 1.0).
    ///
    /// Confidence indicates how certain the detector is that the current
    /// traversal is linear:
    ///
    /// - **1.0**: Confirmed Linear (threshold+ consecutive degree-1 steps)
    /// - **0.0 < x < 1.0**: Accumulating (progress toward threshold, e.g., 2/3 = 0.67)
    /// - **0.0**: Unknown or Branching
    ///
    /// # Returns
    ///
    /// Confidence score in range [0.0, 1.0].
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// assert_eq!(detector.confidence(), 0.0); // Initial state
    ///
    /// detector.observe(1, 1);
    /// assert_eq!(detector.confidence(), 1.0 / 3.0); // 1/3 ≈ 0.33
    ///
    /// detector.observe(2, 1);
    /// assert_eq!(detector.confidence(), 2.0 / 3.0); // 2/3 ≈ 0.67
    ///
    /// detector.observe(3, 1);
    /// assert_eq!(detector.confidence(), 1.0); // Confirmed Linear
    /// ```
    #[inline]
    pub fn confidence(&self) -> f64 {
        match self.state {
            DetectorState::Linear => 1.0,
            DetectorState::Accumulating => {
                // Partial confidence based on progress to threshold
                if self.threshold > 0 {
                    (self.consecutive_linear as f64) / (self.threshold as f64)
                } else {
                    0.0
                }
            }
            DetectorState::Unknown | DetectorState::Branching => 0.0,
        }
    }

    /// Reset detector state (for new traversal).
    ///
    /// Clears all state and returns the detector to initial Unknown condition.
    /// Call this when starting a new traversal or reusing a detector instance.
    ///
    /// This also clears the cluster offset history and chain instrumentation,
    /// ensuring per-traversal isolation.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// // First traversal
    /// detector.observe(1, 1);
    /// detector.observe(2, 1);
    /// detector.observe(3, 1);
    /// assert!(detector.is_linear_confirmed());
    ///
    /// detector.record_chain(10);
    /// assert_eq!(detector.chain_count(), 1);
    ///
    /// // Reset for second traversal
    /// detector.reset();
    /// assert!(!detector.is_linear_confirmed());
    /// assert_eq!(detector.confidence(), 0.0);
    /// assert_eq!(detector.cluster_offsets().len(), 0);
    /// assert_eq!(detector.chain_count(), 0);
    /// ```
    #[inline]
    pub fn reset(&mut self) {
        self.state = DetectorState::Unknown;
        self.consecutive_linear = 0;
        self.cluster_offsets.clear();
        self.chains_detected = 0;
        self.total_chain_length = 0;
        self.time_linear_detection_ns = 0;
        self.time_contiguity_validation_ns = 0;
    }

    /// Get current pattern without observation.
    ///
    /// Returns the current pattern classification without modifying state.
    /// Useful for checking detector state between observations.
    ///
    /// # Returns
    ///
    /// The current `TraversalPattern` classification.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::{LinearDetector, TraversalPattern};
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// assert_eq!(detector.current_pattern(), TraversalPattern::Unknown);
    ///
    /// detector.observe(1, 1);
    /// assert_eq!(detector.current_pattern(), TraversalPattern::Unknown);
    ///
    /// detector.observe(2, 1);
    /// detector.observe(3, 1);
    /// assert_eq!(detector.current_pattern(), TraversalPattern::Linear);
    /// ```
    #[inline]
    pub fn current_pattern(&self) -> TraversalPattern {
        match self.state {
            DetectorState::Linear => TraversalPattern::Linear,
            DetectorState::Branching => TraversalPattern::Branching,
            DetectorState::Unknown | DetectorState::Accumulating => TraversalPattern::Unknown,
        }
    }

    /// Check if linear pattern is confirmed.
    ///
    /// Returns `true` if the detector has observed threshold+ consecutive
    /// degree-1 steps and is in Linear state. This is the primary method
    /// to check whether sequential I/O optimization should be enabled.
    ///
    /// # Returns
    ///
    /// `true` if linear pattern is confirmed (confidence >= 1.0), `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// assert!(!detector.is_linear_confirmed());
    ///
    /// detector.observe(1, 1);
    /// assert!(!detector.is_linear_confirmed());
    ///
    /// detector.observe(2, 1);
    /// assert!(!detector.is_linear_confirmed());
    ///
    /// detector.observe(3, 1);
    /// assert!(detector.is_linear_confirmed()); // threshold reached!
    /// ```
    #[inline]
    pub fn is_linear_confirmed(&self) -> bool {
        self.state == DetectorState::Linear
    }

    /// Get the number of chains detected during traversal.
    ///
    /// Returns the count of chains that have been recorded via `record_chain()`.
    /// This metric helps validate the effectiveness of chain detection for IO-12.
    ///
    /// # Returns
    ///
    /// Number of chains detected (0 if none).
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// assert_eq!(detector.chain_count(), 0);
    ///
    /// detector.record_chain(10);
    /// assert_eq!(detector.chain_count(), 1);
    ///
    /// detector.record_chain(5);
    /// assert_eq!(detector.chain_count(), 2);
    /// ```
    #[inline]
    pub fn chain_count(&self) -> u64 {
        self.chains_detected
    }

    /// Get the total accumulated length of all detected chains.
    ///
    /// Returns the sum of lengths of all chains recorded via `record_chain()`.
    /// Combined with `chain_count()`, this enables calculating average chain length.
    ///
    /// # Returns
    ///
    /// Total chain length across all detected chains (0 if none).
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// assert_eq!(detector.total_chain_length(), 0);
    ///
    /// detector.record_chain(10);
    /// assert_eq!(detector.total_chain_length(), 10);
    ///
    /// detector.record_chain(5);
    /// assert_eq!(detector.total_chain_length(), 15);
    /// ```
    #[inline]
    pub fn total_chain_length(&self) -> u64 {
        self.total_chain_length
    }

    /// Get the average length of detected chains.
    ///
    /// Returns the mean chain length across all chains recorded via `record_chain()`.
    /// Returns 0.0 if no chains have been detected.
    ///
    /// # Returns
    ///
    /// Average chain length as f64, or 0.0 if no chains detected.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// // No chains: average is 0.0
    /// assert_eq!(detector.average_chain_length(), 0.0);
    ///
    /// detector.record_chain(10);
    /// assert_eq!(detector.average_chain_length(), 10.0);
    ///
    /// detector.record_chain(20);
    /// // Average: (10 + 20) / 2 = 15.0
    /// assert_eq!(detector.average_chain_length(), 15.0);
    /// ```
    #[inline]
    pub fn average_chain_length(&self) -> f64 {
        if self.chains_detected == 0 {
            0.0
        } else {
            self.total_chain_length as f64 / self.chains_detected as f64
        }
    }

    /// Validate that recorded cluster offsets form a contiguous sequence on disk.
    ///
    /// Contiguity is required for sequential I/O optimization to provide benefit.
    /// Non-contiguous clusters read sequentially are still random I/O from the
    /// disk's perspective.
    ///
    /// # Returns
    ///
    /// `true` if clusters are contiguous, `false` otherwise. Returns `false` if
    /// fewer than 2 clusters have been recorded (contiguity is meaningless for
    /// a single cluster).
    ///
    /// # Contiguity Definition
    ///
    /// Clusters are contiguous if each cluster starts immediately after the
    /// previous one ends: `offsets[i+1] == offsets[i] + sizes[i]`
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// // Empty history: not contiguous
    /// assert!(!detector.validate_contiguity());
    ///
    /// // Single cluster: not contiguous (need >=2)
    /// detector.observe_with_cluster(1, 1, 1024, 4096);
    /// assert!(!detector.validate_contiguity());
    ///
    /// // Two contiguous clusters: 1024 + 4096 = 5120
    /// detector.observe_with_cluster(2, 1, 5120, 4096);
    /// assert!(detector.validate_contiguity());
    ///
    /// // Third cluster creates a gap
    /// detector.observe_with_cluster(3, 1, 10000, 4096); // should be 9216
    /// assert!(!detector.validate_contiguity());
    /// ```
    #[inline]
    pub fn validate_contiguity(&mut self) -> bool {
        let start = Instant::now();
        let result = are_clusters_contiguous(&self.cluster_offsets);
        self.time_contiguity_validation_ns += start.elapsed().as_nanos() as u64;
        result
    }

    /// Get total time spent in linear pattern detection (milliseconds).
    ///
    /// Returns the accumulated time for all calls to `observe()` and `observe_with_cluster()`.
    /// This is diagnostic instrumentation for Phase 37 gap analysis.
    ///
    /// # Returns
    ///
    /// Total detection time in milliseconds (converted from nanoseconds).
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// // Perform observations
    /// for i in 1..=100 {
    ///     detector.observe(i, 1);
    /// }
    ///
    /// // Get accumulated timing
    /// let detection_ms = detector.time_linear_detection_ms();
    /// ```
    #[inline]
    pub fn time_linear_detection_ms(&self) -> f64 {
        self.time_linear_detection_ns as f64 / 1_000_000.0
    }

    /// Get total time spent in contiguity validation (milliseconds).
    ///
    /// Returns the accumulated time for all calls to `validate_contiguity()`.
    /// This is diagnostic instrumentation for Phase 37 gap analysis.
    ///
    /// # Returns
    ///
    /// Total validation time in milliseconds (converted from nanoseconds).
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// // Perform observations with cluster info
    /// detector.observe_with_cluster(1, 1, 1024, 4096);
    /// detector.observe_with_cluster(2, 1, 5120, 4096);
    ///
    /// // Validate contiguity multiple times
    /// let _ = detector.validate_contiguity();
    /// let _ = detector.validate_contiguity();
    ///
    /// // Get accumulated timing
    /// let validation_ms = detector.time_contiguity_validation_ms();
    /// ```
    #[inline]
    pub fn time_contiguity_validation_ms(&self) -> f64 {
        self.time_contiguity_validation_ns as f64 / 1_000_000.0
    }

    /// Check if sequential read path should be used.
    ///
    /// This is the single boolean check that traversal code uses to decide
    /// whether to use sequential cluster reads (Phase 34) or the standard path.
    ///
    /// # When to Call
    ///
    /// Call this method after `observe_with_cluster()` once the traversal has
    /// accumulated enough observations. The method returns `true` only when:
    ///
    /// 1. The pattern is confirmed linear (`is_linear_confirmed()`)
    /// 2. The clusters are contiguous on disk (`validate_contiguity()`)
    ///
    /// Both conditions are required for sequential I/O to provide benefit.
    ///
    /// # Integration with Phase 34
    ///
    /// When this returns `true`, the traversal can use `SequentialClusterReader`
    /// to read all clusters for a chain in a single I/O operation. This provides
    /// the performance optimization target for IO-12 (Chain(500) <= 75ms).
    ///
    /// # Relationship with TraversalContext
    ///
    /// The detector is per-traversal (stored in `TraversalContext` or local to
    /// the traversal function). This preserves MVCC isolation and prevents
    /// cross-traversal state leakage.
    ///
    /// # Returns
    ///
    /// `true` if both linear pattern is confirmed AND clusters are contiguous,
    /// `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// // Before threshold: false
    /// detector.observe_with_cluster(1, 1, 1024, 4096);
    /// assert!(!detector.should_use_sequential_read());
    ///
    /// // Still before threshold: false
    /// detector.observe_with_cluster(2, 1, 5120, 4096);
    /// assert!(!detector.should_use_sequential_read());
    ///
    /// // Linear confirmed + contiguous: true
    /// detector.observe_with_cluster(3, 1, 9216, 4096);
    /// assert!(detector.should_use_sequential_read());
    ///
    /// // Non-contiguous clusters: false
    /// detector.reset();
    /// detector.observe_with_cluster(1, 1, 1024, 4096);
    /// detector.observe_with_cluster(2, 1, 5120, 4096);
    /// detector.observe_with_cluster(3, 1, 10000, 4096); // Gap: should be 9216
    /// assert!(!detector.should_use_sequential_read());
    /// ```
    #[inline]
    pub fn should_use_sequential_read(&mut self) -> bool {
        self.is_linear_confirmed() && {
            let start = Instant::now();
            let result = are_clusters_contiguous(&self.cluster_offsets);
            self.time_contiguity_validation_ns += start.elapsed().as_nanos() as u64;
            result
        }
    }

    /// Predict the total chain length based on current observations.
    ///
    /// This method provides an estimate of the final chain length for allocation-aware
    /// optimization (Phase 40). The prediction is used to determine whether to trigger
    /// contiguous allocation during writes.
    ///
    /// # Prediction Strategy
    ///
    /// - **Linear confirmed**: Returns the max of cluster_offsets.len() and consecutive_linear
    ///   (handles both `observe()` and `observe_with_cluster()` usage)
    /// - **Accumulating**: Returns the current consecutive linear count
    /// - **Unknown/Branching**: Returns 0 (no chain detected)
    ///
    /// # Integration with Write-Path Optimization
    ///
    /// When writing clusters, this method can be called to predict the chain length:
    /// - If predicted >= CHAIN_THRESHOLD, trigger contiguous allocation
    /// - Otherwise, use normal fragmented allocation
    ///
    /// # Returns
    ///
    /// Predicted chain length as `usize` (number of clusters/nodes in the chain).
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::backend::native::adjacency::LinearDetector;
    ///
    /// let mut detector = LinearDetector::new();
    ///
    /// // Initial state: no prediction
    /// assert_eq!(detector.predicted_chain_length(), 0);
    ///
    /// // Observing linear pattern
    /// detector.observe(1, 1);
    /// assert_eq!(detector.predicted_chain_length(), 1);
    ///
    /// detector.observe(2, 1);
    /// assert_eq!(detector.predicted_chain_length(), 2);
    ///
    /// // After threshold: prediction uses cluster_offsets
    /// detector.observe_with_cluster(3, 1, 9216, 4096);
    /// assert_eq!(detector.predicted_chain_length(), 3);
    /// ```
    #[inline]
    pub fn predicted_chain_length(&self) -> usize {
        match self.state {
            DetectorState::Linear => {
                // Linear confirmed: use max of cluster count and consecutive count
                // This handles both observe() and observe_with_cluster() usage
                self.cluster_offsets.len().max(self.consecutive_linear as usize)
            }
            DetectorState::Accumulating => {
                // Accumulating: use consecutive linear count
                self.consecutive_linear as usize
            }
            DetectorState::Unknown | DetectorState::Branching => {
                // No linear pattern detected
                0
            }
        }
    }

    /// Get the current observed chain length (number of linear steps observed).
    ///
    /// This is a helper for accessing the raw consecutive linear count.
    /// For most use cases, `predicted_chain_length()` is preferred.
    ///
    /// # Returns
    ///
    /// Number of consecutive linear steps observed so far.
    #[inline]
    pub fn observed_length(&self) -> usize {
        self.consecutive_linear as usize
    }
}

impl Default for LinearDetector {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_detector_new() {
        let detector = LinearDetector::new();
        assert_eq!(detector.confidence(), 0.0);
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.current_pattern(), TraversalPattern::Unknown);
    }

    #[test]
    fn test_linear_detector_default() {
        let detector = LinearDetector::default();
        assert_eq!(detector.confidence(), 0.0);
        assert!(!detector.is_linear_confirmed());
    }

    #[test]
    fn test_linear_detector_with_threshold() {
        let detector = LinearDetector::with_threshold(5);
        assert_eq!(detector.confidence(), 0.0);
        assert!(!detector.is_linear_confirmed());
    }

    #[test]
    fn test_linear_detector_chain_confirms_after_three() {
        let mut detector = LinearDetector::new();

        // First degree-1 step: Unknown, confidence = 1/3
        assert_eq!(detector.observe(1, 1), TraversalPattern::Unknown);
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.current_pattern(), TraversalPattern::Unknown);
        assert!((detector.confidence() - 1.0 / 3.0).abs() < f64::EPSILON);

        // Second degree-1 step: Unknown, confidence = 2/3
        assert_eq!(detector.observe(2, 1), TraversalPattern::Unknown);
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.current_pattern(), TraversalPattern::Unknown);
        assert!((detector.confidence() - 2.0 / 3.0).abs() < f64::EPSILON);

        // Third degree-1 step: Linear confirmed, confidence = 1.0
        assert_eq!(detector.observe(3, 1), TraversalPattern::Linear);
        assert!(detector.is_linear_confirmed());
        assert_eq!(detector.current_pattern(), TraversalPattern::Linear);
        assert_eq!(detector.confidence(), 1.0);
    }

    #[test]
    fn test_linear_detector_star_immediate_branching() {
        let mut detector = LinearDetector::new();

        // First observation: degree 3 -> immediate Branching
        assert_eq!(detector.observe(1, 3), TraversalPattern::Branching);
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.current_pattern(), TraversalPattern::Branching);
        assert_eq!(detector.confidence(), 0.0);

        // Subsequent observations stay in Branching (terminal state)
        assert_eq!(detector.observe(2, 1), TraversalPattern::Branching);
        assert_eq!(detector.observe(3, 1), TraversalPattern::Branching);
        assert_eq!(detector.confidence(), 0.0);
    }

    #[test]
    fn test_linear_detector_diamond_transitions_to_branching() {
        let mut detector = LinearDetector::new();

        // First node: degree 1 -> Unknown
        assert_eq!(detector.observe(1, 1), TraversalPattern::Unknown);

        // Second node: degree 2 -> Branching
        assert_eq!(detector.observe(2, 2), TraversalPattern::Branching);
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.current_pattern(), TraversalPattern::Branching);
        assert_eq!(detector.confidence(), 0.0);
    }

    #[test]
    fn test_linear_detector_linear_then_branching() {
        let mut detector = LinearDetector::new();

        // Three degree-1 steps -> Linear confirmed
        assert_eq!(detector.observe(1, 1), TraversalPattern::Unknown);
        assert_eq!(detector.observe(2, 1), TraversalPattern::Unknown);
        assert_eq!(detector.observe(3, 1), TraversalPattern::Linear);
        assert!(detector.is_linear_confirmed());

        // Fourth step: degree 2 -> transitions to Branching
        assert_eq!(detector.observe(4, 2), TraversalPattern::Branching);
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.confidence(), 0.0);
    }

    #[test]
    fn test_linear_detector_dead_end_stays_unknown() {
        let mut detector = LinearDetector::new();

        // Degree 0: dead end, stays Unknown
        assert_eq!(detector.observe(1, 0), TraversalPattern::Unknown);
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.confidence(), 0.0);

        // Another degree 0
        assert_eq!(detector.observe(2, 0), TraversalPattern::Unknown);
        assert_eq!(detector.confidence(), 0.0);

        // Then degree 1
        assert_eq!(detector.observe(3, 1), TraversalPattern::Unknown);
        assert!((detector.confidence() - 1.0 / 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_linear_detector_reset() {
        let mut detector = LinearDetector::new();

        // Confirm Linear pattern
        detector.observe(1, 1);
        detector.observe(2, 1);
        detector.observe(3, 1);
        assert!(detector.is_linear_confirmed());
        assert_eq!(detector.confidence(), 1.0);

        // Reset
        detector.reset();
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.confidence(), 0.0);
        assert_eq!(detector.current_pattern(), TraversalPattern::Unknown);

        // Can detect again
        detector.observe(1, 1);
        assert!((detector.confidence() - 1.0 / 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_linear_detector_custom_threshold() {
        let mut detector = LinearDetector::with_threshold(5);

        // 4 steps: not yet confirmed with threshold=5
        detector.observe(1, 1);
        assert!((detector.confidence() - 1.0 / 5.0).abs() < f64::EPSILON);

        detector.observe(2, 1);
        assert!((detector.confidence() - 2.0 / 5.0).abs() < f64::EPSILON);

        detector.observe(3, 1);
        assert!((detector.confidence() - 3.0 / 5.0).abs() < f64::EPSILON);

        detector.observe(4, 1);
        assert!((detector.confidence() - 4.0 / 5.0).abs() < f64::EPSILON);
        assert!(!detector.is_linear_confirmed());

        // Fifth step: confirmed
        detector.observe(5, 1);
        assert_eq!(detector.confidence(), 1.0);
        assert!(detector.is_linear_confirmed());
    }

    #[test]
    fn test_linear_detector_confidence_progression() {
        let mut detector = LinearDetector::new();

        assert_eq!(detector.confidence(), 0.0);

        detector.observe(1, 1);
        assert!((detector.confidence() - 1.0 / 3.0).abs() < f64::EPSILON);

        detector.observe(2, 1);
        assert!((detector.confidence() - 2.0 / 3.0).abs() < f64::EPSILON);

        detector.observe(3, 1);
        assert_eq!(detector.confidence(), 1.0);
    }

    #[test]
    fn test_linear_detector_current_pattern() {
        let mut detector = LinearDetector::new();

        assert_eq!(detector.current_pattern(), TraversalPattern::Unknown);

        detector.observe(1, 1);
        assert_eq!(detector.current_pattern(), TraversalPattern::Unknown);

        detector.observe(2, 1);
        assert_eq!(detector.current_pattern(), TraversalPattern::Unknown);

        detector.observe(3, 1);
        assert_eq!(detector.current_pattern(), TraversalPattern::Linear);

        detector.observe(4, 2);
        assert_eq!(detector.current_pattern(), TraversalPattern::Branching);
    }

    #[test]
    fn test_traversal_pattern_traits() {
        // Verify TraversalPattern has required derives
        let pattern = TraversalPattern::Linear;
        let _ = pattern; // Copy works
        let _clone = pattern; // Clone works
        let _format = format!("{:?}", pattern); // Debug works
        let _eq = pattern == TraversalPattern::Linear; // PartialEq works
    }

    // Phase 33: Cluster offset tracking tests

    #[test]
    fn test_cluster_offsets_initially_empty() {
        let detector = LinearDetector::new();
        assert_eq!(detector.cluster_offsets().len(), 0);
        assert!(detector.cluster_offsets().is_empty());
    }

    #[test]
    fn test_cluster_offsets_single_observation() {
        let mut detector = LinearDetector::new();

        detector.observe_with_cluster(1, 1, 1024, 4096);

        let offsets = detector.cluster_offsets();
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0], (1024, 4096));
    }

    #[test]
    fn test_cluster_offsets_multiple_observations() {
        let mut detector = LinearDetector::new();

        // Simulate contiguous clusters: 1024 + 4096 = 5120, 5120 + 4096 = 9216
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        detector.observe_with_cluster(3, 1, 9216, 4096);
        detector.observe_with_cluster(4, 1, 13312, 4096);

        let offsets = detector.cluster_offsets();
        assert_eq!(offsets.len(), 4);
        assert_eq!(offsets[0], (1024, 4096));
        assert_eq!(offsets[1], (5120, 4096));
        assert_eq!(offsets[2], (9216, 4096));
        assert_eq!(offsets[3], (13312, 4096));
    }

    #[test]
    fn test_cluster_offsets_recorded_in_order() {
        let mut detector = LinearDetector::new();

        // Non-contiguous offsets to verify ordering is preserved
        detector.observe_with_cluster(1, 1, 100, 100);
        detector.observe_with_cluster(2, 1, 5000, 200);
        detector.observe_with_cluster(3, 1, 10000, 150);
        detector.observe_with_cluster(4, 1, 2000, 300);

        let offsets = detector.cluster_offsets();
        assert_eq!(offsets.len(), 4);

        // Verify order matches observation order
        assert_eq!(offsets[0], (100, 100));
        assert_eq!(offsets[1], (5000, 200));
        assert_eq!(offsets[2], (10000, 150));
        assert_eq!(offsets[3], (2000, 300));
    }

    #[test]
    fn test_cluster_offsets_reset_clears_history() {
        let mut detector = LinearDetector::new();

        // Record some offsets
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        detector.observe_with_cluster(3, 1, 9216, 4096);

        assert_eq!(detector.cluster_offsets().len(), 3);

        // Reset
        detector.reset();

        // Verify cleared
        assert_eq!(detector.cluster_offsets().len(), 0);
        assert!(detector.cluster_offsets().is_empty());
    }

    #[test]
    fn test_cluster_offsets_with_pattern_detection() {
        let mut detector = LinearDetector::new();

        // Pattern detection should work alongside offset recording
        assert_eq!(
            detector.observe_with_cluster(1, 1, 1024, 4096),
            TraversalPattern::Unknown
        );
        assert_eq!(detector.cluster_offsets().len(), 1);

        assert_eq!(
            detector.observe_with_cluster(2, 1, 5120, 4096),
            TraversalPattern::Unknown
        );
        assert_eq!(detector.cluster_offsets().len(), 2);

        assert_eq!(
            detector.observe_with_cluster(3, 1, 9216, 4096),
            TraversalPattern::Linear
        );
        assert_eq!(detector.cluster_offsets().len(), 3);
        assert!(detector.is_linear_confirmed());
    }

    #[test]
    fn test_cluster_offsets_after_branching() {
        let mut detector = LinearDetector::new();

        // Linear steps
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        detector.observe_with_cluster(3, 1, 9216, 4096);

        // Branching - offsets continue to be recorded
        assert_eq!(
            detector.observe_with_cluster(4, 2, 13312, 8192),
            TraversalPattern::Branching
        );

        let offsets = detector.cluster_offsets();
        assert_eq!(offsets.len(), 4);
        assert_eq!(offsets[3], (13312, 8192));
    }

    #[test]
    fn test_cluster_offsets_different_sizes() {
        let mut detector = LinearDetector::new();

        // Clusters can have different sizes
        detector.observe_with_cluster(1, 1, 0, 100);
        detector.observe_with_cluster(2, 1, 100, 200);
        detector.observe_with_cluster(3, 1, 300, 150);
        detector.observe_with_cluster(4, 1, 450, 4000);

        let offsets = detector.cluster_offsets();
        assert_eq!(offsets.len(), 4);
        assert_eq!(offsets[0].1, 100);
        assert_eq!(offsets[1].1, 200);
        assert_eq!(offsets[2].1, 150);
        assert_eq!(offsets[3].1, 4000);
    }

    #[test]
    fn test_cluster_offsets_empty_using_observe() {
        let mut detector = LinearDetector::new();

        // Using observe() (without cluster info) should not record offsets
        detector.observe(1, 1);
        detector.observe(2, 1);
        detector.observe(3, 1);

        // No offsets recorded via observe()
        assert_eq!(detector.cluster_offsets().len(), 0);
    }

    #[test]
    fn test_cluster_offsets_mixed_observe_methods() {
        let mut detector = LinearDetector::new();

        // Mix of observe() and observe_with_cluster()
        detector.observe(1, 1);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        detector.observe(3, 1);
        detector.observe_with_cluster(4, 1, 13312, 4096);

        // Only observe_with_cluster() calls record offsets
        let offsets = detector.cluster_offsets();
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0], (5120, 4096));
        assert_eq!(offsets[1], (13312, 4096));

        // But pattern detection still works
        assert!(detector.is_linear_confirmed());
    }

    #[test]
    fn test_cluster_offsets_large_offsets() {
        let mut detector = LinearDetector::new();

        // Test with large offset values (near u32::MAX boundary for u64)
        let large_offset: u64 = 4_000_000_000;
        let large_size: u32 = 4096;

        detector.observe_with_cluster(1, 1, large_offset, large_size);

        let offsets = detector.cluster_offsets();
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0], (large_offset, large_size));
    }

    // Phase 33: Contiguity validation tests

    #[test]
    fn test_are_clusters_contiguous_empty_returns_false() {
        let offsets: &[(u64, u32)] = &[];
        assert!(!are_clusters_contiguous(offsets));
    }

    #[test]
    fn test_are_clusters_contiguous_single_returns_false() {
        let offsets = [(1024, 4096)];
        assert!(!are_clusters_contiguous(&offsets));
    }

    #[test]
    fn test_are_clusters_contiguous_two_contiguous_returns_true() {
        // 1024 + 4096 = 5120, so next cluster starts at 5120
        let offsets = [(1024, 4096), (5120, 4096)];
        assert!(are_clusters_contiguous(&offsets));
    }

    #[test]
    fn test_are_clusters_contiguous_multiple_contiguous_returns_true() {
        // 1024 + 4096 = 5120, 5120 + 4096 = 9216, 9216 + 4096 = 13312
        let offsets = [(1024, 4096), (5120, 4096), (9216, 4096), (13312, 4096)];
        assert!(are_clusters_contiguous(&offsets));
    }

    #[test]
    fn test_are_clusters_contiguous_gap_returns_false() {
        // Gap: 5120 + 4096 = 9216, but next is 10000 (gap of 784)
        let offsets = [(1024, 4096), (5120, 4096), (10000, 4096)];
        assert!(!are_clusters_contiguous(&offsets));
    }

    #[test]
    fn test_are_clusters_contiguous_overlap_returns_false() {
        // Overlap: 1024 + 4096 = 5120, but next is 4000 (overlap)
        let offsets = [(1024, 4096), (4000, 4096)];
        assert!(!are_clusters_contiguous(&offsets));
    }

    #[test]
    fn test_are_clusters_contiguous_different_sizes() {
        // 0 + 100 = 100, 100 + 200 = 300, 300 + 150 = 450
        let offsets = [(0, 100), (100, 200), (300, 150)];
        assert!(are_clusters_contiguous(&offsets));
    }

    #[test]
    fn test_are_clusters_contiguous_non_contiguous_different_sizes() {
        // Gap: 0 + 100 = 100, but next is 150 (gap of 50)
        let offsets = [(0, 100), (150, 200)];
        assert!(!are_clusters_contiguous(&offsets));
    }

    #[test]
    fn test_validate_contiguity_empty_returns_false() {
        let mut detector = LinearDetector::new();
        assert!(!detector.validate_contiguity());
    }

    #[test]
    fn test_validate_contiguity_single_cluster_returns_false() {
        let mut detector = LinearDetector::new();
        detector.observe_with_cluster(1, 1, 1024, 4096);
        assert!(!detector.validate_contiguity());
    }

    #[test]
    fn test_validate_contiguity_contiguous_returns_true() {
        let mut detector = LinearDetector::new();
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        assert!(detector.validate_contiguity());
    }

    #[test]
    fn test_validate_contiguity_gap_returns_false() {
        let mut detector = LinearDetector::new();
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        detector.observe_with_cluster(3, 1, 10000, 4096); // Gap: should be 9216
        assert!(!detector.validate_contiguity());
    }

    #[test]
    fn test_validate_contiguity_overlap_returns_false() {
        let mut detector = LinearDetector::new();
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 4000, 4096); // Overlap
        assert!(!detector.validate_contiguity());
    }

    #[test]
    fn test_validate_contiguity_after_reset_returns_false() {
        let mut detector = LinearDetector::new();
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        assert!(detector.validate_contiguity());

        detector.reset();
        assert!(!detector.validate_contiguity());
    }

    #[test]
    fn test_validate_contiguity_with_branching() {
        let mut detector = LinearDetector::new();
        // Linear pattern with contiguous clusters
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        detector.observe_with_cluster(3, 1, 9216, 4096);
        assert!(detector.validate_contiguity());

        // Branching doesn't affect contiguity of recorded offsets
        detector.observe_with_cluster(4, 2, 13312, 8192);
        assert!(detector.validate_contiguity());
    }

    #[test]
    fn test_validate_contiguity_variable_sizes() {
        let mut detector = LinearDetector::new();
        // 0 + 100 = 100, 100 + 200 = 300, 300 + 150 = 450, 450 + 4000 = 4450
        detector.observe_with_cluster(1, 1, 0, 100);
        detector.observe_with_cluster(2, 1, 100, 200);
        detector.observe_with_cluster(3, 1, 300, 150);
        detector.observe_with_cluster(4, 1, 450, 4000);
        assert!(detector.validate_contiguity());
    }

    #[test]
    fn test_validate_contiguity_large_offsets() {
        let mut detector = LinearDetector::new();
        let large_offset: u64 = 4_000_000_000;
        let large_size: u32 = 4096;
        let next_offset = large_offset + large_size as u64;

        detector.observe_with_cluster(1, 1, large_offset, large_size);
        detector.observe_with_cluster(2, 1, next_offset, large_size);
        assert!(detector.validate_contiguity());
    }

    // Phase 33 Plan 04: Chain detection instrumentation tests

    #[test]
    fn test_chain_instrumentation_initial_state() {
        let detector = LinearDetector::new();

        // Initial state: zero-initialized
        assert_eq!(detector.chain_count(), 0);
        assert_eq!(detector.total_chain_length(), 0);
        assert_eq!(detector.average_chain_length(), 0.0);
    }

    #[test]
    fn test_chain_instrumentation_single_chain() {
        let mut detector = LinearDetector::new();

        // Record a single chain
        detector.record_chain(10);

        assert_eq!(detector.chain_count(), 1);
        assert_eq!(detector.total_chain_length(), 10);
        assert_eq!(detector.average_chain_length(), 10.0);
    }

    #[test]
    fn test_chain_instrumentation_multiple_chains() {
        let mut detector = LinearDetector::new();

        // Record multiple chains
        detector.record_chain(10);
        detector.record_chain(20);
        detector.record_chain(30);

        assert_eq!(detector.chain_count(), 3);
        assert_eq!(detector.total_chain_length(), 60);
        assert_eq!(detector.average_chain_length(), 20.0);
    }

    #[test]
    fn test_chain_instrumentation_average_calculation() {
        let mut detector = LinearDetector::new();

        // Test average calculation with various chain lengths
        detector.record_chain(5);
        assert_eq!(detector.average_chain_length(), 5.0);

        detector.record_chain(15);
        assert_eq!(detector.average_chain_length(), 10.0); // (5 + 15) / 2

        detector.record_chain(10);
        assert_eq!(detector.average_chain_length(), 10.0); // (5 + 15 + 10) / 3
    }

    #[test]
    fn test_chain_instrumentation_accumulation() {
        let mut detector = LinearDetector::new();

        // Accumulate chains over time
        let mut total = 0u64;
        for i in 1u32..=10 {
            detector.record_chain(i * 5);
            total += (i * 5) as u64;

            assert_eq!(detector.chain_count(), i as u64);
            assert_eq!(detector.total_chain_length(), total);
        }

        // Final average: (5 + 10 + 15 + ... + 50) / 10 = 275 / 10 = 27.5
        assert_eq!(detector.chain_count(), 10);
        assert_eq!(detector.total_chain_length(), 275);
        assert!((detector.average_chain_length() - 27.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_chain_instrumentation_zero_length_chain() {
        let mut detector = LinearDetector::new();

        // Record a zero-length chain (edge case)
        detector.record_chain(0);

        assert_eq!(detector.chain_count(), 1);
        assert_eq!(detector.total_chain_length(), 0);
        assert_eq!(detector.average_chain_length(), 0.0);
    }

    #[test]
    fn test_chain_instrumentation_large_chain() {
        let mut detector = LinearDetector::new();

        // Record a large chain (test u32 to u64 conversion)
        let large_length: u32 = 1_000_000;
        detector.record_chain(large_length);

        assert_eq!(detector.chain_count(), 1);
        assert_eq!(detector.total_chain_length(), 1_000_000);
        assert_eq!(detector.average_chain_length(), 1_000_000.0);
    }

    #[test]
    fn test_chain_instrumentation_reset_clears_metrics() {
        let mut detector = LinearDetector::new();

        // Record some chains
        detector.record_chain(10);
        detector.record_chain(20);
        detector.record_chain(30);

        assert_eq!(detector.chain_count(), 3);
        assert_eq!(detector.total_chain_length(), 60);
        assert_eq!(detector.average_chain_length(), 20.0);

        // Reset should clear instrumentation
        detector.reset();

        assert_eq!(detector.chain_count(), 0);
        assert_eq!(detector.total_chain_length(), 0);
        assert_eq!(detector.average_chain_length(), 0.0);
    }

    #[test]
    fn test_chain_instrumentation_with_pattern_detection() {
        let mut detector = LinearDetector::new();

        // Record chains while pattern detection works
        detector.observe(1, 1);
        detector.observe(2, 1);
        detector.observe(3, 1);
        assert!(detector.is_linear_confirmed());

        // Simulate chain detection after pattern confirmation
        detector.record_chain(3);

        assert_eq!(detector.chain_count(), 1);
        assert_eq!(detector.total_chain_length(), 3);
        assert_eq!(detector.average_chain_length(), 3.0);
        // Pattern detection state unchanged
        assert!(detector.is_linear_confirmed());
    }

    #[test]
    fn test_chain_instrumentation_with_threshold() {
        let mut detector = LinearDetector::with_threshold(5);

        // Custom threshold detector should still track chains
        detector.record_chain(7);
        detector.record_chain(13);

        assert_eq!(detector.chain_count(), 2);
        assert_eq!(detector.total_chain_length(), 20);
        assert_eq!(detector.average_chain_length(), 10.0);
    }

    // Phase 33 Plan 03: Sequential read trigger tests

    #[test]
    fn test_should_use_sequential_read_returns_false_before_threshold() {
        let mut detector = LinearDetector::new();

        // Before threshold: false
        detector.observe_with_cluster(1, 1, 1024, 4096);
        assert!(!detector.should_use_sequential_read());

        // Still before threshold: false
        detector.observe_with_cluster(2, 1, 5120, 4096);
        assert!(!detector.should_use_sequential_read());

        // Pattern not confirmed yet
        assert!(!detector.is_linear_confirmed());
    }

    #[test]
    fn test_should_use_sequential_read_returns_false_for_non_contiguous() {
        let mut detector = LinearDetector::new();

        // Linear pattern confirmed but non-contiguous clusters
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        detector.observe_with_cluster(3, 1, 10000, 4096); // Gap: should be 9216

        // Linear confirmed, but clusters not contiguous
        assert!(detector.is_linear_confirmed());
        assert!(!detector.validate_contiguity());
        assert!(!detector.should_use_sequential_read());
    }

    #[test]
    fn test_should_use_sequential_read_returns_true_for_linear_and_contiguous() {
        let mut detector = LinearDetector::new();

        // Linear pattern confirmed + contiguous clusters
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        detector.observe_with_cluster(3, 1, 9216, 4096);

        // Both conditions met
        assert!(detector.is_linear_confirmed());
        assert!(detector.validate_contiguity());
        assert!(detector.should_use_sequential_read());
    }

    #[test]
    fn test_should_use_sequential_read_returns_false_for_branching() {
        let mut detector = LinearDetector::new();

        // Branching detected immediately
        detector.observe_with_cluster(1, 2, 1024, 4096);

        // Not linear, even if clusters happen to be contiguous
        assert!(!detector.is_linear_confirmed());
        assert!(!detector.should_use_sequential_read());
    }

    #[test]
    fn test_should_use_sequential_read_linear_then_branching() {
        let mut detector = LinearDetector::new();

        // First: linear + contiguous = true
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        detector.observe_with_cluster(3, 1, 9216, 4096);
        assert!(detector.should_use_sequential_read());

        // Then branching: false
        detector.observe_with_cluster(4, 2, 13312, 8192);
        assert!(!detector.should_use_sequential_read());
        assert!(!detector.is_linear_confirmed());
    }

    #[test]
    fn test_should_use_sequential_read_single_cluster_returns_false() {
        let mut detector = LinearDetector::new();

        // Single cluster: not enough for contiguity check
        detector.observe_with_cluster(1, 1, 1024, 4096);
        assert!(!detector.should_use_sequential_read());

        // Even if linear pattern is confirmed after more observations,
        // we need at least 2 clusters for contiguity
        detector.observe_with_cluster(2, 1, 5120, 4096);
        detector.observe_with_cluster(3, 1, 9216, 4096);
        assert!(detector.should_use_sequential_read());
    }

    #[test]
    fn test_should_use_sequential_read_reset_clears_state() {
        let mut detector = LinearDetector::new();

        // First: true
        detector.observe_with_cluster(1, 1, 1024, 4096);
        detector.observe_with_cluster(2, 1, 5120, 4096);
        detector.observe_with_cluster(3, 1, 9216, 4096);
        assert!(detector.should_use_sequential_read());

        // Reset: false
        detector.reset();
        assert!(!detector.should_use_sequential_read());
    }

    #[test]
    fn test_should_use_sequential_read_with_custom_threshold() {
        let mut detector = LinearDetector::with_threshold(5);

        // Need 5 observations with custom threshold
        for i in 0..5 {
            let offset = (i as u64) * 4096;
            detector.observe_with_cluster(i, 1, offset, 4096);
        }

        // Now both conditions met
        assert!(detector.is_linear_confirmed());
        assert!(detector.validate_contiguity());
        assert!(detector.should_use_sequential_read());
    }

    #[test]
    fn test_should_use_sequential_read_variable_cluster_sizes() {
        let mut detector = LinearDetector::new();

        // Different cluster sizes, but contiguous
        // 0 + 100 = 100, 100 + 200 = 300, 300 + 150 = 450
        detector.observe_with_cluster(1, 1, 0, 100);
        detector.observe_with_cluster(2, 1, 100, 200);
        detector.observe_with_cluster(3, 1, 300, 150);

        assert!(detector.is_linear_confirmed());
        assert!(detector.validate_contiguity());
        assert!(detector.should_use_sequential_read());
    }

    #[test]
    fn test_should_use_sequential_read_dead_end_returns_false() {
        let mut detector = LinearDetector::new();

        // Dead ends (degree 0) don't confirm linear pattern
        detector.observe_with_cluster(1, 0, 1024, 4096);
        assert!(!detector.should_use_sequential_read());

        detector.observe_with_cluster(2, 0, 5120, 4096);
        assert!(!detector.should_use_sequential_read());

        // Even with multiple observations, degree 0 keeps us in Unknown
        assert!(!detector.is_linear_confirmed());
        assert!(!detector.should_use_sequential_read());
    }

    // Phase 33 Plan 05: Integration tests for graph patterns

    #[test]
    fn test_chain_detection_on_linear_graph() {
        let mut detector = LinearDetector::new();

        // Simulate linear chain of 100 nodes with contiguous cluster storage
        // Each cluster is 4096 bytes (4KB), starting at offset 0
        let cluster_size = 4096u64;
        let mut current_offset = 0u64;

        for node_id in 1..=100 {
            detector.observe_with_cluster(node_id, 1, current_offset, cluster_size as u32);
            current_offset += cluster_size;
        }

        // After 100 linear nodes with contiguous clusters, should use sequential read
        assert!(detector.is_linear_confirmed());
        assert!(detector.validate_contiguity());
        assert!(detector.should_use_sequential_read());

        // Verify all 100 cluster offsets were recorded
        assert_eq!(detector.cluster_offsets().len(), 100);

        // Verify first and last offsets
        assert_eq!(detector.cluster_offsets()[0], (0, 4096));
        assert_eq!(
            detector.cluster_offsets()[99],
            (99 * 4096, 4096)
        );
    }

    #[test]
    fn test_no_false_positive_on_tree() {
        let mut detector = LinearDetector::new();

        // Simulate a binary tree with 31 nodes (depth 4)
        // Structure: level 0 has 1 node (root), level 1 has 2 nodes, etc.
        // In a BFS traversal, degrees vary:
        // - Root (node 1): degree 2 (has 2 children)
        // - Internal nodes: degree 3 (1 parent + 2 children, except leaves)
        // - Leaf nodes: degree 1 (only parent)

        // Node 1: root with degree 2 (branches immediately)
        detector.observe_with_cluster(1, 2, 0, 4096);

        // Already in Branching state - should NOT use sequential read
        assert!(!detector.is_linear_confirmed());
        assert!(!detector.should_use_sequential_read());

        // Even if we observe more nodes (simulating traversal continues)
        detector.observe_with_cluster(2, 3, 4096, 4096);
        detector.observe_with_cluster(3, 3, 8192, 4096);
        detector.observe_with_cluster(4, 1, 12288, 4096);

        // Still NOT linear - tree pattern detected
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.current_pattern(), TraversalPattern::Branching);
        assert!(!detector.should_use_sequential_read());
    }

    #[test]
    fn test_no_false_positive_on_diamond() {
        let mut detector = LinearDetector::new();

        // Diamond pattern: A -> B, C; B, C -> D
        // Node sequence: A(1), B(2), C(2), D(2)
        // Degrees shown in parentheses

        // Node A: degree 2 (branches to B and C)
        detector.observe_with_cluster(1, 2, 0, 4096);

        // Immediately in Branching state
        assert_eq!(detector.current_pattern(), TraversalPattern::Branching);
        assert!(!detector.is_linear_confirmed());
        assert!(!detector.should_use_sequential_read());

        // Continue traversal through B, C, D
        // Even if clusters happen to be contiguous, pattern is NOT linear
        detector.observe_with_cluster(2, 2, 4096, 4096); // Node B
        detector.observe_with_cluster(3, 2, 8192, 4096); // Node C
        detector.observe_with_cluster(4, 2, 12288, 4096); // Node D

        // Diamond pattern correctly detected as non-linear
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.current_pattern(), TraversalPattern::Branching);
        assert!(!detector.should_use_sequential_read());
    }

    #[test]
    fn test_mixed_pattern_detection() {
        let mut detector = LinearDetector::new();

        // Create graph with 5 linear nodes then branch
        // Nodes 1-5: degree 1 (linear prefix)
        // Node 6: degree 2 (branching point)

        // Linear prefix: nodes 1-5 with contiguous clusters
        for i in 1..=5 {
            let offset = ((i - 1) * 4096) as u64;
            detector.observe_with_cluster(i, 1, offset, 4096);
        }

        // After 5 linear nodes, should be confirmed
        assert!(detector.is_linear_confirmed());
        assert!(detector.validate_contiguity());
        assert!(detector.should_use_sequential_read());

        // Node 6: branching starts (degree 2)
        detector.observe_with_cluster(6, 2, 5 * 4096, 4096);

        // After branching, should NOT use sequential read
        assert!(!detector.is_linear_confirmed());
        assert_eq!(detector.current_pattern(), TraversalPattern::Branching);
        assert!(!detector.should_use_sequential_read());
    }

    #[test]
    fn test_non_contiguous_linear_chain() {
        let mut detector = LinearDetector::new();

        // Create linear chain where clusters are NOT contiguous
        // All nodes have degree 1 (linear pattern)
        // But clusters have gaps between them

        // Node 1: offset 0
        detector.observe_with_cluster(1, 1, 0, 4096);

        // Node 2: offset 4096 (contiguous so far)
        detector.observe_with_cluster(2, 1, 4096, 4096);

        // Node 3: offset 8192 (contiguous so far)
        detector.observe_with_cluster(3, 1, 8192, 4096);

        // Linear pattern confirmed (3+ degree-1 nodes)
        assert!(detector.is_linear_confirmed());

        // But clusters ARE contiguous so far
        assert!(detector.validate_contiguity());

        // Node 4: offset 20000 (GAP! should be 12280)
        // This breaks contiguity
        detector.observe_with_cluster(4, 1, 20000, 4096);

        // Linear pattern still confirmed (all degree 1)
        assert!(detector.is_linear_confirmed());

        // But clusters are NOT contiguous due to gap
        assert!(!detector.validate_contiguity());

        // Therefore, should NOT use sequential read
        assert!(!detector.should_use_sequential_read());
    }

    // === 40-10: predicted_chain_length() Tests ===

    #[test]
    fn test_predicted_chain_length_initial_state() {
        let detector = LinearDetector::new();
        assert_eq!(detector.predicted_chain_length(), 0);
        assert_eq!(detector.observed_length(), 0);
    }

    #[test]
    fn test_predicted_chain_length_accumulating() {
        let mut detector = LinearDetector::new();

        // First observation: Unknown -> Accumulating
        detector.observe(1, 1);
        assert_eq!(detector.predicted_chain_length(), 1);

        // Second observation: still Accumulating
        detector.observe(2, 1);
        assert_eq!(detector.predicted_chain_length(), 2);
    }

    #[test]
    fn test_predicted_chain_length_linear_confirmed() {
        let mut detector = LinearDetector::new();

        // Reach threshold (3 for default)
        detector.observe(1, 1);
        detector.observe(2, 1);
        detector.observe(3, 1);

        assert!(detector.is_linear_confirmed());
        // With observe() (not observe_with_cluster), cluster_offsets is empty
        // So predicted uses consecutive_linear
        assert_eq!(detector.predicted_chain_length(), 3);
    }

    #[test]
    fn test_predicted_chain_length_with_clusters() {
        let mut detector = LinearDetector::new();

        // Use observe_with_cluster to populate cluster_offsets
        detector.observe_with_cluster(1, 1, 0, 4096);
        detector.observe_with_cluster(2, 1, 4096, 4096);
        detector.observe_with_cluster(3, 1, 8192, 4096);

        assert!(detector.is_linear_confirmed());
        // With cluster_offsets populated, uses cluster_offsets.len()
        assert_eq!(detector.predicted_chain_length(), 3);
        assert_eq!(detector.cluster_offsets().len(), 3);
    }

    #[test]
    fn test_predicted_chain_length_branching() {
        let mut detector = LinearDetector::new();

        // Immediate branching
        detector.observe(1, 2);

        assert_eq!(detector.predicted_chain_length(), 0);
        assert!(!detector.is_linear_confirmed());
    }

    #[test]
    fn test_predicted_chain_length_dead_end() {
        let mut detector = LinearDetector::new();

        // Dead end (degree 0) stays in Unknown
        detector.observe(1, 0);

        assert_eq!(detector.predicted_chain_length(), 0);
        assert!(!detector.is_linear_confirmed());
    }

    #[test]
    fn test_predicted_chain_length_long_chain() {
        let mut detector = LinearDetector::new();

        // Simulate a long linear chain using observe_with_cluster
        // to populate cluster_offsets for accurate prediction
        for i in 0..15 {
            let offset = (i * 4096) as u64;
            detector.observe_with_cluster(i, 1, offset, 4096);
        }

        // With observe_with_cluster, cluster_offsets.len() gives accurate count
        assert_eq!(detector.predicted_chain_length(), 15);
        assert!(detector.is_linear_confirmed());
        assert_eq!(detector.cluster_offsets().len(), 15);
    }

    #[test]
    fn test_predicted_chain_length_long_chain_with_observe_only() {
        let mut detector = LinearDetector::new();

        // When using observe() (not observe_with_cluster), consecutive_linear
        // stops incrementing at threshold, so prediction is conservative
        for i in 0..15 {
            detector.observe(i, 1);
        }

        // observe() doesn't populate cluster_offsets, so prediction
        // uses consecutive_linear which stops at threshold
        assert_eq!(detector.predicted_chain_length(), 3); // threshold value
        assert!(detector.is_linear_confirmed());
        assert_eq!(detector.cluster_offsets().len(), 0); // no clusters recorded
    }

    #[test]
    fn test_predicted_chain_length_linear_then_branching() {
        let mut detector = LinearDetector::new();

        // Build linear chain
        detector.observe(1, 1);
        detector.observe(2, 1);
        detector.observe(3, 1);
        assert_eq!(detector.predicted_chain_length(), 3);

        // Branch
        detector.observe(4, 2);

        // After branching, prediction returns 0
        assert_eq!(detector.predicted_chain_length(), 0);
        assert!(!detector.is_linear_confirmed());
    }

    #[test]
    fn test_predicted_chain_length_reset() {
        let mut detector = LinearDetector::new();

        detector.observe(1, 1);
        detector.observe(2, 1);
        assert_eq!(detector.predicted_chain_length(), 2);

        detector.reset();
        assert_eq!(detector.predicted_chain_length(), 0);
    }

    #[test]
    fn test_predicted_chain_length_with_custom_threshold() {
        let mut detector = LinearDetector::with_threshold(5);

        // Below custom threshold
        for i in 0..3 {
            detector.observe(i, 1);
        }
        // Still accumulating, returns consecutive_linear
        assert_eq!(detector.predicted_chain_length(), 3);
        assert!(!detector.is_linear_confirmed());

        // Reach custom threshold
        detector.observe(3, 1);
        detector.observe(4, 1);
        assert!(detector.is_linear_confirmed());
        assert_eq!(detector.predicted_chain_length(), 5);
    }

    #[test]
    fn test_observed_length_helper() {
        let mut detector = LinearDetector::new();

        assert_eq!(detector.observed_length(), 0);

        detector.observe(1, 1);
        assert_eq!(detector.observed_length(), 1);

        detector.observe(2, 1);
        assert_eq!(detector.observed_length(), 2);

        detector.observe(3, 1);
        assert_eq!(detector.observed_length(), 3);

        // Branching resets state
        detector.observe(4, 2);
        // observed_length returns raw consecutive_linear (0 after branching)
        assert_eq!(detector.observed_length(), 0);
    }
}
