//! Subscriber data structures for in-process pub/sub system
//!
//! This module defines the types for managing subscriptions to pub/sub events.
//!
//! # Subscription Filtering
//!
//! Subscribers can filter events by:
//! - Event type (Node, Edge, KV, Commit)
//! - Specific entity IDs (node_ids, edge_ids, key_hashes)
//! - Pattern matching on node kind/name (kind_patterns, name_patterns)
//!
//! Filters are inclusive - an event is delivered if it matches ANY of the specified criteria.
//!
//! # Pattern-Based Subscriptions
//!
//! Pattern filters use glob matching with `*` (wildcard) and `?` (single char):
//! - `kind_patterns: vec!["agent:*"]` matches all nodes with kind starting with "agent:"
//! - `name_patterns: vec!["user-???"]` matches all nodes with names like "user-123"
//!
//! **Important:** Pattern filters require fetching node metadata at publish time, which has
//! a performance cost. Use ID-based filters when possible for better performance.

use crate::backend::native::pattern::glob_matches;
use crate::backend::native::v2::pubsub::event::{PubSubEvent, PubSubEventType};
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique subscriber identifier
///
/// Each subscriber gets a unique ID that is used to manage the subscription lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriberId(u64);

/// Node metadata for pattern-based subscription matching
///
/// This structure holds the kind and name of a node, which are used
/// for pattern-based filtering in subscriptions.
///
/// # Purpose
///
/// When a subscription uses `kind_patterns` or `name_patterns`, the publisher
/// needs to fetch node metadata to check if the event matches the pattern.
/// This structure is passed to `SubscriptionFilter::matches()` to enable
/// pattern matching.
///
/// # Example
///
/// ```
/// use sqlitegraph::backend::native::v2::pubsub::subscriber::NodeMetadata;
///
/// let metadata = NodeMetadata {
///     kind: "agent:worker".to_string(),
///     name: "agent-123".to_string(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeMetadata {
    /// The node's kind (e.g., "agent:worker", "user:admin")
    pub kind: String,
    /// The node's name (e.g., "agent-123", "user-admin-1")
    pub name: String,
}

impl NodeMetadata {
    /// Create new node metadata
    pub fn new(kind: String, name: String) -> Self {
        Self { kind, name }
    }
}

impl SubscriberId {
    /// Generate a new unique subscriber ID
    ///
    /// IDs are monotonically increasing across all subscribers.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    /// Get the raw ID value
    pub fn value(&self) -> u64 {
        self.0
    }

    /// Create a SubscriberId from a raw value
    ///
    /// This is used internally by the Publisher to generate sequential IDs.
    pub fn from_raw(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value (alias for value())
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Filter for which events a subscriber receives
///
/// Filters are inclusive - an event matches if it matches ANY of the specified criteria.
/// All None fields means "match all events of this type".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionFilter {
    /// Event types to receive (None = all types)
    pub event_types: Option<Vec<PubSubEventType>>,
    /// Specific node IDs to watch (None = all nodes)
    pub node_ids: Option<Vec<i64>>,
    /// Specific edge IDs to watch (None = all edges)
    pub edge_ids: Option<Vec<i64>>,
    /// Specific key hashes to watch (None = all keys)
    pub key_hashes: Option<Vec<u64>>,
    /// Glob patterns for node kind matching (None = no pattern filtering)
    ///
    /// Uses glob matching with `*` (wildcard) and `?` (single char).
    /// Example: `vec!["agent:*", "user:*"]` matches all agent and user nodes.
    pub kind_patterns: Option<Vec<String>>,
    /// Glob patterns for node name matching (None = no pattern filtering)
    ///
    /// Uses glob matching with `*` (wildcard) and `?` (single char).
    /// Example: `vec!["node-*", "entity-???"]` matches various node naming patterns.
    pub name_patterns: Option<Vec<String>>,
}

impl SubscriptionFilter {
    /// Create a filter that matches all events
    pub fn all() -> Self {
        Self {
            event_types: None,
            node_ids: None,
            edge_ids: None,
            key_hashes: None,
            kind_patterns: None,
            name_patterns: None,
        }
    }

    /// Create a filter that matches specific node IDs
    pub fn nodes(ids: Vec<i64>) -> Self {
        Self {
            event_types: Some(vec![PubSubEventType::Node]),
            node_ids: Some(ids),
            edge_ids: None,
            key_hashes: None,
            kind_patterns: None,
            name_patterns: None,
        }
    }

    /// Create a filter that matches specific edge IDs
    pub fn edges(ids: Vec<i64>) -> Self {
        Self {
            event_types: Some(vec![PubSubEventType::Edge]),
            node_ids: None,
            edge_ids: Some(ids),
            key_hashes: None,
            kind_patterns: None,
            name_patterns: None,
        }
    }

    /// Create a filter that matches specific key hashes
    pub fn keys(hashes: Vec<u64>) -> Self {
        Self {
            event_types: Some(vec![PubSubEventType::KV]),
            node_ids: None,
            edge_ids: None,
            key_hashes: Some(hashes),
            kind_patterns: None,
            name_patterns: None,
        }
    }

    /// Create a filter that matches specific event types
    pub fn event_types(types: Vec<PubSubEventType>) -> Self {
        Self {
            event_types: Some(types),
            node_ids: None,
            edge_ids: None,
            key_hashes: None,
            kind_patterns: None,
            name_patterns: None,
        }
    }

    /// Create a filter that matches nodes by kind pattern
    ///
    /// Uses glob matching with `*` (wildcard) and `?` (single char).
    ///
    /// # Arguments
    ///
    /// * `patterns` - Vector of glob patterns to match against node kind
    ///
    /// # Example
    ///
    /// ```
    /// use sqlitegraph::backend::native::v2::pubsub::SubscriptionFilter;
    ///
    /// // Match all agent nodes
    /// let filter = SubscriptionFilter::kind_patterns(vec!["agent:*".to_string()]);
    ///
    /// // Match multiple kind patterns
    /// let filter = SubscriptionFilter::kind_patterns(vec![
    ///     "agent:*".to_string(),
    ///     "user:*".to_string(),
    /// ]);
    /// ```
    pub fn kind_patterns(patterns: Vec<String>) -> Self {
        Self {
            event_types: Some(vec![PubSubEventType::Node]),
            node_ids: None,
            edge_ids: None,
            key_hashes: None,
            kind_patterns: Some(patterns),
            name_patterns: None,
        }
    }

    /// Create a filter that matches nodes by name pattern
    ///
    /// Uses glob matching with `*` (wildcard) and `?` (single char).
    ///
    /// # Arguments
    ///
    /// * `patterns` - Vector of glob patterns to match against node name
    ///
    /// # Example
    ///
    /// ```
    /// use sqlitegraph::backend::native::v2::pubsub::SubscriptionFilter;
    ///
    /// // Match all nodes with names like "node-123"
    /// let filter = SubscriptionFilter::name_patterns(vec!["node-*".to_string()]);
    ///
    /// // Match nodes with exactly 3-character IDs
    /// let filter = SubscriptionFilter::name_patterns(vec!["entity-???".to_string()]);
    /// ```
    pub fn name_patterns(patterns: Vec<String>) -> Self {
        Self {
            event_types: Some(vec![PubSubEventType::Node]),
            node_ids: None,
            edge_ids: None,
            key_hashes: None,
            kind_patterns: None,
            name_patterns: Some(patterns),
        }
    }

    /// Create a filter that matches nodes by both kind and name patterns
    ///
    /// A node must match at least one kind pattern AND at least one name pattern.
    ///
    /// # Arguments
    ///
    /// * `kind_patterns` - Vector of glob patterns for node kind
    /// * `name_patterns` - Vector of glob patterns for node name
    ///
    /// # Example
    ///
    /// ```
    /// use sqlitegraph::backend::native::v2::pubsub::SubscriptionFilter;
    ///
    /// // Match agent nodes with names like "agent-123"
    /// let filter = SubscriptionFilter::node_patterns(
    ///     vec!["agent:*".to_string()],
    ///     vec!["agent-*".to_string()],
    /// );
    /// ```
    pub fn node_patterns(kind_patterns: Vec<String>, name_patterns: Vec<String>) -> Self {
        Self {
            event_types: Some(vec![PubSubEventType::Node]),
            node_ids: None,
            edge_ids: None,
            key_hashes: None,
            kind_patterns: Some(kind_patterns),
            name_patterns: Some(name_patterns),
        }
    }

    /// Check if this filter has pattern-based criteria
    ///
    /// Returns true if kind_patterns or name_patterns is set.
    /// This is used to determine if node metadata needs to be fetched for matching.
    pub fn has_patterns(&self) -> bool {
        self.kind_patterns.is_some() || self.name_patterns.is_some()
    }

    /// Check if an event matches this filter
    ///
    /// Returns true if the event should be delivered to a subscriber with this filter.
    ///
    /// # Pattern Matching
    ///
    /// For events with pattern filters (kind_patterns, name_patterns), the caller
    /// must provide node metadata via the `node_metadata` parameter. If the filter
    /// has patterns but no metadata is provided, the event will not match.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to check
    /// * `node_metadata` - Optional node metadata for pattern matching
    pub fn matches(&self, event: &PubSubEvent, node_metadata: Option<&NodeMetadata>) -> bool {
        // Check event type filter
        if let Some(ref types) = self.event_types {
            let event_type = event.event_type();
            if !types.iter().any(|t| matches_type(t, event_type)) {
                return false;
            }
        }

        // Check entity-specific filters
        match event {
            PubSubEvent::NodeChanged { node_id, .. } => {
                // Check node ID filter
                let id_match = if let Some(ref ids) = self.node_ids {
                    ids.contains(node_id)
                } else {
                    true
                };

                if !id_match {
                    return false;
                }

                // Check pattern filters (require metadata)
                if self.has_patterns() {
                    if let Some(metadata) = node_metadata {
                        // Check kind patterns
                        if let Some(ref kind_patterns) = self.kind_patterns {
                            let kind_match = kind_patterns
                                .iter()
                                .any(|pattern| glob_matches(pattern, &metadata.kind));
                            if !kind_match {
                                return false;
                            }
                        }

                        // Check name patterns
                        if let Some(ref name_patterns) = self.name_patterns {
                            let name_match = name_patterns
                                .iter()
                                .any(|pattern| glob_matches(pattern, &metadata.name));
                            if !name_match {
                                return false;
                            }
                        }

                        // All pattern checks passed
                        true
                    } else {
                        // Filter has patterns but no metadata provided - conservative: don't match
                        false
                    }
                } else {
                    // No pattern filters, ID check already passed
                    true
                }
            }
            PubSubEvent::EdgeChanged { edge_id, .. } => {
                if let Some(ref ids) = self.edge_ids {
                    ids.contains(edge_id)
                } else {
                    true
                }
            }
            PubSubEvent::KVChanged { key_hash, .. } => {
                if let Some(ref hashes) = self.key_hashes {
                    hashes.contains(key_hash)
                } else {
                    true
                }
            }
            PubSubEvent::SnapshotCommitted { .. } => {
                // Commit events are only filtered by event type
                true
            }
        }
    }

    /// Check if an event matches this filter (without pattern metadata)
    ///
    /// This is a convenience method for filters that don't use pattern matching.
    /// For pattern-based filters, use `matches()` with node metadata.
    pub fn matches_simple(&self, event: &PubSubEvent) -> bool {
        self.matches(event, None)
    }
}

/// Helper to match event types with the "All" wildcard
fn matches_type(filter_type: &PubSubEventType, event_type: PubSubEventType) -> bool {
    matches!(filter_type, PubSubEventType::All) || *filter_type == event_type
}

/// Represents a subscription to pub/sub events
///
/// Each subscriber has a unique ID and a filter that determines which events they receive.
#[derive(Debug)]
pub struct Subscriber {
    id: SubscriberId,
    filter: SubscriptionFilter,
    // Channel sender will be added in the next plan when we implement event delivery
}

impl Subscriber {
    /// Create a new subscriber with the given filter
    pub fn new(filter: SubscriptionFilter) -> Self {
        Self {
            id: SubscriberId::new(),
            filter,
        }
    }

    /// Get the subscriber's unique ID
    pub fn id(&self) -> SubscriberId {
        self.id
    }

    /// Get the subscriber's event filter
    pub fn filter(&self) -> &SubscriptionFilter {
        &self.filter
    }

    /// Check if an event should be delivered to this subscriber
    ///
    /// This is a simplified check that doesn't support pattern matching.
    /// For pattern-based subscriptions, use `accepts_with_metadata()` instead.
    pub fn accepts(&self, event: &PubSubEvent) -> bool {
        self.filter.matches_simple(event)
    }

    /// Check if an event should be delivered to this subscriber (with pattern support)
    ///
    /// For pattern-based subscriptions, provide node metadata to enable matching.
    pub fn accepts_with_metadata(&self, event: &PubSubEvent, metadata: Option<&NodeMetadata>) -> bool {
        self.filter.matches(event, metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscriber_id_unique() {
        let id1 = SubscriberId::new();
        let id2 = SubscriberId::new();
        let id3 = SubscriberId::new();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);

        // IDs should be monotonically increasing
        assert!(id2.value() > id1.value());
        assert!(id3.value() > id2.value());
    }

    #[test]
    fn test_filter_all_matches_all() {
        let filter = SubscriptionFilter::all();

        let node_event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&node_event));

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 2,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&edge_event));

        let kv_event = PubSubEvent::KVChanged {
            key_hash: 999,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&kv_event));

        let commit_event = PubSubEvent::SnapshotCommitted { snapshot_id: 100 };
        assert!(filter.matches_simple(&commit_event));
    }

    #[test]
    fn test_filter_nodes_only() {
        let filter = SubscriptionFilter::nodes(vec![1, 2, 3]);

        let node_event_1 = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&node_event_1));

        let node_event_2 = PubSubEvent::NodeChanged {
            node_id: 2,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&node_event_2));

        let node_event_4 = PubSubEvent::NodeChanged {
            node_id: 4,
            snapshot_id: 100,
        };
        assert!(!filter.matches_simple(&node_event_4));

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 999,
            snapshot_id: 100,
        };
        assert!(!filter.matches_simple(&edge_event));

        let kv_event = PubSubEvent::KVChanged {
            key_hash: 999,
            snapshot_id: 100,
        };
        assert!(!filter.matches_simple(&kv_event));
    }

    #[test]
    fn test_filter_edges_only() {
        let filter = SubscriptionFilter::edges(vec![10, 20, 30]);

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 10,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&edge_event));

        let edge_event_wrong = PubSubEvent::EdgeChanged {
            edge_id: 99,
            snapshot_id: 100,
        };
        assert!(!filter.matches_simple(&edge_event_wrong));

        let node_event = PubSubEvent::NodeChanged {
            node_id: 10,
            snapshot_id: 100,
        };
        // Different event type, even though ID is in list
        assert!(!filter.matches_simple(&node_event));
    }

    #[test]
    fn test_filter_key_hashes() {
        let filter = SubscriptionFilter::keys(vec![100, 200, 300]);

        let kv_event = PubSubEvent::KVChanged {
            key_hash: 100,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&kv_event));

        let kv_event_wrong = PubSubEvent::KVChanged {
            key_hash: 999,
            snapshot_id: 100,
        };
        assert!(!filter.matches_simple(&kv_event_wrong));

        let node_event = PubSubEvent::NodeChanged {
            node_id: 100,
            snapshot_id: 100,
        };
        assert!(!filter.matches_simple(&node_event));
    }

    #[test]
    fn test_filter_event_types() {
        let filter =
            SubscriptionFilter::event_types(vec![PubSubEventType::Node, PubSubEventType::Edge]);

        let node_event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&node_event));

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 2,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&edge_event));

        let kv_event = PubSubEvent::KVChanged {
            key_hash: 999,
            snapshot_id: 100,
        };
        assert!(!filter.matches_simple(&kv_event));

        let commit_event = PubSubEvent::SnapshotCommitted { snapshot_id: 100 };
        assert!(!filter.matches_simple(&commit_event));
    }

    #[test]
    fn test_filter_event_types_all_wildcard() {
        let filter = SubscriptionFilter::event_types(vec![PubSubEventType::All]);

        let node_event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&node_event));

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 2,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&edge_event));

        let kv_event = PubSubEvent::KVChanged {
            key_hash: 999,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&kv_event));

        let commit_event = PubSubEvent::SnapshotCommitted { snapshot_id: 100 };
        assert!(filter.matches_simple(&commit_event));
    }

    #[test]
    fn test_subscriber_creation() {
        let filter = SubscriptionFilter::nodes(vec![1, 2, 3]);
        let subscriber = Subscriber::new(filter.clone());

        assert_eq!(subscriber.filter(), &filter);

        let node_event = PubSubEvent::NodeChanged {
            node_id: 2,
            snapshot_id: 100,
        };
        assert!(subscriber.accepts(&node_event));

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 999,
            snapshot_id: 100,
        };
        assert!(!subscriber.accepts(&edge_event));
    }

    #[test]
    fn test_multiple_subscribers_unique_ids() {
        let filter1 = SubscriptionFilter::all();
        let filter2 = SubscriptionFilter::all();

        let sub1 = Subscriber::new(filter1);
        let sub2 = Subscriber::new(filter2);

        assert_ne!(sub1.id(), sub2.id());
    }

    #[test]
    fn test_filter_specific_node() {
        let filter = SubscriptionFilter::nodes(vec![42]);

        let node_event_42 = PubSubEvent::NodeChanged {
            node_id: 42,
            snapshot_id: 100,
        };
        assert!(filter.matches_simple(&node_event_42));

        let node_event_43 = PubSubEvent::NodeChanged {
            node_id: 43,
            snapshot_id: 100,
        };
        assert!(!filter.matches_simple(&node_event_43));

        // Filter only matches NodeChanged events, not other types
        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 42,
            snapshot_id: 100,
        };
        assert!(!filter.matches_simple(&edge_event));
    }

    // Pattern-based subscription tests

    #[test]
    fn test_filter_kind_patterns_wildcard() {
        let filter = SubscriptionFilter::kind_patterns(vec!["agent:*".to_string()]);

        let event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };

        // No metadata - should not match
        assert!(!filter.matches(&event, None));

        // With matching metadata
        let metadata = NodeMetadata::new("agent:worker".to_string(), "agent-123".to_string());
        assert!(filter.matches(&event, Some(&metadata)));

        // With non-matching metadata
        let wrong_metadata = NodeMetadata::new("user:admin".to_string(), "user-1".to_string());
        assert!(!filter.matches(&event, Some(&wrong_metadata)));
    }

    #[test]
    fn test_filter_name_patterns_wildcard() {
        let filter = SubscriptionFilter::name_patterns(vec!["node-*".to_string()]);

        let event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };

        // No metadata - should not match
        assert!(!filter.matches(&event, None));

        // With matching metadata
        let metadata = NodeMetadata::new("any:kind".to_string(), "node-123".to_string());
        assert!(filter.matches(&event, Some(&metadata)));

        // With non-matching metadata
        let wrong_metadata = NodeMetadata::new("any:kind".to_string(), "entity-123".to_string());
        assert!(!filter.matches(&event, Some(&wrong_metadata)));
    }

    #[test]
    fn test_filter_kind_patterns_multiple() {
        let filter = SubscriptionFilter::kind_patterns(vec![
            "agent:*".to_string(),
            "user:*".to_string(),
        ]);

        let event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };

        // Should match agent:worker
        let agent_metadata = NodeMetadata::new("agent:worker".to_string(), "agent-1".to_string());
        assert!(filter.matches(&event, Some(&agent_metadata)));

        // Should match user:admin
        let user_metadata = NodeMetadata::new("user:admin".to_string(), "user-1".to_string());
        assert!(filter.matches(&event, Some(&user_metadata)));

        // Should not match system:process
        let system_metadata = NodeMetadata::new("system:process".to_string(), "sys-1".to_string());
        assert!(!filter.matches(&event, Some(&system_metadata)));
    }

    #[test]
    fn test_filter_name_patterns_question_mark() {
        let filter = SubscriptionFilter::name_patterns(vec!["entity-???".to_string()]);

        let event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };

        // Should match entity-123 (3 characters after dash)
        let metadata_match = NodeMetadata::new("any:kind".to_string(), "entity-123".to_string());
        assert!(filter.matches(&event, Some(&metadata_match)));

        // Should not match entity-12 (2 characters)
        let metadata_short = NodeMetadata::new("any:kind".to_string(), "entity-12".to_string());
        assert!(!filter.matches(&event, Some(&metadata_short)));

        // Should not match entity-1234 (4 characters)
        let metadata_long = NodeMetadata::new("any:kind".to_string(), "entity-1234".to_string());
        assert!(!filter.matches(&event, Some(&metadata_long)));
    }

    #[test]
    fn test_filter_node_patterns_both() {
        let filter = SubscriptionFilter::node_patterns(
            vec!["agent:*".to_string()],
            vec!["agent-*".to_string()],
        );

        let event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };

        // Should match both patterns
        let metadata_match = NodeMetadata::new("agent:worker".to_string(), "agent-123".to_string());
        assert!(filter.matches(&event, Some(&metadata_match)));

        // Should not match - wrong kind
        let metadata_wrong_kind = NodeMetadata::new("user:admin".to_string(), "agent-123".to_string());
        assert!(!filter.matches(&event, Some(&metadata_wrong_kind)));

        // Should not match - wrong name
        let metadata_wrong_name = NodeMetadata::new("agent:worker".to_string(), "user-123".to_string());
        assert!(!filter.matches(&event, Some(&metadata_wrong_name)));
    }

    #[test]
    fn test_filter_has_patterns() {
        let filter_no_patterns = SubscriptionFilter::nodes(vec![1, 2, 3]);
        assert!(!filter_no_patterns.has_patterns());

        let filter_kind_patterns = SubscriptionFilter::kind_patterns(vec!["agent:*".to_string()]);
        assert!(filter_kind_patterns.has_patterns());

        let filter_name_patterns = SubscriptionFilter::name_patterns(vec!["node-*".to_string()]);
        assert!(filter_name_patterns.has_patterns());

        let filter_both = SubscriptionFilter::node_patterns(
            vec!["agent:*".to_string()],
            vec!["agent-*".to_string()],
        );
        assert!(filter_both.has_patterns());
    }

    #[test]
    fn test_pattern_filter_no_metadata_conservative() {
        let filter = SubscriptionFilter::kind_patterns(vec!["agent:*".to_string()]);

        let event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };

        // Without metadata, pattern filters should not match (conservative)
        assert!(!filter.matches(&event, None));
    }

    #[test]
    fn test_subscriber_with_pattern_filter() {
        let filter = SubscriptionFilter::kind_patterns(vec!["agent:*".to_string()]);
        let subscriber = Subscriber::new(filter.clone());

        let event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };

        // Simple accepts doesn't support patterns
        assert!(!subscriber.accepts(&event));

        // With metadata, it should match
        let metadata = NodeMetadata::new("agent:worker".to_string(), "agent-1".to_string());
        assert!(subscriber.accepts_with_metadata(&event, Some(&metadata)));
    }

    #[test]
    fn test_pattern_filter_edge_event_no_match() {
        let filter = SubscriptionFilter::kind_patterns(vec!["agent:*".to_string()]);

        let event = PubSubEvent::EdgeChanged {
            edge_id: 1,
            snapshot_id: 100,
        };

        // Pattern filters only apply to NodeChanged events
        let metadata = NodeMetadata::new("agent:worker".to_string(), "agent-1".to_string());
        assert!(!filter.matches(&event, Some(&metadata)));
    }

    #[test]
    fn test_node_metadata_new() {
        let metadata = NodeMetadata::new("agent:worker".to_string(), "agent-123".to_string());
        assert_eq!(metadata.kind, "agent:worker");
        assert_eq!(metadata.name, "agent-123");
    }
}
