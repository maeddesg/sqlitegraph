//! Event types for in-process pub/sub system
//!
//! This module defines the events that are emitted when transactions commit.
//!
//! # ID-Only Design
//!
//! Events carry only identifiers (node_id, edge_id, key_hash, snapshot_id) - not the actual
//! entity data. Consumers must read the actual data from the graph/KV APIs using the
//! provided snapshot_id.
//!
//! This design ensures:
//! - **Minimal overhead:** Events are lightweight (just IDs)
//! - **Snapshot consistency:** Consumers read data from a specific snapshot
//! - **Decoupling:** Event structure doesn't change when entity schemas change
//!
//! # Event Emission
//!
//! Events are emitted ONLY when a transaction commits, not on every write.
//! All events from a single transaction share the same snapshot_id.
//!
//! # Delivery Guarantees
//!
//! This is a **best-effort** system:
//! - No message persistence
//! - No delivery guarantees
//! - No ordering guarantees between subscribers
//! - In-process delivery only (no networking or IPC)

use crate::backend::native::v2::wal::SnapshotId;

/// Event type categories for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PubSubEventType {
    /// Node-related events (NodeChanged)
    Node,
    /// Edge-related events (EdgeChanged)
    Edge,
    /// Key-value events (KVChanged)
    KV,
    /// Transaction commit events (SnapshotCommitted)
    Commit,
    /// All event types (used for subscriptions)
    All,
}

/// Events emitted on transaction commit
///
/// All events carry only identifiers - consumers read actual data
/// from the graph/KV APIs using the provided snapshot_id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PubSubEvent {
    /// A node was created or modified
    NodeChanged {
        /// The node ID that changed
        node_id: i64,
        /// The snapshot containing this change
        snapshot_id: SnapshotId,
    },

    /// An edge was created or modified
    EdgeChanged {
        /// The edge ID that changed (compact representation: node_id + direction + position)
        edge_id: i64,
        /// The snapshot containing this change
        snapshot_id: SnapshotId,
    },

    /// A KV entry was created, modified, or deleted
    KVChanged {
        /// Hash of the key (for privacy and efficiency)
        key_hash: u64,
        /// The snapshot containing this change
        snapshot_id: SnapshotId,
    },

    /// A transaction was committed (contains all changes)
    SnapshotCommitted {
        /// The snapshot ID that was committed
        snapshot_id: SnapshotId,
    },
}

impl PubSubEvent {
    /// Returns the event type category
    pub fn event_type(&self) -> PubSubEventType {
        match self {
            PubSubEvent::NodeChanged { .. } => PubSubEventType::Node,
            PubSubEvent::EdgeChanged { .. } => PubSubEventType::Edge,
            PubSubEvent::KVChanged { .. } => PubSubEventType::KV,
            PubSubEvent::SnapshotCommitted { .. } => PubSubEventType::Commit,
        }
    }

    /// Extracts the snapshot_id from any event
    pub fn snapshot_id(&self) -> SnapshotId {
        match self {
            PubSubEvent::NodeChanged { snapshot_id, .. } => *snapshot_id,
            PubSubEvent::EdgeChanged { snapshot_id, .. } => *snapshot_id,
            PubSubEvent::KVChanged { snapshot_id, .. } => *snapshot_id,
            PubSubEvent::SnapshotCommitted { snapshot_id } => *snapshot_id,
        }
    }

    /// Returns true if this is a NodeChanged event
    pub fn is_node_event(&self) -> bool {
        matches!(self, PubSubEvent::NodeChanged { .. })
    }

    /// Returns true if this is an EdgeChanged event
    pub fn is_edge_event(&self) -> bool {
        matches!(self, PubSubEvent::EdgeChanged { .. })
    }

    /// Returns true if this is a KVChanged event
    pub fn is_kv_event(&self) -> bool {
        matches!(self, PubSubEvent::KVChanged { .. })
    }

    /// Returns true if this is a SnapshotCommitted event
    pub fn is_commit_event(&self) -> bool {
        matches!(self, PubSubEvent::SnapshotCommitted { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_classification() {
        let node_event = PubSubEvent::NodeChanged {
            node_id: 42,
            snapshot_id: 100,
        };
        assert_eq!(node_event.event_type(), PubSubEventType::Node);
        assert!(node_event.is_node_event());
        assert!(!node_event.is_edge_event());
        assert!(!node_event.is_kv_event());
        assert!(!node_event.is_commit_event());

        let edge_event = PubSubEvent::EdgeChanged {
            edge_id: 123,
            snapshot_id: 100,
        };
        assert_eq!(edge_event.event_type(), PubSubEventType::Edge);
        assert!(!edge_event.is_node_event());
        assert!(edge_event.is_edge_event());
        assert!(!edge_event.is_kv_event());
        assert!(!edge_event.is_commit_event());

        let kv_event = PubSubEvent::KVChanged {
            key_hash: 999,
            snapshot_id: 100,
        };
        assert_eq!(kv_event.event_type(), PubSubEventType::KV);
        assert!(!kv_event.is_node_event());
        assert!(!kv_event.is_edge_event());
        assert!(kv_event.is_kv_event());
        assert!(!kv_event.is_commit_event());

        let commit_event = PubSubEvent::SnapshotCommitted { snapshot_id: 100 };
        assert_eq!(commit_event.event_type(), PubSubEventType::Commit);
        assert!(!commit_event.is_node_event());
        assert!(!commit_event.is_edge_event());
        assert!(!commit_event.is_kv_event());
        assert!(commit_event.is_commit_event());
    }

    #[test]
    fn test_snapshot_id_extraction() {
        let snapshot_id = 42u64;

        assert_eq!(
            PubSubEvent::NodeChanged {
                node_id: 1,
                snapshot_id
            }
            .snapshot_id(),
            snapshot_id
        );

        assert_eq!(
            PubSubEvent::EdgeChanged {
                edge_id: 2,
                snapshot_id
            }
            .snapshot_id(),
            snapshot_id
        );

        assert_eq!(
            PubSubEvent::KVChanged {
                key_hash: 3,
                snapshot_id
            }
            .snapshot_id(),
            snapshot_id
        );

        assert_eq!(
            PubSubEvent::SnapshotCommitted { snapshot_id }.snapshot_id(),
            snapshot_id
        );
    }

    #[test]
    fn test_id_only_design() {
        // Events don't carry payloads - just IDs
        let event = PubSubEvent::NodeChanged {
            node_id: 123,
            snapshot_id: 456,
        };

        // We can query what changed and where to find it
        assert_eq!(event.snapshot_id(), 456);
        assert!(event.is_node_event());

        // But we don't have node data here - consumer must call graph API
        // This is intentional and by design
    }

    #[test]
    fn test_event_equality() {
        let event1 = PubSubEvent::NodeChanged {
            node_id: 42,
            snapshot_id: 100,
        };
        let event2 = PubSubEvent::NodeChanged {
            node_id: 42,
            snapshot_id: 100,
        };
        let event3 = PubSubEvent::NodeChanged {
            node_id: 43,
            snapshot_id: 100,
        };

        assert_eq!(event1, event2);
        assert_ne!(event1, event3);
    }
}
