//! WAL record to pub/sub event conversion
//!
//! This module converts V2WALRecord types into PubSubEvent types for emission
//! to subscribers. Events are emitted only for committed transactions.

use crate::backend::native::v2::edge_cluster::CompactEdgeRecord;
use crate::backend::native::v2::pubsub::PubSubEvent;
use crate::backend::native::v2::wal::V2WALRecord;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Convert WAL records to pub/sub events
///
/// Iterates through WAL records and emits events for data modifications.
/// Returns the final SnapshotCommitted event.
///
/// # Arguments
///
/// * `records` - WAL records from a committed transaction
/// * `commit_lsn` - The commit LSN (used as snapshot_id for all events)
///
/// # Returns
///
/// A vector of PubSubEvent objects, with SnapshotCommitted always last.
///
/// # Event Emission Rules
///
/// - NodeInsert → NodeChanged
/// - NodeUpdate → NodeChanged
/// - NodeDelete → NO EVENT (entity is gone)
/// - EdgeInsert → EdgeChanged
/// - EdgeUpdate → EdgeChanged
/// - EdgeDelete → EdgeChanged (consumer can check if exists)
/// - KvSet → KVChanged
/// - KvDelete → KVChanged
/// - Transaction control (Begin/Commit/Rollback) → NO EVENT
/// - Checkpoint/Header/other metadata → NO EVENT
pub fn records_to_events(records: &[V2WALRecord], commit_lsn: u64) -> Vec<PubSubEvent> {
    let mut events = Vec::new();

    for record in records {
        match record {
            // Node events
            V2WALRecord::NodeInsert { node_id, .. } => {
                events.push(PubSubEvent::NodeChanged {
                    node_id: *node_id,
                    snapshot_id: commit_lsn,
                });
            }
            V2WALRecord::NodeUpdate { node_id, .. } => {
                events.push(PubSubEvent::NodeChanged {
                    node_id: *node_id,
                    snapshot_id: commit_lsn,
                });
            }
            // Note: NodeDelete does NOT emit NodeChanged (entity gone)

            // Edge events
            V2WALRecord::EdgeInsert { edge_record, .. } => {
                let edge_id = edge_id_from_record(edge_record);
                events.push(PubSubEvent::EdgeChanged {
                    edge_id,
                    snapshot_id: commit_lsn,
                });
            }
            V2WALRecord::EdgeUpdate { new_edge, .. } => {
                let edge_id = edge_id_from_record(new_edge);
                events.push(PubSubEvent::EdgeChanged {
                    edge_id,
                    snapshot_id: commit_lsn,
                });
            }
            V2WALRecord::EdgeDelete { .. } => {
                // Edge deletion also emits EdgeChanged (consumer can check if exists)
                let edge_id = 0; // Deleted edge - ID not meaningful
                events.push(PubSubEvent::EdgeChanged {
                    edge_id,
                    snapshot_id: commit_lsn,
                });
            }

            // KV events (from Phase 43)
            V2WALRecord::KvSet { key, .. } => {
                let key_hash = key_hash_bytes(key);
                events.push(PubSubEvent::KVChanged {
                    key_hash,
                    snapshot_id: commit_lsn,
                });
            }
            V2WALRecord::KvDelete { key, .. } => {
                let key_hash = key_hash_bytes(key);
                events.push(PubSubEvent::KVChanged {
                    key_hash,
                    snapshot_id: commit_lsn,
                });
            }

            // Ignore transaction control, checkpoint, and other metadata records
            _ => {}
        }
    }

    // Always emit SnapshotCommitted at the end
    events.push(PubSubEvent::SnapshotCommitted {
        snapshot_id: commit_lsn,
    });

    events
}

/// Generate edge ID from CompactEdgeRecord
///
/// Combines neighbor_id and edge_type_offset into a single i64 for
/// notification purposes. This is NOT a storage ID, just a compact
/// representation for event notification.
fn edge_id_from_record(edge: &CompactEdgeRecord) -> i64 {
    // Combine neighbor_id (lower 48 bits) and edge_type_offset (upper 16 bits)
    // This creates a unique identifier for the edge within its cluster
    let neighbor_part = (edge.neighbor_id as u64) & 0xFFFFFFFFFFFFu64; // 48 bits
    let type_part = (edge.edge_type_offset as u64) << 48; // Upper 16 bits
    (neighbor_part | type_part) as i64
}

/// Hash key bytes for KV events
///
/// Uses std::collections::hash_map::DefaultHasher to generate a consistent
/// hash of the key bytes. This provides privacy (key not exposed) and
/// efficiency (fixed-size u64 instead of variable-length Vec<u8>).
fn key_hash_bytes(key: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// Check if a WAL record type should emit an event
///
/// Returns true if the record type corresponds to a data modification that
/// should trigger a pub/sub event.
///
/// # Examples
///
/// ```
/// use sqlitegraph::backend::native::v2::wal::V2WALRecord;
/// use sqlitegraph::backend::native::v2::pubsub::emit::should_emit_event;
///
/// let record = V2WALRecord::NodeInsert {
///     node_id: 1,
///     slot_offset: 0,
///     node_data: vec![1, 2, 3],
/// };
/// assert!(should_emit_event(&record));
///
/// let record = V2WALRecord::TransactionBegin {
///     tx_id: 1,
///     timestamp: 0,
/// };
/// assert!(!should_emit_event(&record));
/// ```
pub fn should_emit_event(record: &V2WALRecord) -> bool {
    matches!(
        record,
        V2WALRecord::NodeInsert { .. }
            | V2WALRecord::NodeUpdate { .. }
            | V2WALRecord::EdgeInsert { .. }
            | V2WALRecord::EdgeUpdate { .. }
            | V2WALRecord::EdgeDelete { .. }
            | V2WALRecord::KvSet { .. }
            | V2WALRecord::KvDelete { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_insert_emits_node_changed() {
        let records = vec![V2WALRecord::NodeInsert {
            node_id: 42,
            slot_offset: 0,
            node_data: vec![1, 2, 3],
        }];
        let events = records_to_events(&records, 100);

        assert_eq!(events.len(), 2); // NodeChanged + SnapshotCommitted
        assert!(events[0].is_node_event());
        match &events[0] {
            PubSubEvent::NodeChanged {
                node_id,
                snapshot_id,
            } => {
                assert_eq!(*node_id, 42);
                assert_eq!(*snapshot_id, 100);
            }
            _ => panic!("Expected NodeChanged event"),
        }
        assert!(events[1].is_commit_event());
    }

    #[test]
    fn test_edge_insert_emits_edge_changed() {
        let edge_record = CompactEdgeRecord::new(123, 45, vec![6, 7, 8]);
        let records = vec![V2WALRecord::EdgeInsert {
            cluster_key: (
                42,
                crate::backend::native::v2::edge_cluster::cluster_trace::Direction::Outgoing,
            ),
            edge_record,
            insertion_point: 0,
        }];
        let events = records_to_events(&records, 200);

        assert_eq!(events.len(), 2); // EdgeChanged + SnapshotCommitted
        assert!(events[0].is_edge_event());
        match &events[0] {
            PubSubEvent::EdgeChanged {
                edge_id,
                snapshot_id,
            } => {
                assert_eq!(*snapshot_id, 200);
                // edge_id should combine neighbor_id and type_offset
                assert_ne!(*edge_id, 0);
            }
            _ => panic!("Expected EdgeChanged event"),
        }
        assert!(events[1].is_commit_event());
    }

    #[test]
    fn test_snapshot_committed_always_emitted() {
        let records = vec![
            V2WALRecord::NodeInsert {
                node_id: 1,
                slot_offset: 0,
                node_data: vec![1, 2, 3],
            },
            V2WALRecord::NodeUpdate {
                node_id: 2,
                slot_offset: 0,
                old_data: vec![4, 5],
                new_data: vec![6, 7],
            },
        ];
        let events = records_to_events(&records, 300);

        // Should have 2 NodeChanged + 1 SnapshotCommitted
        assert_eq!(events.len(), 3);
        assert!(events[0].is_node_event());
        assert!(events[1].is_node_event());
        assert!(events[2].is_commit_event());

        match &events[2] {
            PubSubEvent::SnapshotCommitted { snapshot_id } => {
                assert_eq!(*snapshot_id, 300);
            }
            _ => panic!("Expected SnapshotCommitted event"),
        }
    }

    #[test]
    fn test_transaction_records_ignored() {
        let records = vec![
            V2WALRecord::TransactionBegin {
                tx_id: 1,
                timestamp: 0,
            },
            V2WALRecord::TransactionCommit {
                tx_id: 1,
                timestamp: 0,
            },
        ];
        let events = records_to_events(&records, 400);

        // Only SnapshotCommitted should be emitted
        assert_eq!(events.len(), 1);
        assert!(events[0].is_commit_event());
    }

    #[test]
    fn test_kv_set_emits_kv_changed() {
        let records = vec![V2WALRecord::KvSet {
            key: b"test_key".to_vec(),
            value_bytes: b"test_value".to_vec(),
            value_type: 1,
            ttl_seconds: Some(3600),
            version: 123,
        }];
        let events = records_to_events(&records, 500);

        assert_eq!(events.len(), 2); // KVChanged + SnapshotCommitted
        assert!(events[0].is_kv_event());
        match &events[0] {
            PubSubEvent::KVChanged {
                key_hash,
                snapshot_id,
            } => {
                assert_eq!(*snapshot_id, 500);
                assert_ne!(*key_hash, 0); // Should have a hash
            }
            _ => panic!("Expected KVChanged event"),
        }
    }

    #[test]
    fn test_kv_delete_emits_kv_changed() {
        let records = vec![V2WALRecord::KvDelete {
            key: b"deleted_key".to_vec(),
            old_value_bytes: Some(b"old_value".to_vec()),
            old_value_type: 1,
            old_version: 122,
        }];
        let events = records_to_events(&records, 600);

        assert_eq!(events.len(), 2); // KVChanged + SnapshotCommitted
        assert!(events[0].is_kv_event());
    }

    #[test]
    fn test_node_delete_emits_no_event() {
        let records = vec![V2WALRecord::NodeDelete {
            node_id: 99,
            slot_offset: 0,
            old_data: vec![1, 2, 3],
            outgoing_edges: vec![],
            incoming_edges: vec![],
        }];
        let events = records_to_events(&records, 700);

        // Only SnapshotCommitted - NodeDelete doesn't emit NodeChanged
        assert_eq!(events.len(), 1);
        assert!(events[0].is_commit_event());
    }

    #[test]
    fn test_edge_id_from_record() {
        let edge = CompactEdgeRecord::new(12345, 67, vec![]);
        let edge_id = edge_id_from_record(&edge);

        // Verify the encoding
        assert_ne!(edge_id, 0);

        // Lower 48 bits should contain neighbor_id
        let neighbor_part = (edge_id as u64) & 0xFFFFFFFFFFFF;
        assert_eq!(neighbor_part as i64, 12345);

        // Upper 16 bits should contain edge_type_offset
        let type_part = (edge_id as u64) >> 48;
        assert_eq!(type_part as u16, 67);
    }

    #[test]
    fn test_key_hash_consistency() {
        let key1 = b"test_key";
        let key2 = b"test_key";
        let key3 = b"other_key";

        let hash1 = key_hash_bytes(key1);
        let hash2 = key_hash_bytes(key2);
        let hash3 = key_hash_bytes(key3);

        // Same key should produce same hash
        assert_eq!(hash1, hash2);

        // Different keys should (likely) produce different hashes
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_should_emit_event() {
        // Records that should emit events
        assert!(should_emit_event(&V2WALRecord::NodeInsert {
            node_id: 1,
            slot_offset: 0,
            node_data: vec![],
        }));
        assert!(should_emit_event(&V2WALRecord::NodeUpdate {
            node_id: 1,
            slot_offset: 0,
            old_data: vec![],
            new_data: vec![],
        }));
        assert!(should_emit_event(&V2WALRecord::EdgeInsert {
            cluster_key: (
                1,
                crate::backend::native::v2::edge_cluster::cluster_trace::Direction::Outgoing
            ),
            edge_record: CompactEdgeRecord::new(2, 3, vec![]),
            insertion_point: 0,
        }));
        assert!(should_emit_event(&V2WALRecord::KvSet {
            key: vec![],
            value_bytes: vec![],
            value_type: 0,
            ttl_seconds: None,
            version: 0,
        }));

        // Records that should NOT emit events
        assert!(!should_emit_event(&V2WALRecord::NodeDelete {
            node_id: 1,
            slot_offset: 0,
            old_data: vec![],
            outgoing_edges: vec![],
            incoming_edges: vec![],
        }));
        assert!(!should_emit_event(&V2WALRecord::TransactionBegin {
            tx_id: 1,
            timestamp: 0,
        }));
        assert!(!should_emit_event(&V2WALRecord::TransactionCommit {
            tx_id: 1,
            timestamp: 0,
        }));
        assert!(!should_emit_event(&V2WALRecord::Checkpoint {
            checkpointed_lsn: 0,
            timestamp: 0,
        }));
    }

    #[test]
    fn test_empty_records_emits_only_snapshot_committed() {
        let records: Vec<V2WALRecord> = vec![];
        let events = records_to_events(&records, 800);

        assert_eq!(events.len(), 1);
        assert!(events[0].is_commit_event());
        match &events[0] {
            PubSubEvent::SnapshotCommitted { snapshot_id } => {
                assert_eq!(*snapshot_id, 800);
            }
            _ => panic!("Expected SnapshotCommitted event"),
        }
    }

    #[test]
    fn test_multiple_operations_same_transaction() {
        let records = vec![
            V2WALRecord::NodeInsert {
                node_id: 1,
                slot_offset: 0,
                node_data: vec![1, 2, 3],
            },
            V2WALRecord::NodeInsert {
                node_id: 2,
                slot_offset: 0,
                node_data: vec![4, 5, 6],
            },
            V2WALRecord::KvSet {
                key: b"key1".to_vec(),
                value_bytes: b"value1".to_vec(),
                value_type: 0,
                ttl_seconds: None,
                version: 1,
            },
            V2WALRecord::EdgeInsert {
                cluster_key: (
                    1,
                    crate::backend::native::v2::edge_cluster::cluster_trace::Direction::Outgoing,
                ),
                edge_record: CompactEdgeRecord::new(2, 1, vec![]),
                insertion_point: 0,
            },
        ];
        let events = records_to_events(&records, 900);

        // Should have 2 NodeChanged + 1 KVChanged + 1 EdgeChanged + 1 SnapshotCommitted
        assert_eq!(events.len(), 5);
        assert!(events[0].is_node_event());
        assert!(events[1].is_node_event());
        assert!(events[2].is_kv_event());
        assert!(events[3].is_edge_event());
        assert!(events[4].is_commit_event());
    }
}
