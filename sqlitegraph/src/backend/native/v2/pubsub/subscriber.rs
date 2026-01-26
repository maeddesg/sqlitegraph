//! Subscriber data structures for in-process pub/sub system
//!
//! This module defines the types for managing subscriptions to pub/sub events.
//!
//! # Subscription Filtering
//!
//! Subscribers can filter events by:
//! - Event type (Node, Edge, KV, Commit)
//! - Specific entity IDs (node_ids, edge_ids, key_hashes)
//!
//! Filters are inclusive - an event is delivered if it matches ANY of the specified criteria.

use crate::backend::native::v2::pubsub::event::{PubSubEvent, PubSubEventType};
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique subscriber identifier
///
/// Each subscriber gets a unique ID that is used to manage the subscription lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriberId(u64);

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
}

impl SubscriptionFilter {
    /// Create a filter that matches all events
    pub fn all() -> Self {
        Self {
            event_types: None,
            node_ids: None,
            edge_ids: None,
            key_hashes: None,
        }
    }

    /// Create a filter that matches specific node IDs
    pub fn nodes(ids: Vec<i64>) -> Self {
        Self {
            event_types: Some(vec![PubSubEventType::Node]),
            node_ids: Some(ids),
            edge_ids: None,
            key_hashes: None,
        }
    }

    /// Create a filter that matches specific edge IDs
    pub fn edges(ids: Vec<i64>) -> Self {
        Self {
            event_types: Some(vec![PubSubEventType::Edge]),
            node_ids: None,
            edge_ids: Some(ids),
            key_hashes: None,
        }
    }

    /// Create a filter that matches specific key hashes
    pub fn keys(hashes: Vec<u64>) -> Self {
        Self {
            event_types: Some(vec![PubSubEventType::KV]),
            node_ids: None,
            edge_ids: None,
            key_hashes: Some(hashes),
        }
    }

    /// Create a filter that matches specific event types
    pub fn event_types(types: Vec<PubSubEventType>) -> Self {
        Self {
            event_types: Some(types),
            node_ids: None,
            edge_ids: None,
            key_hashes: None,
        }
    }

    /// Check if an event matches this filter
    ///
    /// Returns true if the event should be delivered to a subscriber with this filter.
    pub fn matches(&self, event: &PubSubEvent) -> bool {
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
                if let Some(ref ids) = self.node_ids {
                    ids.contains(node_id)
                } else {
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
    pub fn accepts(&self, event: &PubSubEvent) -> bool {
        self.filter.matches(event)
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
        assert!(filter.matches(&node_event));

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 2,
            snapshot_id: 100,
        };
        assert!(filter.matches(&edge_event));

        let kv_event = PubSubEvent::KVChanged {
            key_hash: 999,
            snapshot_id: 100,
        };
        assert!(filter.matches(&kv_event));

        let commit_event = PubSubEvent::SnapshotCommitted { snapshot_id: 100 };
        assert!(filter.matches(&commit_event));
    }

    #[test]
    fn test_filter_nodes_only() {
        let filter = SubscriptionFilter::nodes(vec![1, 2, 3]);

        let node_event_1 = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };
        assert!(filter.matches(&node_event_1));

        let node_event_2 = PubSubEvent::NodeChanged {
            node_id: 2,
            snapshot_id: 100,
        };
        assert!(filter.matches(&node_event_2));

        let node_event_4 = PubSubEvent::NodeChanged {
            node_id: 4,
            snapshot_id: 100,
        };
        assert!(!filter.matches(&node_event_4));

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 999,
            snapshot_id: 100,
        };
        assert!(!filter.matches(&edge_event));

        let kv_event = PubSubEvent::KVChanged {
            key_hash: 999,
            snapshot_id: 100,
        };
        assert!(!filter.matches(&kv_event));
    }

    #[test]
    fn test_filter_edges_only() {
        let filter = SubscriptionFilter::edges(vec![10, 20, 30]);

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 10,
            snapshot_id: 100,
        };
        assert!(filter.matches(&edge_event));

        let edge_event_wrong = PubSubEvent::EdgeChanged {
            edge_id: 99,
            snapshot_id: 100,
        };
        assert!(!filter.matches(&edge_event_wrong));

        let node_event = PubSubEvent::NodeChanged {
            node_id: 10,
            snapshot_id: 100,
        };
        // Different event type, even though ID is in list
        assert!(!filter.matches(&node_event));
    }

    #[test]
    fn test_filter_key_hashes() {
        let filter = SubscriptionFilter::keys(vec![100, 200, 300]);

        let kv_event = PubSubEvent::KVChanged {
            key_hash: 100,
            snapshot_id: 100,
        };
        assert!(filter.matches(&kv_event));

        let kv_event_wrong = PubSubEvent::KVChanged {
            key_hash: 999,
            snapshot_id: 100,
        };
        assert!(!filter.matches(&kv_event_wrong));

        let node_event = PubSubEvent::NodeChanged {
            node_id: 100,
            snapshot_id: 100,
        };
        assert!(!filter.matches(&node_event));
    }

    #[test]
    fn test_filter_event_types() {
        let filter =
            SubscriptionFilter::event_types(vec![PubSubEventType::Node, PubSubEventType::Edge]);

        let node_event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };
        assert!(filter.matches(&node_event));

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 2,
            snapshot_id: 100,
        };
        assert!(filter.matches(&edge_event));

        let kv_event = PubSubEvent::KVChanged {
            key_hash: 999,
            snapshot_id: 100,
        };
        assert!(!filter.matches(&kv_event));

        let commit_event = PubSubEvent::SnapshotCommitted { snapshot_id: 100 };
        assert!(!filter.matches(&commit_event));
    }

    #[test]
    fn test_filter_event_types_all_wildcard() {
        let filter = SubscriptionFilter::event_types(vec![PubSubEventType::All]);

        let node_event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };
        assert!(filter.matches(&node_event));

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 2,
            snapshot_id: 100,
        };
        assert!(filter.matches(&edge_event));

        let kv_event = PubSubEvent::KVChanged {
            key_hash: 999,
            snapshot_id: 100,
        };
        assert!(filter.matches(&kv_event));

        let commit_event = PubSubEvent::SnapshotCommitted { snapshot_id: 100 };
        assert!(filter.matches(&commit_event));
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
        assert!(filter.matches(&node_event_42));

        let node_event_43 = PubSubEvent::NodeChanged {
            node_id: 43,
            snapshot_id: 100,
        };
        assert!(!filter.matches(&node_event_43));

        // Filter only matches NodeChanged events, not other types
        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 42,
            snapshot_id: 100,
        };
        assert!(!filter.matches(&edge_event));
    }
}
