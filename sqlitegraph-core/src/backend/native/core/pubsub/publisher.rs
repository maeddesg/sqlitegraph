//! Publisher for in-process pub/sub system
//!
//! This module implements the event delivery mechanism using Rust channels.
//!
//! # Architecture
//!
//! - **Channel-based:** Uses `std::sync::mpsc` for in-process delivery
//! - **Multiple subscribers:** Each subscriber gets their own channel
//! - **Synchronous emit:** `emit()` is synchronous on the commit path (no background threads)
//! - **Best-effort delivery:** Events are dropped if channel is full or receiver is gone
//!
//! # Thread Safety
//!
//! The `Publisher` uses `Arc<Mutex<>>` to allow thread-safe access to the subscriber list.
//! This means multiple threads can subscribe/unsubscribe concurrently.

use crate::backend::native::v2::pubsub::{
    NodeMetadata, PubSubEvent, SubscriberId, SubscriptionFilter,
};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Publishes events to subscribers via channels
///
/// The Publisher maintains a list of subscribers with their channels and filters.
/// When an event is emitted, it is delivered to all subscribers whose filters match.
///
/// # Best-Effort Delivery
///
/// This is a **best-effort** system:
/// - If a channel is full, the event is dropped
/// - If a receiver has been dropped, the event is dropped
/// - No blocking, no retries, no guarantees
///
/// This design ensures that slow or dead subscribers cannot block the commit path.
///
/// # Thread Safety
///
/// Publisher can be safely shared across threads via `Arc<Mutex<Publisher>>` or
/// by cloning the `Arc` wrapping the internal state.
#[derive(Debug)]
pub struct Publisher {
    /// Channel senders for each subscriber
    ///
    /// Each tuple contains:
    /// - SubscriberId: Unique identifier for the subscriber
    /// - Sender: Channel end for sending events to this subscriber
    /// - SubscriptionFilter: Filter for which events this subscriber receives
    senders: Arc<Mutex<Vec<(SubscriberId, Sender<PubSubEvent>, SubscriptionFilter)>>>,
    /// Next subscriber ID
    ///
    /// Used to generate unique subscriber IDs. We don't use SubscriberId::new()
    /// here because we need to track IDs within the publisher context.
    next_id: Arc<Mutex<u64>>,
}

impl Publisher {
    /// Create a new publisher
    ///
    /// # Example
    ///
    /// ```rust
    /// use sqlitegraph::backend::native::v2::pubsub::Publisher;
    ///
    /// let publisher = Publisher::new();
    /// assert_eq!(publisher.subscriber_count(), 0);
    /// ```
    pub fn new() -> Self {
        Self {
            senders: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(Mutex::new(1)),
        }
    }

    /// Subscribe to events with a filter
    ///
    /// Creates a new channel for this subscriber and returns the receiver end.
    /// The subscriber will receive only events that match the provided filter.
    ///
    /// # Arguments
    ///
    /// * `filter` - Filter for which events this subscriber receives
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - `SubscriberId`: Unique identifier for this subscription
    /// - `Receiver<PubSubEvent>`: Channel end for receiving events
    ///
    /// # Example
    ///
    /// ```rust
    /// use sqlitegraph::backend::native::v2::pubsub::{Publisher, SubscriptionFilter};
    ///
    /// let publisher = Publisher::new();
    /// let filter = SubscriptionFilter::all();
    /// let (_id, _rx) = publisher.subscribe(filter);
    /// ```
    pub fn subscribe(&self, filter: SubscriptionFilter) -> (SubscriberId, Receiver<PubSubEvent>) {
        let (tx, rx) = mpsc::channel();
        let id = {
            let mut next = self.next_id.lock().unwrap();
            let id = SubscriberId::from_raw(*next);
            *next += 1;
            id
        };
        let mut senders = self.senders.lock().unwrap();
        senders.push((id, tx, filter));
        (id, rx)
    }

    /// Unsubscribe a subscriber
    ///
    /// Removes the subscriber's channel from the publisher.
    /// Any events in the subscriber's channel are lost when the channel is dropped.
    ///
    /// # Arguments
    ///
    /// * `id` - The subscriber ID to unsubscribe
    ///
    /// # Returns
    ///
    /// `true` if the subscriber existed and was removed, `false` if not found.
    ///
    /// # Example
    ///
    /// ```rust
    /// use sqlitegraph::backend::native::v2::pubsub::{Publisher, SubscriptionFilter};
    ///
    /// let publisher = Publisher::new();
    /// let filter = SubscriptionFilter::all();
    /// let (id, _rx) = publisher.subscribe(filter);
    ///
    /// assert!(publisher.unsubscribe(id));
    /// assert!(!publisher.unsubscribe(id)); // Already unsubscribed
    /// ```
    pub fn unsubscribe(&self, id: SubscriberId) -> bool {
        let mut senders = self.senders.lock().unwrap();
        let original_len = senders.len();
        senders.retain(|(sub_id, _, _)| *sub_id != id);
        senders.len() < original_len
    }

    /// Emit an event to all matching subscribers
    ///
    /// Iterates through all subscribers and sends the event to those whose
    /// filters match. The event is cloned for each subscriber.
    ///
    /// # Best-Effort Delivery
    ///
    /// - If a channel is full, the send fails silently
    /// - If a receiver has been dropped, the send fails silently
    /// - No blocking, no retries
    ///
    /// This ensures that a slow or dead subscriber cannot block the commit path.
    ///
    /// # Pattern-Based Subscriptions
    ///
    /// This method uses simple matching (ID-based only). For pattern-based
    /// subscriptions (kind_patterns, name_patterns), use `emit_with_metadata()`
    /// instead to provide node metadata for pattern matching.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to emit
    ///
    /// # Example
    ///
    /// ```rust
    /// use sqlitegraph::backend::native::v2::pubsub::{Publisher, PubSubEvent, SubscriptionFilter};
    ///
    /// let publisher = Publisher::new();
    /// let filter = SubscriptionFilter::all();
    /// let (_id, rx) = publisher.subscribe(filter);
    ///
    /// publisher.emit(PubSubEvent::SnapshotCommitted { snapshot_id: 100 });
    /// ```
    pub fn emit(&self, event: PubSubEvent) {
        let senders = self.senders.lock().unwrap();
        for (_, sender, filter) in senders.iter() {
            // Check if event matches filter (simple matching, no pattern support)
            if filter.matches_simple(&event) {
                // Send, ignore errors (channel full/closed = best-effort)
                let _ = sender.send(event.clone());
            }
        }
    }

    /// Emit an event to all matching subscribers (with pattern support)
    ///
    /// This method supports pattern-based subscriptions by accepting node metadata
    /// for NodeChanged events. For other event types, metadata is ignored.
    ///
    /// # Pattern-Based Subscriptions
    ///
    /// When any subscriber has `kind_patterns` or `name_patterns` filters, the
    /// caller must provide node metadata for NodeChanged events. If metadata is
    /// not provided, pattern-based subscribers will not receive the event.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to emit
    /// * `node_metadata` - Optional node metadata for pattern matching (only used for NodeChanged events)
    ///
    /// # Example
    ///
    /// ```rust
    /// use sqlitegraph::backend::native::v2::pubsub::{Publisher, PubSubEvent, SubscriptionFilter, NodeMetadata};
    ///
    /// let publisher = Publisher::new();
    /// let filter = SubscriptionFilter::kind_patterns(vec!["agent:*".to_string()]);
    /// let (_id, rx) = publisher.subscribe(filter);
    ///
    /// let metadata = NodeMetadata::new("agent:worker".to_string(), "agent-123".to_string());
    /// publisher.emit_with_metadata(
    ///     PubSubEvent::NodeChanged { node_id: 1, snapshot_id: 100 },
    ///     Some(&metadata)
    /// );
    /// ```
    pub fn emit_with_metadata(&self, event: PubSubEvent, node_metadata: Option<&NodeMetadata>) {
        let senders = self.senders.lock().unwrap();
        for (_, sender, filter) in senders.iter() {
            // Check if event matches filter (with pattern support)
            if filter.matches(&event, node_metadata) {
                // Send, ignore errors (channel full/closed = best-effort)
                let _ = sender.send(event.clone());
            }
        }
    }

    /// Get current subscriber count
    ///
    /// Returns the number of active subscribers.
    ///
    /// # Example
    ///
    /// ```rust
    /// use sqlitegraph::backend::native::v2::pubsub::{Publisher, SubscriptionFilter};
    ///
    /// let publisher = Publisher::new();
    /// assert_eq!(publisher.subscriber_count(), 0);
    ///
    /// let (_id1, _rx1) = publisher.subscribe(SubscriptionFilter::all());
    /// assert_eq!(publisher.subscriber_count(), 1);
    ///
    /// let (_id2, _rx2) = publisher.subscribe(SubscriptionFilter::all());
    /// assert_eq!(publisher.subscriber_count(), 2);
    /// ```
    pub fn subscriber_count(&self) -> usize {
        self.senders.lock().unwrap().len()
    }
}

impl Default for Publisher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::native::v2::pubsub::{PubSubEvent, PubSubEventType, SubscriptionFilter};

    #[test]
    fn test_publisher_creation() {
        let pubber = Publisher::new();
        assert_eq!(pubber.subscriber_count(), 0);
    }

    #[test]
    fn test_default_publisher() {
        let pubber = Publisher::default();
        assert_eq!(pubber.subscriber_count(), 0);
    }

    #[test]
    fn test_subscribe_unsubscribe() {
        let pubber = Publisher::new();
        let filter = SubscriptionFilter::all();

        // Subscribe
        let (id, _rx) = pubber.subscribe(filter.clone());
        assert_eq!(pubber.subscriber_count(), 1);

        // Unsubscribe
        assert!(pubber.unsubscribe(id));
        assert_eq!(pubber.subscriber_count(), 0);

        // Unsubscribe non-existent
        assert!(!pubber.unsubscribe(id));
    }

    #[test]
    fn test_multiple_subscribers() {
        let pubber = Publisher::new();
        let filter = SubscriptionFilter::all();

        let (_id1, _rx1) = pubber.subscribe(filter.clone());
        assert_eq!(pubber.subscriber_count(), 1);

        let (_id2, _rx2) = pubber.subscribe(filter.clone());
        assert_eq!(pubber.subscriber_count(), 2);

        let (_id3, _rx3) = pubber.subscribe(filter);
        assert_eq!(pubber.subscriber_count(), 3);
    }

    #[test]
    fn test_emit_to_single_subscriber() {
        let pubber = Publisher::new();
        let filter = SubscriptionFilter::all();
        let (_id, rx) = pubber.subscribe(filter);

        pubber.emit(PubSubEvent::SnapshotCommitted { snapshot_id: 100 });

        let received = rx.recv().unwrap();
        assert_eq!(received.snapshot_id(), 100);
    }

    #[test]
    fn test_emit_to_multiple_subscribers() {
        let pubber = Publisher::new();
        let filter = SubscriptionFilter::all();

        let (_id1, rx1) = pubber.subscribe(filter.clone());
        let (_id2, rx2) = pubber.subscribe(filter);

        pubber.emit(PubSubEvent::SnapshotCommitted { snapshot_id: 200 });

        assert_eq!(rx1.recv().unwrap().snapshot_id(), 200);
        assert_eq!(rx2.recv().unwrap().snapshot_id(), 200);
    }

    #[test]
    fn test_filter_by_event_type() {
        let pubber = Publisher::new();
        let node_filter = SubscriptionFilter::event_types(vec![PubSubEventType::Node]);
        let (_id, rx) = pubber.subscribe(node_filter);

        pubber.emit(PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        });
        pubber.emit(PubSubEvent::EdgeChanged {
            edge_id: 1,
            snapshot_id: 100,
        });

        // Should receive NodeChanged but not EdgeChanged
        let received = rx.recv().unwrap();
        assert!(received.is_node_event());

        // Channel should be empty (EdgeChanged was filtered)
        let timeout = std::time::Duration::from_millis(100);
        let result = rx.recv_timeout(timeout);
        assert!(result.is_err()); // Timeout = no more events
    }

    #[test]
    fn test_filter_by_specific_node() {
        let pubber = Publisher::new();
        let filter = SubscriptionFilter::nodes(vec![42, 43]);
        let (_id, rx) = pubber.subscribe(filter);

        pubber.emit(PubSubEvent::NodeChanged {
            node_id: 42,
            snapshot_id: 100,
        });
        pubber.emit(PubSubEvent::NodeChanged {
            node_id: 99,
            snapshot_id: 101,
        });

        // Should receive node 42 but not node 99
        let received = rx.recv().unwrap();
        assert_eq!(received.snapshot_id(), 100);

        // Channel should be empty (node 99 was filtered)
        let timeout = std::time::Duration::from_millis(100);
        let result = rx.recv_timeout(timeout);
        assert!(result.is_err());
    }

    #[test]
    fn test_best_effort_delivery() {
        let pubber = Publisher::new();
        let filter = SubscriptionFilter::all();
        let (_id, rx) = pubber.subscribe(filter);

        // Drop receiver - next send should not block/fail
        drop(rx);

        // This should not panic (best-effort)
        pubber.emit(PubSubEvent::SnapshotCommitted { snapshot_id: 100 });
    }

    #[test]
    fn test_mixed_filters() {
        let pubber = Publisher::new();

        // Subscriber 1: Only node events
        let node_filter = SubscriptionFilter::event_types(vec![PubSubEventType::Node]);
        let (_id1, rx1) = pubber.subscribe(node_filter);

        // Subscriber 2: Only edge events
        let edge_filter = SubscriptionFilter::event_types(vec![PubSubEventType::Edge]);
        let (_id2, rx2) = pubber.subscribe(edge_filter);

        // Subscriber 3: All events
        let all_filter = SubscriptionFilter::all();
        let (_id3, rx3) = pubber.subscribe(all_filter);

        pubber.emit(PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        });
        pubber.emit(PubSubEvent::EdgeChanged {
            edge_id: 1,
            snapshot_id: 100,
        });

        // Subscriber 1 should only receive node event
        assert!(rx1.recv().unwrap().is_node_event());
        assert!(
            rx1.recv_timeout(std::time::Duration::from_millis(100))
                .is_err()
        );

        // Subscriber 2 should only receive edge event
        assert!(rx2.recv().unwrap().is_edge_event());
        assert!(
            rx2.recv_timeout(std::time::Duration::from_millis(100))
                .is_err()
        );

        // Subscriber 3 should receive both events
        assert!(rx3.recv().unwrap().is_node_event());
        assert!(rx3.recv().unwrap().is_edge_event());
    }
}
