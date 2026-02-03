//! Taint analysis for security and compliance vulnerability detection.
//!
//! This module provides taint propagation algorithms for tracking untrusted data
//! from sources to sinks in directed graphs. Taint analysis is a fundamental technique
//! for security vulnerability detection, enabling identification of:
//!
//! - **SQL injection**: User input reaching SQL query construction
//! - **Cross-site scripting (XSS)**: Untrusted data flowing to HTML output
//! - **Command injection**: User input flowing to shell command execution
//! - **Path traversal**: File names from user input reaching file operations
//! - **LDAP injection**: User input reaching LDAP queries
//!
//! # What is Taint Analysis?
//!
//! Taint analysis marks data from untrusted sources as "tainted" and tracks
//! its flow through the program. If tainted data reaches a security-sensitive
//! sink without proper sanitization, a potential vulnerability is reported.
//!
//! ## Core Concepts
//!
//! - **Source**: Untrusted data entry point (user input, environment variables, network data)
//! - **Sink**: Security-sensitive operation (SQL query, HTML output, command execution)
//! - **Taint propagation**: Tracking flow from sources to sinks via graph reachability
//! - **Vulnerability**: Path exists from source to sink (taint reaches sink)
//!
//! # Algorithm
//!
//! Taint propagation is fundamentally a **reachability problem**:
//!
//! ## Forward Taint Analysis
//! 1. Start from taint source nodes
//! 2. Compute forward reachability using BFS (follow data flow)
//! 3. Check if any sink nodes are in the reachable set
//! 4. Report source-sink paths as vulnerabilities
//!
//! ## Backward Taint Analysis
//! 1. Start from sink node
//! 2. Compute backward reachability using reverse BFS (trace data sources)
//! 3. Find which taint sources can reach the sink
//! 4. Report affecting sources as vulnerabilities
//!
//! Both directions reuse the reachability algorithms from [`reachability`],
//! treating taint analysis as annotated reachability with source/sink semantics.
//!
//! # Complexity
//!
//! - **Time**: O(|V| + |E|) - BFS traversal for reachability
//! - **Space**: O(|V|) - for tainted nodes set and BFS queue
//!
//! Where:
//! - V = number of vertices
//! - E = number of edges
//!
//! # Example
//!
//! ```rust,ignore
//! use sqlitegraph::{
//!     SqliteGraph,
//!     algo::{
//!         propagate_taint_forward,
//!         discover_sources_and_sinks_default,
//!     },
//! };
//!
//! let graph = SqliteGraph::open_in_memory()?;
//! // ... build graph with source/sink annotations ...
//!
//! // Discover sources and sinks automatically
//! let (sources, sinks) = discover_sources_and_sinks_default(&graph)?;
//!
//! // Forward analysis: Find all vulnerabilities
//! let result = propagate_taint_forward(&graph, &sources, &sinks)?;
//!
//! if result.has_vulnerability() {
//!     println!("SECURITY ISSUE: {} tainted nodes reach {} sinks",
//!              result.tainted_nodes.len(),
//!              result.sinks_reached.len());
//!
//!     for (source, sink) in result.sorted_vulnerabilities() {
//!         println!("  Vulnerability: source {} reaches sink {}", source, sink);
//!     }
//! }
//! ```
//!
//! # Source and Sink Detection
//!
//! Sources and sinks can be detected via:
//!
//! 1. **Metadata-based**: Using [`MetadataSourceDetector`] and [`MetadataSinkDetector`]
//!    which check entity `data` field for `"kind": "source"` or `"kind": "sink"`
//! 2. **Custom callbacks**: Implement [`SourceCallback`] and [`SinkCallback`] traits
//!    for domain-specific detection logic
//!
//! # References
//!
//! - Livshits, B., & Lam, M. S. "Finding security vulnerabilities in Java applications
//!   with static analysis." USENIX Security, 2005.
//! - Tripp, O., et al. "Taint propagation with demand-driven flow analysis." ACM TOSEM, 2013.
//!
//! [`reachability`]: crate::algo::reachability

use std::collections::VecDeque;

use ahash::{AHashMap, AHashSet};

use crate::graph::types::GraphEntity;
use crate::progress::ProgressCallback;
use crate::{errors::SqliteGraphError, graph::SqliteGraph};

use super::reachability::{can_reach, reachable_from, reverse_reachable_from};

/// Result of taint analysis operation.
///
/// Contains the complete taint propagation result with sources, sinks reached,
/// tainted nodes, and source-sink vulnerability paths.
///
/// # Fields
///
/// - `sources`: Source nodes that were analyzed (taint origins)
/// - `sinks_reached`: Sink nodes that are reachable from sources (vulnerabilities)
/// - `tainted_nodes`: All nodes tainted by source data (forward reachability from sources)
/// - `source_sink_paths`: Vulnerability paths as (source, sink) pairs
/// - `size`: Number of tainted nodes (len of tainted_nodes)
///
/// # Example
///
/// ```rust,ignore
/// let result = propagate_taint_forward(&graph, &sources, &sinks)?;
///
/// println!("Tainted {} nodes from {} sources",
///          result.size, result.sources.len());
///
/// // Check if specific node is tainted
/// if result.is_tainted(node_id) {
///     println!("Node {} is tainted", node_id);
/// }
///
/// // Check for vulnerabilities
/// if result.has_vulnerability() {
///     println!("Found {} source-sink vulnerabilities",
///              result.source_sink_paths.len());
///
///     for (source, sink) in result.sorted_vulnerabilities() {
///         println!("  VULN: source {} -> sink {}", source, sink);
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TaintResult {
    /// Source nodes that taint originated from
    pub sources: AHashSet<i64>,

    /// Sink nodes that were reached by tainted data (vulnerabilities)
    pub sinks_reached: AHashSet<i64>,

    /// All nodes that are tainted (reachable from any source)
    pub tainted_nodes: AHashSet<i64>,

    /// Source-sink paths that represent vulnerabilities
    pub source_sink_paths: Vec<(i64, i64)>,

    /// Number of tainted nodes
    pub size: usize,
}

impl TaintResult {
    /// Creates a new empty taint result.
    pub fn new() -> Self {
        Self {
            sources: AHashSet::new(),
            sinks_reached: AHashSet::new(),
            tainted_nodes: AHashSet::new(),
            source_sink_paths: Vec::new(),
            size: 0,
        }
    }

    /// Checks if a node is tainted.
    ///
    /// Returns true if the node is in the tainted_nodes set.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// if result.is_tainted(node_id) {
    ///     println!("Node {} is tainted", node_id);
    /// }
    /// ```
    pub fn is_tainted(&self, node: i64) -> bool {
        self.tainted_nodes.contains(&node)
    }

    /// Checks if any vulnerabilities were found.
    ///
    /// Returns true if any sink is reachable from any source.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// if result.has_vulnerability() {
    ///     println!("SECURITY ISSUE: tainted data reaches sensitive sink!");
    /// }
    /// ```
    pub fn has_vulnerability(&self) -> bool {
        !self.sinks_reached.is_empty()
    }

    /// Returns sorted list of tainted node IDs.
    ///
    /// Provides deterministic output for testing and reporting.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// for node in result.sorted_tainted_nodes() {
    ///     println!("Tainted node: {}", node);
    /// }
    /// ```
    pub fn sorted_tainted_nodes(&self) -> Vec<i64> {
        let mut nodes: Vec<i64> = self.tainted_nodes.iter().copied().collect();
        nodes.sort();
        nodes
    }

    /// Returns sorted list of vulnerability paths.
    ///
    /// Each vulnerability is a (source, sink) pair.
    /// Sorted by source then sink for deterministic output.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// for (source, sink) in result.sorted_vulnerabilities() {
    ///     println!("VULN: source {} -> sink {}", source, sink);
    /// }
    /// ```
    pub fn sorted_vulnerabilities(&self) -> Vec<(i64, i64)> {
        let mut paths = self.source_sink_paths.clone();
        paths.sort();
        paths
    }
}

impl Default for TaintResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Callback trait for detecting taint sources.
///
/// Implement this trait to define custom source detection logic.
/// The callback receives a node ID and its entity data, returning true
/// if the node is a taint source.
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::algo::{SourceCallback, TaintResult};
/// use sqlitegraph::graph::types::GraphEntity;
///
/// struct HttpParamDetector;
///
/// impl SourceCallback for HttpParamDetector {
///     fn is_source(&self, node: i64, entity: &GraphEntity) -> bool {
///         // Detect HTTP parameter nodes
///         entity.kind == "http_param" ||
///         entity.data["taint"].as_str() == Some("untrusted")
///     }
/// }
/// ```
pub trait SourceCallback {
    /// Checks if a node is a taint source.
    ///
    /// # Arguments
    /// * `node` - Node ID to check
    /// * `entity` - Graph entity containing metadata
    ///
    /// # Returns
    /// true if the node is a taint source, false otherwise
    fn is_source(&self, node: i64, entity: &GraphEntity) -> bool;
}

/// Callback trait for detecting security-sensitive sinks.
///
/// Implement this trait to define custom sink detection logic.
/// The callback receives a node ID and its entity data, returning true
/// if the node is a security-sensitive sink.
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::algo::{SinkCallback, TaintResult};
/// use sqlitegraph::graph::types::GraphEntity;
///
/// struct SqlQueryDetector;
///
/// impl SinkCallback for SqlQueryDetector {
///     fn is_sink(&self, node: i64, entity: &GraphEntity) -> bool {
///         // Detect SQL query execution nodes
///         entity.kind == "sql_execute" ||
///         entity.data["operation"].as_str() == Some("query")
///     }
/// }
/// ```
pub trait SinkCallback {
    /// Checks if a node is a security-sensitive sink.
    ///
    /// # Arguments
    /// * `node` - Node ID to check
    /// * `entity` - Graph entity containing metadata
    ///
    /// # Returns
    /// true if the node is a sink, false otherwise
    fn is_sink(&self, node: i64, entity: &GraphEntity) -> bool;
}

/// Default source detector using entity metadata.
///
/// Checks for common source indicators in entity data field:
/// - `"kind": "source"`
/// - `"kind": "untrusted"`
/// - `"kind": "user_input"`
/// - `"taint": "source"`
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::algo::{discover_sources_and_sinks, MetadataSourceDetector};
///
/// let (sources, sinks) = discover_sources_and_sinks(
///     &graph,
///     &MetadataSourceDetector,
///     &MetadataSinkDetector,
/// )?;
/// ```
pub struct MetadataSourceDetector;

impl SourceCallback for MetadataSourceDetector {
    fn is_source(&self, _node: i64, entity: &GraphEntity) -> bool {
        // Check data.kind field
        if let Some(kind) = entity.data.get("kind").and_then(|k| k.as_str()) {
            if matches!(kind, "source" | "untrusted" | "user_input") {
                return true;
            }
        }

        // Check data.taint field
        if let Some(taint) = entity.data.get("taint").and_then(|t| t.as_str()) {
            if taint == "source" {
                return true;
            }
        }

        false
    }
}

/// Default sink detector using entity metadata.
///
/// Checks for common sink indicators in entity data field:
/// - `"kind": "sink"`
/// - `"kind": "sql_query"`
/// - `"kind": "html_output"`
/// - `"kind": "command"`
/// - `"operation": "execute"`
/// - `"operation": "query"`
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::algo::{discover_sources_and_sinks, MetadataSinkDetector};
///
/// let (sources, sinks) = discover_sources_and_sinks(
///     &graph,
///     &MetadataSourceDetector,
///     &MetadataSinkDetector,
/// )?;
/// ```
pub struct MetadataSinkDetector;

impl SinkCallback for MetadataSinkDetector {
    fn is_sink(&self, _node: i64, entity: &GraphEntity) -> bool {
        // Check data.kind field
        if let Some(kind) = entity.data.get("kind").and_then(|k| k.as_str()) {
            if matches!(
                kind,
                "sink" | "sql_query" | "html_output" | "command" | "file_operation"
            ) {
                return true;
            }
        }

        // Check data.operation field
        if let Some(operation) = entity.data.get("operation").and_then(|o| o.as_str()) {
            if matches!(operation, "execute" | "query" | "render" | "write") {
                return true;
            }
        }

        false
    }
}

/// Propagates taint forward from sources to find reachable sinks.
///
/// Computes all nodes reachable from taint sources and identifies which
/// security-sensitive sinks are reachable (vulnerabilities).
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `sources` - Source node IDs where taint originates
/// * `sinks` - Sink node IDs to check for taint reachability
///
/// # Returns
/// TaintResult containing:
/// - All nodes tainted by sources (forward reachable)
/// - Which sinks are reachable (vulnerabilities)
/// - Source-sink paths that represent vulnerabilities
///
/// # Complexity
/// - **Time**: O(S × (V + E) + S × Sinks × (V + E)) where S = sources count
///   - O(V + E) per source for forward reachability
///   - O(V + E) per source-sink pair for path validation
/// - **Space**: O(V) for tainted nodes set
///
/// # Algorithm
/// 1. Initialize tainted_nodes = empty set
/// 2. For each source:
///    - Compute forward reachability using reachable_from()
///    - Extend tainted_nodes with reachable nodes
/// 3. Compute sinks_reached = sinks ∩ tainted_nodes
/// 4. Build source_sink_paths:
///    - For each source and sink pair, check can_reach(source, sink)
///    - If true, add (source, sink) to paths
/// 5. Return TaintResult with size = tainted_nodes.len()
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{SqliteGraph, algo::propagate_taint_forward};
///
/// let graph = SqliteGraph::open_in_memory()?;
/// // ... build graph with sources and sinks ...
///
/// let sources = vec![1, 2];   // User input nodes
/// let sinks = vec![10, 20];   // SQL query nodes
///
/// let result = propagate_taint_forward(&graph, &sources, &sinks)?;
///
/// if result.has_vulnerability() {
///     println!("Found {} vulnerabilities", result.source_sink_paths.len());
///     for (source, sink) in result.sorted_vulnerabilities() {
///         println!("  Source {} can reach sink {}", source, sink);
///     }
/// }
/// ```
pub fn propagate_taint_forward(
    graph: &SqliteGraph,
    sources: &[i64],
    sinks: &[i64],
) -> Result<TaintResult, SqliteGraphError> {
    let mut tainted_nodes: AHashSet<i64> = AHashSet::new();
    let sources_set: AHashSet<i64> = sources.iter().copied().collect();
    let sinks_set: AHashSet<i64> = sinks.iter().copied().collect();

    // Step 1: Propagate taint from each source
    for &source in sources {
        let reachable = reachable_from(graph, source)?;
        tainted_nodes.extend(reachable);
    }

    // Step 2: Find which sinks are reachable (vulnerabilities)
    let sinks_reached: AHashSet<i64> = sinks_set
        .intersection(&tainted_nodes)
        .copied()
        .collect();

    // Step 3: Build source-sink paths
    let mut source_sink_paths = Vec::new();
    for &source in sources {
        for &sink in &sinks_reached {
            if can_reach(graph, source, sink)? {
                source_sink_paths.push((source, sink));
            }
        }
    }

    Ok(TaintResult {
        sources: sources_set,
        sinks_reached,
        tainted_nodes,
        source_sink_paths,
        size: tainted_nodes.len(),
    })
}

/// Propagates taint forward with progress tracking.
///
/// Same algorithm as [`propagate_taint_forward`] but reports progress
/// during execution. Useful for long-running operations on large graphs.
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `sources` - Source node IDs where taint originates
/// * `sinks` - Sink node IDs to check for taint reachability
/// * `progress` - Progress callback for reporting execution status
///
/// # Returns
/// TaintResult with tainted nodes and vulnerability paths.
///
/// # Progress Reporting
///
/// The callback receives:
/// - `current`: Current number of sources processed
/// - `total`: Total number of sources (Some(total))
/// - `message`: "Taint propagation: {current}/{total} sources processed, {tainted} tainted nodes"
///
/// Progress is reported for each source processed.
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{
///     algo::propagate_taint_forward_with_progress,
///     progress::ConsoleProgress
/// };
///
/// let progress = ConsoleProgress::new();
/// let result = propagate_taint_forward_with_progress(&graph, &sources, &sinks, &progress)?;
/// // Output: Taint propagation: 1/5 sources processed, 10 tainted nodes
/// ```
pub fn propagate_taint_forward_with_progress<F>(
    graph: &SqliteGraph,
    sources: &[i64],
    sinks: &[i64],
    progress: &F,
) -> Result<TaintResult, SqliteGraphError>
where
    F: ProgressCallback,
{
    let mut tainted_nodes: AHashSet<i64> = AHashSet::new();
    let sources_set: AHashSet<i64> = sources.iter().copied().collect();
    let sinks_set: AHashSet<i64> = sinks.iter().copied().collect();
    let total = sources.len();

    // Step 1: Propagate taint from each source with progress
    for (idx, &source) in sources.iter().enumerate() {
        let reachable = reachable_from(graph, source)?;
        tainted_nodes.extend(reachable);

        // Report progress
        progress.on_progress(
            idx + 1,
            Some(total),
            &format!(
                "Taint propagation: {}/{} sources processed, {} tainted nodes",
                idx + 1,
                total,
                tainted_nodes.len()
            ),
        );
    }

    // Step 2: Find which sinks are reachable (vulnerabilities)
    let sinks_reached: AHashSet<i64> = sinks_set
        .intersection(&tainted_nodes)
        .copied()
        .collect();

    // Step 3: Build source-sink paths
    let mut source_sink_paths = Vec::new();
    for &source in sources {
        for &sink in &sinks_reached {
            if can_reach(graph, source, sink)? {
                source_sink_paths.push((source, sink));
            }
        }
    }

    // Report completion
    progress.on_complete();

    Ok(TaintResult {
        sources: sources_set,
        sinks_reached,
        tainted_nodes,
        source_sink_paths,
        size: tainted_nodes.len(),
    })
}

/// Propagates taint backward from a sink to find affecting sources.
///
/// Computes all nodes that can reach the sink (ancestors) and identifies
/// which taint sources can influence the sink.
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `sink` - The sink node ID to trace back from
/// * `sources` - Source node IDs to check for influence
///
/// # Returns
/// TaintResult containing:
/// - sources: Sources that can reach the sink (affecting sources)
/// - sinks_reached: Contains only the input sink
/// - tainted_nodes: All ancestors that can reach the sink
/// - source_sink_paths: (affecting_source, sink) pairs
///
/// # Complexity
/// - **Time**: O(V + E) for backward reachability + O(S) for source intersection
///   where S = sources count
/// - **Space**: O(V) for ancestors set
///
/// # Algorithm
/// 1. Call reverse_reachable_from(graph, sink) to get ancestors
/// 2. Find affecting_sources = sources ∩ ancestors
/// 3. Build source_sink_paths: (source, sink) for each affecting source
/// 4. Return TaintResult with:
///    - sources = affecting_sources
///    - sinks_reached = {sink}
///    - tainted_nodes = ancestors
///    - source_sink_paths = (affecting_source, sink) for each
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{SqliteGraph, algo::propagate_taint_backward};
///
/// let graph = SqliteGraph::open_in_memory()?;
/// // ... build graph with sources and sinks ...
///
/// let sources = vec![1, 2, 3];   // User input nodes
/// let sink = 10;                 // SQL query node
///
/// let result = propagate_taint_backward(&graph, sink, &sources)?;
///
/// println!("Sink {} is affected by {} sources",
///          sink, result.sources.len());
/// for &source in &result.sources {
///     println!("  Source {} can reach sink {}", source, sink);
/// }
/// ```
pub fn propagate_taint_backward(
    graph: &SqliteGraph,
    sink: i64,
    sources: &[i64],
) -> Result<TaintResult, SqliteGraphError> {
    let sources_set: AHashSet<i64> = sources.iter().copied().collect();

    // Step 1: Find all ancestors that can reach the sink
    let ancestors = reverse_reachable_from(graph, sink)?;

    // Step 2: Find which sources can reach the sink
    let affecting_sources: AHashSet<i64> = sources_set
        .intersection(&ancestors)
        .copied()
        .collect();

    // Step 3: Build source-sink paths
    let source_sink_paths: Vec<(i64, i64)> = affecting_sources
        .iter()
        .map(|&source| (source, sink))
        .collect();

    // Step 4: Build result
    let mut sinks_reached = AHashSet::new();
    sinks_reached.insert(sink);

    Ok(TaintResult {
        sources: affecting_sources,
        sinks_reached,
        tainted_nodes: ancestors,
        source_sink_paths,
        size: ancestors.len(),
    })
}

/// Propagates taint backward with progress tracking.
///
/// Same algorithm as [`propagate_taint_backward`] but reports progress
/// during execution.
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `sink` - The sink node ID to trace back from
/// * `sources` - Source node IDs to check for influence
/// * `progress` - Progress callback for reporting execution status
///
/// # Returns
/// TaintResult with affecting sources and ancestors.
///
/// # Progress Reporting
///
/// The callback receives:
/// - `current`: Always 1 (single operation)
/// - `total`: Always 1
/// - `message`: "Backward taint propagation: from sink {sink}, {affecting} sources found"
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{
///     algo::propagate_taint_backward_with_progress,
///     progress::ConsoleProgress
/// };
///
/// let progress = ConsoleProgress::new();
/// let result = propagate_taint_backward_with_progress(&graph, sink, &sources, &progress)?;
/// // Output: Backward taint propagation: from sink 10, 2 sources found
/// ```
pub fn propagate_taint_backward_with_progress<F>(
    graph: &SqliteGraph,
    sink: i64,
    sources: &[i64],
    progress: &F,
) -> Result<TaintResult, SqliteGraphError>
where
    F: ProgressCallback,
{
    let sources_set: AHashSet<i64> = sources.iter().copied().collect();

    // Step 1: Find all ancestors that can reach the sink
    let ancestors = reverse_reachable_from(graph, sink)?;

    // Step 2: Find which sources can reach the sink
    let affecting_sources: AHashSet<i64> = sources_set
        .intersection(&ancestors)
        .copied()
        .collect();

    // Report progress
    progress.on_progress(
        1,
        Some(1),
        &format!(
            "Backward taint propagation: from sink {}, {} sources found",
            sink,
            affecting_sources.len()
        ),
    );

    // Step 3: Build source-sink paths
    let source_sink_paths: Vec<(i64, i64)> = affecting_sources
        .iter()
        .map(|&source| (source, sink))
        .collect();

    // Step 4: Build result
    let mut sinks_reached = AHashSet::new();
    sinks_reached.insert(sink);

    // Report completion
    progress.on_complete();

    Ok(TaintResult {
        sources: affecting_sources,
        sinks_reached,
        tainted_nodes: ancestors,
        source_sink_paths,
        size: ancestors.len(),
    })
}

/// Performs sink reachability analysis for all sinks.
///
/// Analyzes each sink to determine which taint sources can reach it.
/// Returns a mapping of vulnerable sinks to their affecting sources.
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `sources` - Source node IDs where taint originates
/// * `sinks` - Sink node IDs to analyze
///
/// # Returns
/// HashMap mapping vulnerable sink -> Vec<affecting_sources>
/// Only includes sinks that have at least one affecting source.
///
/// # Complexity
/// - **Time**: O(Sinks × (V + E)) - one backward BFS per sink
/// - **Space**: O(V) for ancestors set (per sink)
///
/// # Algorithm
/// 1. Initialize result = empty HashMap
/// 2. For each sink:
///    - Call propagate_taint_backward(graph, sink, sources)
///    - If result.sources is non-empty (vulnerable):
///      - Add entry: sink -> Vec<affecting_sources>
/// 3. Return result mapping
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{SqliteGraph, algo::sink_reachability_analysis};
///
/// let graph = SqliteGraph::open_in_memory()?;
/// // ... build graph with sources and sinks ...
///
/// let sources = vec![1, 2, 3];   // User input nodes
/// let sinks = vec![10, 20, 30];  // SQL query nodes
///
/// let vulnerabilities = sink_reachability_analysis(&graph, &sources, &sinks)?;
///
/// if vulnerabilities.is_empty() {
///     println!("No vulnerabilities found!");
/// } else {
///     println!("Found {} vulnerable sinks:", vulnerabilities.len());
///     for (sink, affecting_sources) in vulnerabilities {
///         println!("  Sink {} is reachable from sources: {:?}", sink, affecting_sources);
///     }
/// }
/// ```
pub fn sink_reachability_analysis(
    graph: &SqliteGraph,
    sources: &[i64],
    sinks: &[i64],
) -> Result<AHashMap<i64, Vec<i64>>, SqliteGraphError> {
    let mut result: AHashMap<i64, Vec<i64>> = AHashMap::new();

    for &sink in sinks {
        let taint_result = propagate_taint_backward(graph, sink, sources)?;

        // Only include sinks that have affecting sources (vulnerabilities)
        if !taint_result.sources.is_empty() {
            let affecting_sources: Vec<i64> = taint_result.sources.iter().copied().collect();
            result.insert(sink, affecting_sources);
        }
    }

    Ok(result)
}

/// Performs sink reachability analysis with progress tracking.
///
/// Same algorithm as [`sink_reachability_analysis`] but reports progress
/// during execution. Useful for analyzing many sinks.
///
/// # Arguments
/// * `graph` - The graph to analyze
/// * `sources` - Source node IDs where taint originates
/// * `sinks` - Sink node IDs to analyze
/// * `progress` - Progress callback for reporting execution status
///
/// # Returns
/// HashMap mapping vulnerable sink -> Vec<affecting_sources>
///
/// # Progress Reporting
///
/// The callback receives:
/// - `current`: Current sink being analyzed
/// - `total`: Total number of sinks (Some(total))
/// - `message`: "Sink reachability: {current}/{total} sinks analyzed, {vulns} vulnerabilities found"
///
/// Progress is reported for each sink analyzed.
///
/// # Example
///
/// ```rust,ignore
/// use sqlitegraph::{
///     algo::sink_reachability_analysis_with_progress,
///     progress::ConsoleProgress
/// };
///
/// let progress = ConsoleProgress::new();
/// let vulnerabilities = sink_reachability_analysis_with_progress(
///     &graph, &sources, &sinks, &progress
/// )?;
/// // Output: Sink reachability: 1/10 sinks analyzed, 1 vulnerabilities found
/// ```
pub fn sink_reachability_analysis_with_progress<F>(
    graph: &SqliteGraph,
    sources: &[i64],
    sinks: &[i64],
    progress: &F,
) -> Result<AHashMap<i64, Vec<i64>>, SqliteGraphError>
where
    F: ProgressCallback,
{
    let mut result: AHashMap<i64, Vec<i64>> = AHashMap::new();
    let total = sinks.len();

    for (idx, &sink) in sinks.iter().enumerate() {
        let taint_result = propagate_taint_backward(graph, sink, sources)?;

        // Only include sinks that have affecting sources (vulnerabilities)
        if !taint_result.sources.is_empty() {
            let affecting_sources: Vec<i64> = taint_result.sources.iter().copied().collect();
            result.insert(sink, affecting_sources);
        }

        // Report progress
        progress.on_progress(
            idx + 1,
            Some(total),
            &format!(
                "Sink reachability: {}/{} sinks analyzed, {} vulnerabilities found",
                idx + 1,
                total,
                result.len()
            ),
        );
    }

    // Report completion
    progress.on_complete();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::graph::types::GraphEntity;

    // Test helper: create a simple entity with metadata
    fn create_test_entity(id: i64, kind: &str, data: serde_json::Value) -> GraphEntity {
        GraphEntity {
            id,
            kind: kind.to_string(),
            name: format!("node_{}", id),
            file_path: None,
            data,
        }
    }

    #[test]
    fn test_metadata_source_detector_kind_source() {
        let detector = MetadataSourceDetector;
        let entity = create_test_entity(1, "variable", json!({"kind": "source"}));

        assert!(detector.is_source(1, &entity));
    }

    #[test]
    fn test_metadata_source_detector_kind_untrusted() {
        let detector = MetadataSourceDetector;
        let entity = create_test_entity(1, "variable", json!({"kind": "untrusted"}));

        assert!(detector.is_source(1, &entity));
    }

    #[test]
    fn test_metadata_source_detector_kind_user_input() {
        let detector = MetadataSourceDetector;
        let entity = create_test_entity(1, "variable", json!({"kind": "user_input"}));

        assert!(detector.is_source(1, &entity));
    }

    #[test]
    fn test_metadata_source_detector_taint_field() {
        let detector = MetadataSourceDetector;
        let entity = create_test_entity(1, "variable", json!({"taint": "source"}));

        assert!(detector.is_source(1, &entity));
    }

    #[test]
    fn test_metadata_source_detector_not_a_source() {
        let detector = MetadataSourceDetector;
        let entity = create_test_entity(1, "variable", json!({"kind": "sanitized"}));

        assert!(!detector.is_source(1, &entity));
    }

    #[test]
    fn test_metadata_sink_detector_kind_sink() {
        let detector = MetadataSinkDetector;
        let entity = create_test_entity(1, "operation", json!({"kind": "sink"}));

        assert!(detector.is_sink(1, &entity));
    }

    #[test]
    fn test_metadata_sink_detector_kind_sql_query() {
        let detector = MetadataSinkDetector;
        let entity = create_test_entity(1, "operation", json!({"kind": "sql_query"}));

        assert!(detector.is_sink(1, &entity));
    }

    #[test]
    fn test_metadata_sink_detector_kind_html_output() {
        let detector = MetadataSinkDetector;
        let entity = create_test_entity(1, "operation", json!({"kind": "html_output"}));

        assert!(detector.is_sink(1, &entity));
    }

    #[test]
    fn test_metadata_sink_detector_kind_command() {
        let detector = MetadataSinkDetector;
        let entity = create_test_entity(1, "operation", json!({"kind": "command"}));

        assert!(detector.is_sink(1, &entity));
    }

    #[test]
    fn test_metadata_sink_detector_operation_execute() {
        let detector = MetadataSinkDetector;
        let entity = create_test_entity(1, "operation", json!({"operation": "execute"}));

        assert!(detector.is_sink(1, &entity));
    }

    #[test]
    fn test_metadata_sink_detector_operation_query() {
        let detector = MetadataSinkDetector;
        let entity = create_test_entity(1, "operation", json!({"operation": "query"}));

        assert!(detector.is_sink(1, &entity));
    }

    #[test]
    fn test_metadata_sink_detector_operation_render() {
        let detector = MetadataSinkDetector;
        let entity = create_test_entity(1, "operation", json!({"operation": "render"}));

        assert!(detector.is_sink(1, &entity));
    }

    #[test]
    fn test_metadata_sink_detector_not_a_sink() {
        let detector = MetadataSinkDetector;
        let entity = create_test_entity(1, "operation", json!({"operation": "validate"}));

        assert!(!detector.is_sink(1, &entity));
    }

    #[test]
    fn test_taint_result_new() {
        let result = TaintResult::new();

        assert!(result.sources.is_empty());
        assert!(result.sinks_reached.is_empty());
        assert!(result.tainted_nodes.is_empty());
        assert!(result.source_sink_paths.is_empty());
        assert_eq!(result.size, 0);
    }

    #[test]
    fn test_taint_result_default() {
        let result = TaintResult::default();

        assert!(result.sources.is_empty());
        assert!(result.sinks_reached.is_empty());
        assert!(result.tainted_nodes.is_empty());
        assert!(result.source_sink_paths.is_empty());
        assert_eq!(result.size, 0);
    }

    #[test]
    fn test_taint_result_is_tainted() {
        let mut result = TaintResult::new();
        result.tainted_nodes.insert(1);
        result.tainted_nodes.insert(5);
        result.tainted_nodes.insert(10);

        assert!(result.is_tainted(1));
        assert!(result.is_tainted(5));
        assert!(result.is_tainted(10));
        assert!(!result.is_tainted(99));
    }

    #[test]
    fn test_taint_result_has_vulnerability() {
        let mut result = TaintResult::new();

        // No vulnerability initially
        assert!(!result.has_vulnerability());

        // Add a sink
        result.sinks_reached.insert(5);
        assert!(result.has_vulnerability());
    }

    #[test]
    fn test_taint_result_sorted_tainted_nodes() {
        let mut result = TaintResult::new();
        result.tainted_nodes.insert(10);
        result.tainted_nodes.insert(1);
        result.tainted_nodes.insert(5);
        result.tainted_nodes.insert(3);

        let sorted = result.sorted_tainted_nodes();
        assert_eq!(sorted, vec![1, 3, 5, 10]);
    }

    #[test]
    fn test_taint_result_sorted_vulnerabilities() {
        let mut result = TaintResult::new();
        result.source_sink_paths = vec![(5, 10), (1, 3), (3, 5), (1, 10)];

        let sorted = result.sorted_vulnerabilities();
        assert_eq!(sorted, vec![(1, 3), (1, 10), (3, 5), (5, 10)]);
    }
}
