# Phase 44: Pub/Sub (Minimal, In-Process) - Research

**Researched:** 2026-01-26
**Domain:** Rust in-process event notification with commit-time emission
**Confidence:** HIGH

## Summary

This research covers implementing a minimal in-process pub/sub system for SQLiteGraph, where events are emitted on transaction commit only. The system uses Rust's standard `std::sync::mpsc` channels for in-process message delivery, with no networking, no background threads, and no payload data in events (only IDs).

The standard approach for in-process pub/sub in Rust is:
- **`std::sync::mpsc`** for single-consumer channels (one subscriber per channel)
- **`tokio::sync::broadcast`** for multi-consumer fan-out (if async runtime is available)
- **Crossbeam channels** as an alternative to std (but not needed here)

Since SQLiteGraph is a synchronous embedded database (no async runtime), the recommended approach is `std::sync::mpsc::channel` for simplicity. For multi-subscriber support, we maintain a registry of channels and clone senders.

**Primary recommendation:** Use `std::sync::mpsc` with a subscription registry. Emit events in `V2WALManager::commit_transaction()` after successful commit. Events carry IDs only; consumers read data via graph/KV APIs with the provided `snapshot_id`.

## Standard Stack

### Core

| Component | Choice | Purpose | Why Standard |
|-----------|--------|---------|--------------|
| Channel type | `std::sync::mpsc` | In-process message delivery | Rust standard library, no external dependencies, sufficient for embedded use |
| Sync primitives | `parking_lot::RwLock` | Subscriber registry protection | Already used throughout codebase |
| Event storage | Enum variants | Type-safe event definitions | Standard Rust pattern for typed events |

### Supporting

| Component | Choice | Purpose | When to Use |
|-----------|--------|---------|-------------|
| Subscription ID | `u64` | Unique subscriber identifier | Needed for unsubscribe operations |
| Event filtering | Closure-based `Box<dyn Fn(&PubSubEvent) -> bool>` | Per-subscription event filtering | Allows consumers to receive only relevant events |
| Snapshot ID | `SnapshotId` (existing) | Event versioning | Events include commit LSN for snapshot-aware reads |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `std::sync::mpsc` | `tokio::sync::broadcast` | Requires async runtime; overkill for sync embedded DB |
| Single channel per subscriber | Shared broadcast channel | Broadcast needs async or complex mutex handling; registry pattern is simpler |
| In-event payloads | ID-only events | Payloads duplicate data; consumers already have read APIs |
| Background delivery | Sync delivery on commit | Background threads add complexity; sync is predictable |

**Installation:** No new dependencies needed. Uses:
- `std::sync::mpsc` (standard library)
- `parking_lot` (already in codebase)
- Existing `SnapshotId` type

## Architecture Patterns

### Recommended Project Structure

```
sqlitegraph/src/backend/native/v2/
├── pubsub/
│   ├── mod.rs         # Module exports, public API
│   ├── types.rs       # PubSubEvent enum, SubscriptionFilter, PubSubError
│   ├── publisher.rs   # Publisher with channel registry, emit methods
│   └── tests.rs       # Unit tests for pub/sub operations
```

### Pattern 1: Channel Registry with Subscription Management

**What:** Central registry maintaining `(subscription_id, sender, filter)` tuples. Each subscription gets its own channel sender cloned from a source.

**When to use:** Need to support multiple independent subscribers with different filters.

**Example:**
```rust
// Source: Based on Rust std::sync::mpsc documentation
use std::sync::mpsc::{channel, Sender, Receiver};
use parking_lot::RwLock;
use std::sync::Arc;
use std::collections::HashMap;

struct Publisher {
    subscribers: Arc<RwLock<HashMap<u64, SubscriptionEntry>>>,
    next_subscription_id: Arc<AtomicU64>,
}

struct SubscriptionEntry {
    sender: Sender<PubSubEvent>,
    filter: Option<Box<dyn Fn(&PubSubEvent) -> bool + Send>>,
}

impl Publisher {
    fn subscribe(&self, filter: Option<Box<dyn Fn(&PubSubEvent) -> bool + Send>>)
        -> (u64, Receiver<PubSubEvent>)
    {
        let (tx, rx) = channel();
        let id = self.next_subscription_id.fetch_add(1, Ordering::SeqCst);

        let entry = SubscriptionEntry { sender: tx, filter };
        self.subscribers.write().insert(id, entry);

        (id, rx)
    }

    fn unsubscribe(&self, id: u64) -> bool {
        self.subscribers.write().remove(&id).is_some()
    }

    fn emit(&self, event: PubSubEvent) {
        let mut subs = self.subscribers.write();
        let mut to_remove = Vec::new();

        for (id, sub) in subs.iter() {
            // Apply filter if present
            if let Some(ref filter) = sub.filter {
                if !filter(&event) {
                    continue;
                }
            }

            // Best-effort send; drop receiver if full/closed
            if sub.sender.try_send(event.clone()).is_err() {
                to_remove.push(*id);
            }
        }

        // Clean up dead subscriptions
        for id in to_remove {
            subs.remove(&id);
        }
    }
}
```

### Pattern 2: Event Emission on Commit

**What:** Hook into existing `V2WALManager::commit_transaction()` to emit events after successful commit.

**When to use:** Events must reflect committed state only (not rolled-back transactions).

**Example:**
```rust
// Integration point in V2WALManager::commit_transaction
// After commit_lsn is assigned and delta index populated

// Source: /home/feanor/Projects/sqlitegraph/sqlitegraph/src/backend/native/v2/wal/manager.rs:335-406
pub fn commit_transaction(&self, tx_id: u64) -> NativeResult<()> {
    // ... existing commit logic ...

    let commit_lsn = self.writer.write_record(commit_record)?;

    // Populate delta index (existing code)
    {
        let mut delta_index = self.delta_index.write();
        if let Err(e) = delta_index.apply_commit(records, commit_lsn) {
            eprintln!("Failed to populate delta index: {}", e);
        }
    }

    // NEW: Emit pub/sub events for committed changes
    // Scan transaction records and emit appropriate events
    for record in &records {
        match record {
            V2WALRecord::NodeInsert { node_id, .. } |
            V2WALRecord::NodeUpdate { node_id, .. } => {
                self.publisher.emit(PubSubEvent::NodeChanged {
                    node_id: *node_id,
                    snapshot_id: SnapshotId::from_lsn(commit_lsn),
                });
            }
            V2WALRecord::EdgeInsert { .. } |
            V2WALRecord::EdgeUpdate { .. } => {
                self.publisher.emit(PubSubEvent::EdgeChanged {
                    edge_id: extract_edge_id(record),
                    snapshot_id: SnapshotId::from_lsn(commit_lsn),
                });
            }
            V2WALRecord::KvSet { key, .. } |
            V2WALRecord::KvDelete { key, .. } => {
                self.publisher.emit(PubSubEvent::KVChanged {
                    key_hash: hash_key(key),
                    snapshot_id: SnapshotId::from_lsn(commit_lsn),
                });
            }
            _ => {} // No event for other record types
        }
    }

    // Emit final commit event
    self.publisher.emit(PubSubEvent::SnapshotCommitted {
        snapshot_id: SnapshotId::from_lsn(commit_lsn),
    });

    // ... rest of existing commit logic ...
}
```

### Pattern 3: Best-Effort Delivery

**What:** Use `try_send()` instead of `send()` to avoid blocking commits when receivers are full or dropped.

**When to use:** Event delivery must not slow down or block transaction commits.

**Example:**
```rust
// Source: Rust std::sync::mpsc documentation
// Best-effort: ignore errors, clean up dead receivers

if sub.sender.try_send(event.clone()).is_err() {
    // Receiver dropped or channel full
    // Mark for cleanup but don't fail the commit
    to_remove.push(*id);
}
```

### Anti-Patterns to Avoid

- **Blocking sends:** Using `send()` instead of `try_send()` can block commits if receivers are slow. Commits must remain fast.
- **Event payloads:** Including data in events duplicates the WAL and creates consistency issues. Consumers should read via snapshot-aware APIs.
- **Background threads:** Spawning threads for event delivery adds complexity and shutdown issues. Emit synchronously during commit.
- **Global mutable state:** Using `static mut` for the publisher. Pass via Arc like other components (WAL manager, checkpoint manager).
- **Emitting on rollback:** Events must only be emitted for *committed* changes, not rolled-back transactions.

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Multi-consumer channels | Custom broadcast logic | Registry of mpsc channels | Broadcast requires complex weak reference handling; registry pattern is simpler |
| Thread-safe subscriber list | Arc<Mutex<Vec<...>>> | `parking_lot::RwLock<HashMap>` | parking_lot is faster and already in use |
| Event filtering | String-based pattern matching | Closure-based `Box<dyn Fn>` | Closures are type-safe and compile-time checked |
| Subscription IDs | UUID generation | `AtomicU64` counter | Simpler, sufficient for in-process |

**Key insight:** The pub/sub system is a notification mechanism, not a message queue. Keep it minimal. Let the existing WAL and storage systems handle data delivery.

## Common Pitfalls

### Pitfall 1: Emitting Events Before Commit is Final

**What goes wrong:** Events are emitted during transaction build-up, then rollback happens. Consumers receive notifications for changes that never committed.

**Why it happens:** Placing event emission in `write_transaction_record()` instead of `commit_transaction()`.

**How to avoid:** Only emit events in `commit_transaction()` after the commit record is written and `commit_lsn` is assigned.

**Warning signs:** Tests fail where rollback still triggers events, or events arrive for transactions that were rolled back.

### Pitfall 2: Blocking Commit Delivery

**What goes wrong:** A slow or hung subscriber blocks transaction commits because `sender.send()` blocks waiting for buffer space.

**Why it happens:** Using blocking `send()` instead of `try_send()`.

**How to avoid:** Always use `try_send()` and drop/ignore failed deliveries. Use bounded channels with reasonable buffer sizes.

**Warning signs:** Commit latency spikes when subscribers are added, or commits hang entirely.

### Pitfall 3: Memory Leak from Dead Subscribers

**What goes wrong:** Unsubscribed or dropped subscribers remain in the registry, causing unbounded memory growth and wasted iteration.

**Why it happens:** `try_send()` succeeds but the receiver was never dropped, or failed sends don't clean up the subscription.

**How to avoid:** Track failed sends and remove those subscribers. Also provide explicit `unsubscribe()` API.

**Warning signs:** Memory usage grows linearly with time, subscriber count never decreases.

### Pitfall 4: Including Payloads in Events

**What goes wrong:** Event payloads duplicate WAL data, create serialization overhead, and may become stale if data changes again.

**Why it happens:** Trying to make events "self-contained" so consumers don't need to call read APIs.

**How to avoid:** Events carry IDs only. Consumers use existing graph/KV read APIs with the provided `snapshot_id`.

**Warning signs:** Events contain serialized node/edge data, or `PubSubEvent` enum has large nested structs.

### Pitfall 5: Missing Snapshot ID in Events

**What goes wrong:** Consumers receive event notifications but can't tell what snapshot to read from to see the changed data.

**Why it happens:** Events only include entity IDs without versioning information.

**How to avoid:** Every event must include `snapshot_id: SnapshotId` so consumers know which committed state to query.

**Warning signs:** Consumers need to call `get_node_current()` or similar non-snapshot-aware APIs to read event data.

## Code Examples

Verified patterns from official sources:

### Basic Channel Creation

```rust
// Source: https://doc.rust-lang.org/std/sync/mpsc/index.html
use std::sync::mpsc::{channel, Sender, Receiver};

let (tx, rx): (Sender<PubSubEvent>, Receiver<PubSubEvent>) = channel();

// Clone sender for multiple subscriptions
let tx2 = tx.clone();
```

### Non-Blocking Send

```rust
// Source: https://doc.rust-lang.org/std/sync/mpsc/struct.Sender.html#method.try_send
use std::sync::mpsc::channel;

let (tx, rx) = channel();
match tx.try_send(PubSubEvent::NodeChanged { node_id: 1, snapshot_id }) {
    Ok(()) => println!("Event sent"),
    Err(TrySendError::Full(_)) => eprintln!("Receiver buffer full, event dropped"),
    Err(TrySendError::Disconnected(_)) => eprintln!("Receiver dropped, cleanup needed"),
}
```

### Filtering with Closures

```rust
// Source: Rust closure pattern standard
type EventFilter = Box<dyn Fn(&PubSubEvent) -> bool + Send>;

fn node_specific_filter(node_id: i64) -> EventFilter {
    Box::new(move |event| match event {
        PubSubEvent::NodeChanged { node_id: n, .. } => *n == node_id,
        _ => false,
    })
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Global static event bus | Arc-shared Publisher | Rust 1.0+ | Thread-safe, testable, no global state |
| Unbounded channels | Bounded channels with try_send | Rust 1.0+ | Prevents memory exhaustion from slow consumers |
| Callback-based | Channel-based receivers | Stable | Channels are standard, composable, idiomatic |

**Deprecated/outdated:**
- **Callback-based pub/sub:** Requires function pointers or trait objects, harder to manage lifetimes
- **Global static mutable state:** Not thread-safe, prevents multiple database instances
- **Unbounded channels:** Can cause OOM if consumers are slow; bounded channels are safer

## Open Questions

1. **Should we use tokio for async event delivery?**
   - What we know: SQLiteGraph is synchronous (no async runtime in main codebase)
   - What's unclear: Future plans for async API surface
   - Recommendation: Stick to `std::sync::mpsc` for now. Async can be added later with a separate `async` feature flag.

2. **Channel buffer size?**
   - What we know: Bounded channels prevent memory exhaustion
   - What's unclear: Optimal size for workloads
   - Recommendation: Start with 16-32 slots per channel. Small enough to bound memory, large enough to avoid drops under normal load. Make configurable if needed.

3. **Should we persist subscription registry?**
   - What we know: Subscriptions are in-process only
   - What's unclear: Whether crash recovery should restore subscriptions
   - Recommendation: No. Subscriptions are transient. Applications re-subscribe after restart. This matches SQLite's behavior (no registered handlers persist across connections).

## Sources

### Primary (HIGH confidence)

- [std::sync::mpsc - Rust Standard Library](https://doc.rust-lang.org/std/sync/mpsc/) - Core channel API for in-process messaging
- [tokio::sync::broadcast - Tokio Documentation](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) - Reference for multi-consumer patterns (even though we use std)
- [parking_lot::RwLock - crates.io](https://docs.rs/parking_lot/latest/parking_lot/) - Lock primitives already in use

### Secondary (MEDIUM confidence)

- [Building a Pub/Sub Server in Rust using Tokio and Channels](https://medium.com/@enravishjeni411/building-a-pub-sub-server-in-rust-using-tokio-and-channels-d27653096522) - Confirms broadcast for in-process, network only when multi-node
- [Best strategy for pub-sub between async tasks? : r/rust](https://www.reddit.com/r/rust/comments/1i2pz2g/best_strategy_for_pubsub_between_async_tasks/) - Community consensus on channel types
- [Avoiding Over-Reliance on mpsc channels in Rust](https://blog.digital-horror.com/blog/how-to-over-reliance-on-mpsc/) - Performance characteristics and alternatives

### Tertiary (LOW confidence)

- [Good server architecture for publishing/subscribing in Rust](https://stackoverflow.com/questions/70770686/good-server-architecture-for-publishing-subscribing-in-rust) - General patterns discussion
- [How to use pub/sub pattern in Event Sourcing & CQRS](https://stackoverflow.com/questions/44588341/how-to-use-pub-sub-pattern-in-event-sourcing-cqrs) - High-level architectural patterns

## Codebase References

### Files Analyzed

| File | Path | Relevance |
|------|------|-----------|
| WAL Manager | `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/backend/native/v2/wal/manager.rs` | Commit integration point (lines 335-406) |
| WAL Record Types | `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/backend/native/v2/wal/record.rs` | Event source data (NodeInsert, EdgeInsert, KvSet, etc.) |
| KV Store Module | `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/backend/native/v2/kv_store/` | Pattern for new module (types, store, tests structure) |
| GraphBackend Trait | `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/backend.rs` | Public API integration (subscribe/unsubscribe methods) |

### Existing Patterns to Follow

1. **Module structure:** Follow `kv_store/` layout (types.rs, store.rs, tests.rs, mod.rs)
2. **Arc-sharing:** WAL manager, checkpoint manager both use `Arc<>` for shared access
3. **parking_lot:** Use `RwLock<HashMap<>>` pattern from `V2WALManager.active_transactions`
4. **Error handling:** Use `NativeBackendError` for pub/sub errors
5. **Feature flag:** Use `#[cfg(feature = "native-v2")]` since pub/sub is Native V2 only

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Rust std::sync::mpsc is well-documented and stable
- Architecture: HIGH - Patterns verified against existing codebase (WAL manager, KV store)
- Pitfalls: HIGH - Common issues documented in Rust channel literature and best practices
- Integration: HIGH - Commit hook point identified (V2WALManager::commit_transaction)

**Research date:** 2026-01-26
**Valid until:** 90 days (stable domain, std library API won't change)
