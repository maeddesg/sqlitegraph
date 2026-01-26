//! Integration tests for pub/sub system
//!
//! This test module validates the complete pub/sub flow from WAL records
//! through event emission to subscriber delivery.
//!
//! # Test Coverage
//!
//! - Event emission on commit (not rollback)
//! - All event types (NodeChanged, EdgeChanged, KVChanged, SnapshotCommitted)
//! - Filter behavior by event type and entity IDs
//! - Multiple subscribers receive independent events
//! - Unsubscribe stops event delivery
//! - Best-effort delivery (no panic on dropped receiver)

use super::*;
use crate::backend::native::v2::wal::V2WALRecord;
use crate::backend::native::v2::edge_cluster::{CompactEdgeRecord, Direction};
use std::time::Duration;

// ============================================================================
// Event Emission Tests
// ============================================================================

#[test]
fn test_node_insert_emits_node_changed() {
    let publisher = Publisher::new();
    let (_id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    let records = vec![V2WALRecord::NodeInsert {
        node_id: 42,
        slot_offset: 1000,
        node_data: vec![1, 2, 3],
    }];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Should receive NodeChanged
    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(matches!(event, PubSubEvent::NodeChanged { node_id: 42, .. }));

    // Should receive SnapshotCommitted
    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(matches!(event, PubSubEvent::SnapshotCommitted { .. }));
}

#[test]
fn test_node_update_emits_node_changed() {
    let publisher = Publisher::new();
    let (_id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    let records = vec![V2WALRecord::NodeUpdate {
        node_id: 99,
        slot_offset: 2000,
        old_data: vec![1, 2, 3],
        new_data: vec![4, 5, 6],
    }];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event.is_node_event());
    if let PubSubEvent::NodeChanged { node_id, .. } = event {
        assert_eq!(node_id, 99);
    } else {
        panic!("Expected NodeChanged event");
    }
}

#[test]
fn test_edge_insert_emits_edge_changed() {
    let publisher = Publisher::new();
    let (_id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    let records = vec![V2WALRecord::EdgeInsert {
        cluster_key: (1, Direction::Outgoing),
        edge_record: CompactEdgeRecord::new(2, 0, vec![]),
        insertion_point: 0,
    }];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event.is_edge_event());
}

#[test]
fn test_edge_update_emits_edge_changed() {
    let publisher = Publisher::new();
    let (_id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    let records = vec![V2WALRecord::EdgeUpdate {
        cluster_key: (5, Direction::Incoming),
        old_edge: CompactEdgeRecord::new(10, 0, vec![]),
        new_edge: CompactEdgeRecord::new(11, 0, vec![]),
        position: 0,
    }];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event.is_edge_event());
}

#[test]
fn test_edge_delete_emits_edge_changed() {
    let publisher = Publisher::new();
    let (_id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    let records = vec![V2WALRecord::EdgeDelete {
        cluster_key: (3, Direction::Outgoing),
        old_edge: CompactEdgeRecord::new(7, 0, vec![]),
        position: 5,
    }];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event.is_edge_event());
}

#[test]
fn test_kv_operations_emit_kv_changed() {
    let publisher = Publisher::new();
    let (_id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    // Test KvSet
    let records = vec![V2WALRecord::KvSet {
        key: vec![1, 2, 3],
        value_bytes: vec![4, 5, 6],
        value_type: 0,
        ttl_seconds: None,
        version: 0,
    }];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event.is_kv_event());

    // Verify SnapshotCommitted also received
    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event.is_commit_event());
}

#[test]
fn test_node_delete_emits_no_event() {
    let publisher = Publisher::new();
    let (_id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    let records = vec![V2WALRecord::NodeDelete {
        node_id: 42,
        slot_offset: 1000,
        old_data: vec![1, 2, 3],
        outgoing_edges: vec![],
        incoming_edges: vec![],
    }];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Should only receive SnapshotCommitted (no NodeChanged)
    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event.is_commit_event());

    // No more events
    let result = rx.recv_timeout(Duration::from_millis(50));
    assert!(result.is_err());
}

#[test]
fn test_multiple_commits_emits_multiple_snapshot_events() {
    let publisher = Publisher::new();
    let (_id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    // First commit
    let records1 = vec![V2WALRecord::NodeInsert {
        node_id: 1,
        slot_offset: 0,
        node_data: vec![],
    }];
    let events1 = emit::records_to_events(&records1, 100);
    for event in events1 {
        publisher.emit(event);
    }

    // Second commit
    let records2 = vec![V2WALRecord::NodeInsert {
        node_id: 2,
        slot_offset: 0,
        node_data: vec![],
    }];
    let events2 = emit::records_to_events(&records2, 200);
    for event in events2 {
        publisher.emit(event);
    }

    // Should receive events from both commits
    let mut snapshot_count = 0;
    for _ in 0..4 {
        // 2 NodeChanged + 2 SnapshotCommitted
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(100)) {
            if event.is_commit_event() {
                snapshot_count += 1;
            }
        }
    }
    assert_eq!(snapshot_count, 2);
}

#[test]
fn test_mixed_records_in_single_transaction() {
    let publisher = Publisher::new();
    let (_id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    let records = vec![
        V2WALRecord::NodeInsert {
            node_id: 1,
            slot_offset: 100,
            node_data: vec![],
        },
        V2WALRecord::EdgeInsert {
            cluster_key: (1, Direction::Outgoing),
            edge_record: CompactEdgeRecord::new(2, 0, vec![]),
            insertion_point: 0,
        },
        V2WALRecord::KvSet {
            key: vec![9, 9, 9],
            value_bytes: vec![1, 2, 3],
            value_type: 0,
            ttl_seconds: None,
            version: 0,
        },
    ];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Should receive all three entity events + SnapshotCommitted
    let events_received = vec![
        rx.recv_timeout(Duration::from_millis(100)).unwrap(),
        rx.recv_timeout(Duration::from_millis(100)).unwrap(),
        rx.recv_timeout(Duration::from_millis(100)).unwrap(),
        rx.recv_timeout(Duration::from_millis(100)).unwrap(),
    ];

    let node_count = events_received.iter().filter(|e| e.is_node_event()).count();
    let edge_count = events_received.iter().filter(|e| e.is_edge_event()).count();
    let kv_count = events_received.iter().filter(|e| e.is_kv_event()).count();
    let commit_count = events_received
        .iter()
        .filter(|e| e.is_commit_event())
        .count();

    assert_eq!(node_count, 1);
    assert_eq!(edge_count, 1);
    assert_eq!(kv_count, 1);
    assert_eq!(commit_count, 1);
}

// ============================================================================
// Filter Tests
// ============================================================================

#[test]
fn test_filter_event_type() {
    let publisher = Publisher::new();
    let node_filter = SubscriptionFilter::event_types(vec![PubSubEventType::Node]);
    let (_id, mut rx) = publisher.subscribe(node_filter);

    let records = vec![V2WALRecord::NodeInsert {
        node_id: 1,
        slot_offset: 0,
        node_data: vec![],
    }];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Should receive NodeChanged but not SnapshotCommitted
    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event.is_node_event());

    // Channel should be empty (SnapshotCommitted filtered)
    let result = rx.recv_timeout(Duration::from_millis(50));
    assert!(result.is_err());
}

#[test]
fn test_filter_specific_node() {
    let publisher = Publisher::new();
    let filter = SubscriptionFilter::nodes(vec![42]);
    let (_id, mut rx) = publisher.subscribe(filter);

    let records = vec![
        V2WALRecord::NodeInsert {
            node_id: 42,
            slot_offset: 0,
            node_data: vec![],
        },
        V2WALRecord::NodeInsert {
            node_id: 99, // Different node
            slot_offset: 0,
            node_data: vec![],
        },
    ];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Should only receive event for node 42
    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    if let PubSubEvent::NodeChanged { node_id, .. } = event {
        assert_eq!(node_id, 42);
    } else {
        panic!("Expected NodeChanged for node 42");
    }

    // No more events
    let result = rx.recv_timeout(Duration::from_millis(50));
    assert!(result.is_err());
}

#[test]
fn test_filter_specific_edges() {
    let publisher = Publisher::new();
    let filter = SubscriptionFilter::event_types(vec![PubSubEventType::Edge]);
    let (_id, mut rx) = publisher.subscribe(filter);

    let records = vec![
        V2WALRecord::EdgeInsert {
            cluster_key: (1, Direction::Outgoing),
            edge_record: CompactEdgeRecord::new(2, 0, vec![]),
            insertion_point: 0,
        },
        V2WALRecord::NodeInsert {
            node_id: 99, // Should be filtered
            slot_offset: 0,
            node_data: vec![],
        },
    ];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Should only receive EdgeChanged
    let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event.is_edge_event());

    // No more events
    let result = rx.recv_timeout(Duration::from_millis(50));
    assert!(result.is_err());
}

#[test]
fn test_filter_multiple_event_types() {
    let publisher = Publisher::new();
    let filter = SubscriptionFilter::event_types(vec![
        PubSubEventType::Node,
        PubSubEventType::Edge,
    ]);
    let (_id, mut rx) = publisher.subscribe(filter);

    let records = vec![
        V2WALRecord::NodeInsert {
            node_id: 1,
            slot_offset: 0,
            node_data: vec![],
        },
        V2WALRecord::EdgeInsert {
            cluster_key: (1, Direction::Outgoing),
            edge_record: CompactEdgeRecord::new(2, 0, vec![]),
            insertion_point: 0,
        },
        V2WALRecord::KvSet {
            key: vec![9, 9, 9], // Should be filtered
            value_bytes: vec![],
            value_type: 0,
            ttl_seconds: None,
            version: 0,
        },
    ];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Should receive NodeChanged and EdgeChanged (not KV or SnapshotCommitted)
    let event1 = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event1.is_node_event() || event1.is_edge_event());

    let event2 = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event2.is_node_event() || event2.is_edge_event());

    // No more events
    let result = rx.recv_timeout(Duration::from_millis(50));
    assert!(result.is_err());
}

// ============================================================================
// Multiple Subscriber Tests
// ============================================================================

#[test]
fn test_multiple_subscribers() {
    let publisher = Publisher::new();
    let (_id1, mut rx1) = publisher.subscribe(SubscriptionFilter::all());
    let (_id2, mut rx2) = publisher.subscribe(SubscriptionFilter::all());

    let records = vec![V2WALRecord::NodeInsert {
        node_id: 1,
        slot_offset: 0,
        node_data: vec![],
    }];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Both subscribers should receive events
    let event1 = rx1.recv_timeout(Duration::from_millis(100)).unwrap();
    let event2 = rx2.recv_timeout(Duration::from_millis(100)).unwrap();

    // Events should be equivalent (same snapshot_id)
    assert_eq!(event1.snapshot_id(), event2.snapshot_id());
}

#[test]
fn test_multiple_subscribers_different_filters() {
    let publisher = Publisher::new();

    // Subscriber 1: Only node events
    let node_filter = SubscriptionFilter::event_types(vec![PubSubEventType::Node]);
    let (_id1, mut rx1) = publisher.subscribe(node_filter);

    // Subscriber 2: Only edge events
    let edge_filter = SubscriptionFilter::event_types(vec![PubSubEventType::Edge]);
    let (_id2, mut rx2) = publisher.subscribe(edge_filter);

    let records = vec![
        V2WALRecord::NodeInsert {
            node_id: 1,
            slot_offset: 0,
            node_data: vec![],
        },
        V2WALRecord::EdgeInsert {
            cluster_key: (1, Direction::Outgoing),
            edge_record: CompactEdgeRecord::new(2, 0, vec![]),
            insertion_point: 0,
        },
    ];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Subscriber 1 should receive node event
    let event1 = rx1.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event1.is_node_event());

    // Subscriber 2 should receive edge event
    let event2 = rx2.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event2.is_edge_event());

    // Subscriber 1 should have no more events
    let result = rx1.recv_timeout(Duration::from_millis(50));
    assert!(result.is_err());

    // Subscriber 2 should have no more events
    let result = rx2.recv_timeout(Duration::from_millis(50));
    assert!(result.is_err());
}

#[test]
fn test_three_subscribers_mixed_filters() {
    let publisher = Publisher::new();

    // Subscriber 1: All events
    let (_id1, mut rx1) = publisher.subscribe(SubscriptionFilter::all());

    // Subscriber 2: Only nodes 1 and 2
    let (_id2, mut rx2) = publisher.subscribe(SubscriptionFilter::nodes(vec![1, 2]));

    // Subscriber 3: Only node 3
    let (_id3, mut rx3) = publisher.subscribe(SubscriptionFilter::nodes(vec![3]));

    let records = vec![
        V2WALRecord::NodeInsert {
            node_id: 1,
            slot_offset: 0,
            node_data: vec![],
        },
        V2WALRecord::NodeInsert {
            node_id: 2,
            slot_offset: 0,
            node_data: vec![],
        },
    ];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Subscriber 1 should receive 2 NodeChanged + 1 SnapshotCommitted
    let _ = rx1.recv_timeout(Duration::from_millis(100)).unwrap();
    let _ = rx1.recv_timeout(Duration::from_millis(100)).unwrap();
    let _ = rx1.recv_timeout(Duration::from_millis(100)).unwrap();

    // Subscriber 2 should receive 2 NodeChanged
    let _ = rx2.recv_timeout(Duration::from_millis(100)).unwrap();
    let _ = rx2.recv_timeout(Duration::from_millis(100)).unwrap();

    // Subscriber 3 should receive no events
    let result = rx3.recv_timeout(Duration::from_millis(50));
    assert!(result.is_err());
}

// ============================================================================
// Unsubscribe Tests
// ============================================================================

#[test]
fn test_unsubscribe_stops_events() {
    let publisher = Publisher::new();
    let (id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    // First commit
    let records1 = vec![V2WALRecord::NodeInsert {
        node_id: 1,
        slot_offset: 0,
        node_data: vec![],
    }];
    let events1 = emit::records_to_events(&records1, 100);
    for event in events1 {
        publisher.emit(event);
    }

    // Should receive NodeChanged and SnapshotCommitted
    let _ = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    let _ = rx.recv_timeout(Duration::from_millis(100)).unwrap();

    // Unsubscribe
    assert!(publisher.unsubscribe(id));

    // Second commit
    let records2 = vec![V2WALRecord::NodeInsert {
        node_id: 2,
        slot_offset: 0,
        node_data: vec![],
    }];
    let events2 = emit::records_to_events(&records2, 200);
    for event in events2 {
        publisher.emit(event);
    }

    // Should NOT receive event
    let result = rx.recv_timeout(Duration::from_millis(100));
    assert!(result.is_err());
}

#[test]
fn test_unsubscribe_nonexistent_returns_false() {
    let publisher = Publisher::new();
    let fake_id = SubscriberId::from_raw(99999);
    assert!(!publisher.unsubscribe(fake_id));
}

#[test]
fn test_unsubscribe_then_resubscribe() {
    let publisher = Publisher::new();
    let (id1, mut rx1) = publisher.subscribe(SubscriptionFilter::all());

    // First commit
    let records1 = vec![V2WALRecord::NodeInsert {
        node_id: 1,
        slot_offset: 0,
        node_data: vec![],
    }];
    let events1 = emit::records_to_events(&records1, 100);
    for event in events1 {
        publisher.emit(event);
    }

    // Should receive NodeChanged and SnapshotCommitted
    let _ = rx1.recv_timeout(Duration::from_millis(100)).unwrap();
    let _ = rx1.recv_timeout(Duration::from_millis(100)).unwrap();

    // Unsubscribe
    assert!(publisher.unsubscribe(id1));

    // Second commit (should not receive)
    let records2 = vec![V2WALRecord::NodeInsert {
        node_id: 2,
        slot_offset: 0,
        node_data: vec![],
    }];
    let events2 = emit::records_to_events(&records2, 200);
    for event in events2 {
        publisher.emit(event);
    }

    let result = rx1.recv_timeout(Duration::from_millis(100));
    assert!(result.is_err());

    // Resubscribe
    let (_id2, mut rx2) = publisher.subscribe(SubscriptionFilter::all());

    // Third commit (should receive again)
    let records3 = vec![V2WALRecord::NodeInsert {
        node_id: 3,
        slot_offset: 0,
        node_data: vec![],
    }];
    let events3 = emit::records_to_events(&records3, 300);
    for event in events3 {
        publisher.emit(event);
    }

    rx2.recv_timeout(Duration::from_millis(100)).unwrap();
}

// ============================================================================
// Best-Effort Delivery Tests
// ============================================================================

#[test]
fn test_best_effort_dropped_receiver() {
    let publisher = Publisher::new();
    let (id, rx) = publisher.subscribe(SubscriptionFilter::all());

    // Drop receiver immediately
    drop(rx);

    // Emit should not block or panic
    let records = vec![V2WALRecord::NodeInsert {
        node_id: 1,
        slot_offset: 0,
        node_data: vec![],
    }];
    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }
    // This should succeed despite dropped receiver

    // Unsubscribe should clean up
    assert!(publisher.unsubscribe(id));
}

#[test]
fn test_best_effort_one_dropped_one_active() {
    let publisher = Publisher::new();

    // Subscriber 1: Will drop receiver
    let (_id1, rx1) = publisher.subscribe(SubscriptionFilter::all());

    // Subscriber 2: Will keep receiving
    let (_id2, mut rx2) = publisher.subscribe(SubscriptionFilter::all());

    // Drop first receiver
    drop(rx1);

    // Emit should succeed and second subscriber should still receive
    let records = vec![V2WALRecord::NodeInsert {
        node_id: 1,
        slot_offset: 0,
        node_data: vec![],
    }];
    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Second subscriber should receive event
    let event = rx2.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(event.is_node_event() || event.is_commit_event());
}

// ============================================================================
// Snapshot ID Tests
// ============================================================================

#[test]
fn test_snapshot_id_monotonically_increases() {
    let publisher = Publisher::new();
    let (_id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    let mut prev_snapshot_id = 0;

    // Commit 3 transactions
    for i in 0..3 {
        let records = vec![V2WALRecord::NodeInsert {
            node_id: i,
            slot_offset: 0,
            node_data: vec![],
        }];
        let events = emit::records_to_events(&records, 100 + i as u64);
        for event in events {
            publisher.emit(event);
        }

        // Receive NodeChanged first (skip it)
        let _ = rx.recv_timeout(Duration::from_millis(100)).unwrap();

        // Receive SnapshotCommitted event
        let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
        if let PubSubEvent::SnapshotCommitted { snapshot_id } = event {
            assert!(snapshot_id > prev_snapshot_id);
            prev_snapshot_id = snapshot_id;
        }
    }

    assert!(prev_snapshot_id > 0);
}

#[test]
fn test_all_events_in_transaction_share_snapshot_id() {
    let publisher = Publisher::new();
    let (_id, mut rx) = publisher.subscribe(SubscriptionFilter::all());

    let records = vec![
        V2WALRecord::NodeInsert {
            node_id: 1,
            slot_offset: 0,
            node_data: vec![],
        },
        V2WALRecord::EdgeInsert {
            cluster_key: (1, Direction::Outgoing),
            edge_record: CompactEdgeRecord::new(2, 0, vec![]),
            insertion_point: 0,
        },
    ];

    let events = emit::records_to_events(&records, 100);
    for event in events {
        publisher.emit(event);
    }

    // Collect all events
    let event1 = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    let event2 = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    let event3 = rx.recv_timeout(Duration::from_millis(100)).unwrap();

    // All should have the same snapshot_id
    let snapshot_id = event1.snapshot_id();
    assert_eq!(event2.snapshot_id(), snapshot_id);
    assert_eq!(event3.snapshot_id(), snapshot_id);
}
