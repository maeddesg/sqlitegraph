# Pub/Sub Implementation Guide

**Last Updated:** 2026-02-12  
**Version:** v1.6.0

This guide explains SQLiteGraph's pub/sub system, available on **all backends** (SQLite, V3, and V2).

---

## Overview

SQLiteGraph provides an in-process, best-effort pub/sub system for graph change notifications. Events are emitted synchronously when operations complete and carry only identifiers (consumers read actual data using the snapshot_id if needed).

### Backend Support

| Backend | Status | Implementation | Notes |
|---------|--------|----------------|-------|
| **SQLite** | ✅ Full | In-memory Publisher | New in v1.6.0 |
| **Native V3** | ✅ Full | Lazy-initialized Publisher | Zero overhead if unused |
| **Native V2** | ✅ Full | In-memory Publisher | Original implementation |

### Key Characteristics

| Characteristic | Description |
|----------------|-------------|
| **Scope** | In-process only (no networking/IPC) |
| **Delivery** | Best-effort via `std::sync::mpsc` channels |
| **Event Content** | ID-only (consumers read actual data using snapshot_id) |
| **Emission** | Synchronous on successful operation |
| **Persistence** | None - events are lost if not delivered |
| **Filtering** | By event type (Node, Edge, KV, Commit) |

---

## Architecture

### Module Structure

**Generic Types** (all backends):
```
src/backend/mod.rs
├── PubSubEvent       # Event enum (NodeChanged, EdgeChanged, KVChanged, SnapshotCommitted)
├── PubSubEventType   # Event type enum
└── SubscriptionFilter # Filter struct
```

**Per-Backend Implementation:**

```
SQLite Backend:
src/backend/sqlite/impl_.rs
├── Publisher (private struct)
├── subscribe() -> (u64, Receiver<PubSubEvent>)
└── Events emitted in insert_node(), insert_edge()

V3 Backend:
src/backend/native/v3/
├── backend.rs
│   ├── publisher: RwLock<Option<Publisher>>  # Lazy init
│   ├── subscribe() / unsubscribe()
│   └── Events emitted in insert_node(), insert_edge(), kv_set()
└── pubsub/
    ├── publisher.rs    # Publisher implementation
    └── types.rs        # V3-specific types
```

### Data Flow

```
User calls insert_node()
        │
        ▼
┌───────────────────────────────────────┐
│  Backend::insert_node()              │
│  - Write to storage                  │
│  - If publisher initialized:         │
│    publisher.emit(NodeChanged {...}) │
└───────────────┬───────────────────────┘
                │
                ▼
┌───────────────────────────────────────┐
│  Publisher::emit()                    │
│  - Iterate subscribers               │
│  - Check SubscriptionFilter::matches │
│  - Send to matching channels         │
└───────────────┬───────────────────────┘
                │
                ▼
┌───────────────────────────────────────┐
│  Subscriber Receiver (mpsc)           │
│  - User code receives via .recv()    │
└───────────────────────────────────────┘
```

---

## Event Types

### PubSubEvent Enum (All Backends)

```rust
pub enum PubSubEvent {
    NodeChanged {
        node_id: i64,
        snapshot_id: u64,      // Snapshot when change occurred
    },
    EdgeChanged {
        edge_id: i64,
        snapshot_id: u64,
    },
    KVChanged {
        key_hash: u64,         // Hash of the key (not the key itself)
        snapshot_id: u64,
    },
    SnapshotCommitted {
        snapshot_id: u64,      // Transaction committed
    },
}
```

### Event Emission by Backend

| Operation | SQLite | V3 | V2 |
|-----------|--------|----|----|
| `insert_node()` | ✅ NodeChanged | ✅ NodeChanged | ✅ NodeChanged |
| `insert_edge()` | ✅ EdgeChanged | ✅ EdgeChanged | ✅ EdgeChanged |
| `kv_set()` | ❌ N/A | ✅ KVChanged | ✅ KVChanged |
| `kv_delete()` | ❌ N/A | ✅ KVChanged | ✅ KVChanged |
| Transaction commit | ❌ (no explicit tx) | ✅ SnapshotCommitted | ✅ SnapshotCommitted |

---

## Usage Examples

### Basic Subscription (Any Backend)

```rust
use sqlitegraph::backend::{GraphBackend, NodeSpec, SubscriptionFilter, PubSubEvent};

fn watch_changes(backend: &dyn GraphBackend) -> Result<(), SqliteGraphError> {
    // Subscribe to all node changes
    let filter = SubscriptionFilter {
        node_changes: true,
        edge_changes: false,
        kv_changes: false,
        snapshot_commits: false,
    };
    
    let (sub_id, rx) = backend.subscribe(filter)?;
    
    // Spawn receiver thread
    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            match event {
                PubSubEvent::NodeChanged { node_id, snapshot_id } => {
                    println!("Node {} changed at snapshot {}", node_id, snapshot_id);
                }
                _ => {}
            }
        }
    });
    
    // Create a node - event will be emitted
    backend.insert_node(NodeSpec {
        kind: "User".to_string(),
        name: "Alice".to_string(),
        file_path: None,
        data: serde_json::json!({"role": "admin"}),
    })?;
    
    // Cleanup
    backend.unsubscribe(sub_id)?;
    Ok(())
}
```

### Filtered Subscription

```rust
// Only edge changes
let filter = SubscriptionFilter {
    node_changes: false,
    edge_changes: true,
    kv_changes: false,
    snapshot_commits: false,
};

// Multiple types
let filter = SubscriptionFilter::all();  // All event types
```

### Reading Changed Data

```rust
use sqlitegraph::snapshot::SnapshotId;

while let Ok(PubSubEvent::NodeChanged { node_id, snapshot_id }) = rx.recv() {
    // Read the node at the snapshot when it changed
    let snapshot = SnapshotId::from(snapshot_id);
    match backend.get_node(snapshot, node_id) {
        Ok(node) => println!("Updated node: {:?}", node),
        Err(e) => println!("Node was deleted or error: {}", e),
    }
}
```

---

## Backend-Specific Details

### SQLite Backend

**Implementation:** Simple in-memory Publisher

```rust
// SqliteGraphBackend
pub struct Publisher {
    subscribers: Vec<(u64, Sender<PubSubEvent>, SubscriptionFilter)>,
    next_id: AtomicU64,
}
```

**Events emitted:**
- `insert_node()` → `NodeChanged`
- `insert_edge()` → `EdgeChanged`

**Note:** No KV events (SQLite backend doesn't emit KV change events as KV is SQL-based).

### V3 Backend (Lazy Initialization)

**Implementation:** Publisher only created when first subscriber connects

```rust
pub struct V3Backend {
    publisher: RwLock<Option<Publisher>>,  // Lazy
}

fn subscribe(&self, filter: SubscriptionFilter) -> Result<...> {
    // Initialize publisher on first subscribe
    if self.publisher.read().is_none() {
        *self.publisher.write() = Some(Publisher::new());
    }
    // ... subscribe logic
}
```

**Inspection:**
```rust
let backend = V3Backend::create("data.graph")?;
assert!(!backend.is_pubsub_initialized());  // false

let (sub_id, rx) = backend.subscribe(SubscriptionFilter::all())?;
assert!(backend.is_pubsub_initialized());   // true
```

**Events emitted:**
- `insert_node()` → `NodeChanged`
- `insert_edge()` → `EdgeChanged`
- `kv_set()` / `kv_delete()` → `KVChanged`

### V2 Backend

**Implementation:** Always-allocated Publisher (not lazy)

**Note:** V2 is deprecated. Use V3 for new projects.

---

## Subscription Filtering

### SubscriptionFilter Structure

```rust
pub struct SubscriptionFilter {
    /// Subscribe to NodeChanged events
    pub node_changes: bool,
    
    /// Subscribe to EdgeChanged events
    pub edge_changes: bool,
    
    /// Subscribe to KVChanged events
    pub kv_changes: bool,
    
    /// Subscribe to SnapshotCommitted events
    pub snapshot_commits: bool,
}

impl SubscriptionFilter {
    pub fn all() -> Self {
        Self {
            node_changes: true,
            edge_changes: true,
            kv_changes: true,
            snapshot_commits: true,
        }
    }
}
```

### Filter Matching

```rust
impl SubscriptionFilter {
    pub fn should_send(&self, event: &PubSubEvent) -> bool {
        match event {
            PubSubEvent::NodeChanged { .. } => self.node_changes,
            PubSubEvent::EdgeChanged { .. } => self.edge_changes,
            PubSubEvent::KVChanged { .. } => self.kv_changes,
            PubSubEvent::SnapshotCommitted { .. } => self.snapshot_commits,
        }
    }
}
```

---

## Best Practices

### 1. Handle Channel Closure

```rust
while let Ok(event) = rx.recv() {
    // Process event
}
// Channel closed - subscriber was dropped or backend shut down
```

### 2. Don't Block the Emitter

```rust
// BAD: Slow processing blocks insert_node() return
while let Ok(event) = rx.recv() {
    expensive_operation(event);  // Blocks emitter!
}

// GOOD: Spawn separate thread for processing
let (tx, rx) = mpsc::channel();
std::thread::spawn(move || {
    while let Ok(event) = rx.recv() {
        expensive_operation(event);
    }
});

// Quick forward from pub/sub channel
while let Ok(event) = backend_rx.recv() {
    let _ = tx.send(event);  // Non-blocking, may drop if full
}
```

### 3. Unsubscribe When Done

```rust
let (sub_id, rx) = backend.subscribe(filter)?;

// ... use subscription ...

drop(rx);  // Drop receiver first
backend.unsubscribe(sub_id)?;  // Then unsubscribe
```

### 4. Don't Rely on Delivery

```rust
// Pub/sub is best-effort - events may be dropped
// Always have a way to query current state

// Instead of:
assert!(rx.recv().is_ok());  // May fail!

// Do:
if let Ok(event) = rx.recv_timeout(Duration::from_secs(1)) {
    // Process event
} else {
    // Fallback: query current state
    let current = backend.get_node(snapshot, node_id)?;
}
```

---

## Testing

```bash
# SQLite Pub/Sub tests
cargo test --lib backend::sqlite::pubsub_tests

# V3 Pub/Sub tests  
cargo test --features native-v3 --lib lazy_init_tests

# All 12 tests covering:
# - Subscribe/unsubscribe
# - Node creation events
# - Edge creation events
# - Filtered subscriptions
# - Multiple subscribers
# - No events after unsubscribe
```

---

## Limitations

1. **In-process only** - No network/IPC support
2. **Best-effort delivery** - Events may be dropped if channel full
3. **No persistence** - Events lost if process crashes
4. **No replay** - Subscribe after event = won't receive it
5. **SQLite backend** - No KV change events (KV is SQL-based)

---

## See Also

- [KV Store Architecture](kv-store-architecture.md) - KV change events
- [Architecture](../../ARCHITECTURE.md) - Backend comparison
- [API Reference](../../API.md) - Pub/Sub API
