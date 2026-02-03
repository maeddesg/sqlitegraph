# Pub/Sub Implementation Guide

**Last Updated:** 2026-02-03
**Version:** v1.4.2

This guide explains how SQLiteGraph's pub/sub system works internally and how to extend it.

---

## Overview

SQLiteGraph provides an in-process, best-effort pub/sub system for Native V2 backend. Events are emitted when transactions commit and carry only identifiers (node_id, edge_id, key_hash, snapshot_id).

### Key Characteristics

| Characteristic | Description |
|----------------|-------------|
| **Scope** | In-process only (no networking/IPC) |
| **Delivery** | Best-effort via `std::sync::mpsc` channels |
| **Event Content** | ID-only (consumers read actual data using snapshot_id) |
| **Emission** | Only on commit, not rollback |
| **Persistence** | None - events are lost if not delivered |

---

## Architecture

### Module Structure

```
src/backend/native/v2/pubsub/
├── mod.rs          # Public exports and module documentation
├── event.rs        # Event type definitions (PubSubEvent, PubSubEventType)
├── subscriber.rs   # Subscription management (Subscriber, SubscriptionFilter)
├── publisher.rs    # Channel-based event delivery (Publisher)
├── emit.rs         # WAL record to event conversion
└── tests.rs        # Integration tests (59 tests passing)
```

### Data Flow

```
Transaction Commit
        │
        ▼
┌───────────────────────────────────────┐
│  V2WALManager::commit_internal()     │
│  - Writes WAL records                │
│  - Calls publisher.emit(events)      │
└───────────────┬───────────────────────┘
                │
                ▼
┌───────────────────────────────────────┐
│  Publisher::emit()                    │
│  - Iterates subscribers              │
│  - Checks each subscription filter   │
│  - Sends to matching channels        │
└───────────────┬───────────────────────┘
                │
                ▼
┌───────────────────────────────────────┐
│  Subscriber Channel (mpsc)            │
│  - Receivers get events via .recv()   │
│  - Non-blocking try_recv() available │
└───────────────────────────────────────┘
```

---

## Event Types

### PubSubEvent Enum

Located in `event.rs`:

```rust
pub enum PubSubEvent {
    NodeChanged {
        node_id: i64,
        snapshot_id: SnapshotId,
    },
    EdgeChanged {
        edge_id: i64,
        snapshot_id: SnapshotId,
    },
    KVChanged {
        key_hash: u64,
        snapshot_id: SnapshotId,
    },
    SnapshotCommitted {
        snapshot_id: SnapshotId,
    },
}
```

### Event Conversion

WAL records are converted to events in `emit.rs`:

```rust
pub fn records_to_events(
    records: &[WALRecord],
    snapshot_id: SnapshotId,
) -> Vec<PubSubEvent> {
    // Convert NodeInserted, NodeUpdated -> NodeChanged
    // Convert EdgeInserted, EdgeUpdated -> EdgeChanged
    // Convert KVPut, KVDelete -> KVChanged
    // Always emit SnapshotCommitted at the end
}
```

---

## Subscription Filtering

### SubscriptionFilter Structure

Located in `subscriber.rs`:

```rust
pub struct SubscriptionFilter {
    /// Event type filter (Node, Edge, KV, Commit, or All)
    pub event_types: Vec<PubSubEventType>,

    /// Specific node IDs to watch (empty = all nodes)
    pub node_ids: Vec<i64>,

    /// Specific edge IDs to watch (empty = all edges)
    pub edge_ids: Vec<i64>,

    /// Specific key hashes to watch (empty = all keys)
    pub key_hashes: Vec<u64>,

    /// Glob patterns for node kind (e.g., ["agent:*", "user:*"])
    pub kind_patterns: Vec<String>,

    /// Glob patterns for node name (e.g., ["msg_index:*"])
    pub name_patterns: Vec<String>,
}
```

### Filter Matching Logic

```rust
impl SubscriptionFilter {
    /// Check if an event matches this subscription
    pub fn matches(&self, event: &PubSubEvent, metadata: Option<&NodeMetadata>) -> bool {
        // 1. Check event type
        if !self.event_types.contains(&event.event_type()) {
            return false;
        }

        // 2. Check ID-based filters
        match event {
            PubSubEvent::NodeChanged { node_id, .. } => {
                if !self.node_ids.is_empty() && !self.node_ids.contains(node_id) {
                    return false;
                }
            }
            // ... similar for EdgeChanged, KVChanged
        }

        // 3. Check pattern filters (requires metadata)
        if let Some(meta) = metadata {
            if !self.kind_patterns.is_empty() {
                if !self.kind_patterns.iter().any(|p| glob_matches(p, &meta.kind)) {
                    return false;
                }
            }
            if !self.name_patterns.is_empty() {
                if !self.name_patterns.iter().any(|p| glob_matches(p, &meta.name)) {
                    return false;
                }
            }
        }

        true
    }
}
```

### Pattern Performance Note

> **Important:** Pattern filters require fetching node metadata at publish time, which has performance cost. Use ID-based filters when possible.

---

## Publisher Implementation

### Channel-Based Delivery

Located in `publisher.rs`:

```rust
pub struct Publisher {
    /// Channel senders for each subscriber
    senders: Arc<Mutex<Vec<(SubscriberId, Sender<PubSubEvent>, SubscriptionFilter)>>>,
    next_id: Arc<Mutex<u64>>,
}

impl Publisher {
    pub fn subscribe(
        &self,
        filter: SubscriptionFilter,
    ) -> Result<(SubscriberId, Receiver<PubSubEvent>), Error> {
        let (tx, rx) = mpsc::channel();
        let id = SubscriberId::new();
        let mut senders = self.senders.lock().unwrap();
        senders.push((id, tx, filter));
        Ok((id, rx))
    }

    pub fn unsubscribe(&self, id: SubscriberId) -> bool {
        let mut senders = self.senders.lock().unwrap();
        let original_len = senders.len();
        senders.retain(|(sub_id, _, _)| *sub_id != id);
        senders.len() < original_len
    }

    pub fn emit(&self, event: &PubSubEvent, metadata: Option<&NodeMetadata>) {
        let senders = self.senders.lock().unwrap();
        for (subscriber_id, sender, filter) in senders.iter() {
            if filter.matches(event, metadata) {
                // Best-effort: ignore errors if channel full/closed
                let _ = sender.send(event.clone());
            }
        }
    }
}
```

### Best-Effort Semantics

The `emit()` method uses `let _ =` to intentionally ignore send errors:
- **Channel full**: Event is dropped (no blocking on commit path)
- **Receiver dropped**: Event is dropped (subscriber gone)
- **No retry**: Events are fire-and-forget

---

## WAL Integration

### Event Emission on Commit

Events are emitted in `V2WALManager::commit_internal()`:

```rust
// After writing WAL records successfully
let events = records_to_events(&records, self.snapshot_id);

// Emit events (non-blocking)
let publisher = self.publisher.read();
for (record_index, record) in records.iter().enumerate() {
    if let Some(event) = events.get(record_index) {
        // For NodeChanged/EdgeChanged, fetch metadata for pattern matching
        let metadata = extract_node_metadata(record, &self.backend);
        publisher.emit(event, metadata.as_ref());
    }
}

// Always emit commit event
publisher.emit(&PubSubEvent::SnapshotCommitted { snapshot_id }, None);
```

### No Events on Rollback

Rollback does **not** emit events:

```rust
pub fn rollback(&self) -> Result<(), Error> {
    // Discard WAL records
    self.pending_records.write().unwrap().clear();

    // NO events emitted for rollback
    Ok(())
}
```

---

## Adding New Event Types

### Step 1: Define Event Type

Add to `event.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PubSubEvent {
    // ... existing events

    /// Your new event type
    YourEventType {
        /// Event-specific data
        your_id: u64,
        snapshot_id: SnapshotId,
    },
}
```

### Step 2: Add Event Type Category

Update `PubSubEventType`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PubSubEventType {
    // ... existing types
    YourEventType,  // Add this
}
```

### Step 3: Implement Conversion

Update `event.rs` to add `event_type()` method:

```rust
impl PubSubEvent {
    pub fn event_type(&self) -> PubSubEventType {
        match self {
            // ... existing cases
            PubSubEvent::YourEventType { .. } => PubSubEventType::YourEventType,
        }
    }
}
```

### Step 4: Add WAL Conversion

Update `emit.rs`:

```rust
pub fn records_to_events(
    records: &[WALRecord],
    snapshot_id: SnapshotId,
) -> Vec<PubSubEvent> {
    let mut events = Vec::new();

    for record in records {
        match record {
            WALRecord::YourRecordType { your_id, .. } => {
                events.push(PubSubEvent::YourEventType {
                    your_id: *your_id,
                    snapshot_id,
                });
            }
            // ... existing cases
        }
    }

    events
}
```

### Step 5: Update Filter Matching

Update `subscriber.rs`:

```rust
impl SubscriptionFilter {
    pub fn matches(&self, event: &PubSubEvent, metadata: Option<&NodeMetadata>) -> bool {
        // Check event type
        let event_type = event.event_type();
        if !self.event_types.contains(&event_type) {
            return false;
        }

        // Handle your event type specific matching
        if let PubSubEvent::YourEventType { your_id, .. } = event {
            if !self.your_ids.is_empty() && !self.your_ids.contains(your_id) {
                return false;
            }
        }

        // ... rest of matching logic
    }
}
```

---

## Testing Pub/Sub Features

### Unit Test Template

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::native::v2::pubsub::{Publisher, SubscriptionFilter};

    #[test]
    fn test_subscription_filter() {
        let filter = SubscriptionFilter {
            event_types: vec![PubSubEventType::Node],
            node_ids: vec![1, 2, 3],
            ..Default::default()
        };

        let event = PubSubEvent::NodeChanged {
            node_id: 2,
            snapshot_id: 100,
        };

        assert!(filter.matches(&event, None));
    }

    #[test]
    fn test_pattern_filter() {
        let filter = SubscriptionFilter {
            kind_patterns: vec!["agent:*".to_string()],
            ..Default::default()
        };

        let metadata = NodeMetadata {
            kind: "agent:worker".to_string(),
            name: "agent-123".to_string(),
        };

        let event = PubSubEvent::NodeChanged {
            node_id: 1,
            snapshot_id: 100,
        };

        assert!(filter.matches(&event, Some(&metadata)));
    }
}
```

### Integration Test Template

See `src/backend/native/v2/pubsub/tests.rs` for full integration test examples. Key scenarios:

1. **Event emission on commit** - verify events emitted when transaction commits
2. **No events on rollback** - verify no events when transaction rolls back
3. **Filter by event type** - subscribers only receive matching event types
4. **Filter by entity IDs** - subscribers only receive matching node/edge/key IDs
5. **Pattern filters** - glob pattern matching on kind/name
6. **Multiple subscribers** - each subscriber gets independent events
7. **Unsubscribe** - verify events stop after unsubscribe

---

## Performance Considerations

### Emission Path

Event emission is **synchronous** on the commit path. Keep it fast:

| Consideration | Recommendation |
|---------------|----------------|
| Channel capacity | Use bounded channels (default: 1000 messages) |
| Event cloning | Events are `Clone`, keep them small |
| Filter matching | O(subscribers) per event, keep subscriber count reasonable |
| Pattern matching | O(patterns) per event, use ID filters when possible |

### Memory Usage

| Component | Memory Impact |
|-----------|---------------|
| Unbounded channels | Unbounded memory growth (not recommended) |
| Per-subscriber channels | ~1KB per subscriber (sender + receiver) |
| Event queue | Depends on channel capacity and subscriber speed |

### Scaling Limits

| Limit | Approximate Value |
|-------|-------------------|
| Max subscribers | ~1000 (practical, limited by lock contention) |
| Max events/second | ~100K (depends on subscriber consumption speed) |
| Channel capacity | 1000 messages (default, configurable) |

---

## Common Patterns

### Subscribe to All Node Changes

```rust
let filter = SubscriptionFilter {
    event_types: vec![PubSubEventType::Node],
    ..Default::default()
};

let (id, rx) = publisher.subscribe(filter)?;
```

### Subscribe to Specific Node

```rust
let filter = SubscriptionFilter {
    node_ids: vec![123],
    ..Default::default()
};

let (id, rx) = publisher.subscribe(filter)?;
```

### Subscribe by Pattern (Agent Messaging)

```rust
let filter = SubscriptionFilter {
    kind_patterns: vec!["agent:*".to_string()],
    name_patterns: vec!["msg_to:agent-*".to_string()],
    ..Default::default()
};

let (id, rx) = publisher.subscribe(filter)?;
```

### Non-Polling Event Loop

```rust
use std::sync::mpsc::RecvTimeoutError;

let (id, rx) = publisher.subscribe(filter)?;

loop {
    match rx.recv_timeout(Duration::from_secs(1)) {
        Ok(event) => handle_event(event),
        Err(RecvTimeoutError::Timeout) => continue,
        Err(RecvTimeoutError::Disconnected) => break,
    }
}
```

---

## Troubleshooting

### Issue: Events not received

**Possible causes:**
1. Transaction rolled back (not committed) - events only emit on commit
2. Filter doesn't match - check event type and ID filters
3. Channel full - events dropped when channel at capacity
4. Subscriber unsubscribed - verify subscriber ID still active

**Debug:**
```rust
eprintln!("Subscriber count: {}", publisher.subscriber_count());
eprintln!("Filter: {:?}", filter);
```

### Issue: Slow commit path

**Possible cause:** Too many subscribers or expensive pattern matching

**Solutions:**
1. Reduce number of subscribers
2. Use ID-based filters instead of patterns
3. Batch operations to reduce commit frequency

### Issue: Memory growing

**Possible cause:** Slow consumer blocking channel

**Solutions:**
1. Use `try_recv()` instead of `recv()` to avoid blocking
2. Add timeout to `recv_timeout()`
3. Spawn dedicated task to drain channel

---

## References

- **Source:** `src/backend/native/v2/pubsub/`
- **Tests:** 59 passing in `pubsub/tests.rs`
- **Related:** `src/backend/native/pattern.rs` for glob matching
- **User Docs:** `MANUAL.md` Section 14: Pub/Sub Events
- **Comparison:** `docs/PUBSUB_COMPARISON.md` (Redis comparison)

---

## Future Enhancements (Not Implemented)

Potential areas for extension:

1. **Async API** - `async fn subscribe()` returning `Stream<Event>`
2. **Persistence** - Optional event persistence for replay
3. **Filter language** - More powerful expression language
4. **Event batching** - Batch multiple events per channel message
5. **Priority channels** - High/low priority event delivery

Note: These are **not currently planned** - the pub/sub system is intentionally minimal.
