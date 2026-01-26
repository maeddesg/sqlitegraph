//! In-process pub/sub for SQLiteGraph
//!
//! Minimal, best-effort pub/sub system with:
//! - ID-only events (no payloads)
//! - Emitted on commit only
//! - In-process channel delivery
//! - No persistence or delivery guarantees
//!
//! # Architecture
//!
//! Events are emitted when transactions commit and carry only identifiers
//! (node_id, edge_id, key_hash, snapshot_id). Consumers must read actual
//! data from the graph/KV APIs using the provided snapshot_id.
//!
//! # Module Organization
//!
//! - [`event`](crate::backend::native::v2::pubsub::event): Event type definitions
//! - [`subscriber`](crate::backend::native::v2::pubsub::subscriber): Subscription management
//! - [`publisher`](crate::backend::native::v2::pubsub::publisher): Channel-based event delivery

pub mod event;
pub mod subscriber;
pub mod publisher;
pub mod emit;

pub use event::{PubSubEvent, PubSubEventType};
pub use subscriber::{Subscriber, SubscriberId, SubscriptionFilter};
pub use publisher::Publisher;
pub use emit::{records_to_events, should_emit_event};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::backend::native::v2::wal::{V2WALConfig, V2WALManager, V2WALRecord};
    use crate::backend::native::v2::pubsub::{SubscriptionFilter, PubSubEvent};
    use std::time::Duration;

    // Note: Full integration tests requiring V2WALManager with graph file setup
    // will be added in Phase 44-05 (Integration Tests) when the complete system
    // is wired together. For now, we test the emit module independently.

    #[test]
    fn test_pubsub_module_exports() {
        // Verify all expected exports are available
        let _ = SubscriptionFilter::all();
        let publisher = Publisher::new();
        assert_eq!(publisher.subscriber_count(), 0);
    }

    #[test]
    fn test_records_to_events_export() {
        // Test that emit module functions are accessible
        use crate::backend::native::v2::pubsub::{records_to_events, should_emit_event};

        let records = vec![V2WALRecord::NodeInsert {
            node_id: 1,
            slot_offset: 0,
            node_data: vec![],
        }];

        let events = records_to_events(&records, 100);
        assert_eq!(events.len(), 2); // NodeChanged + SnapshotCommitted

        assert!(should_emit_event(&records[0]));
    }
}
