# Pub/Sub Comparison: sqlitegraph vs Redis

**Last Updated:** 2026-02-03
**Purpose:** Identify potential improvements to sqlitegraph pub/sub by comparing with Redis

---

## Executive Summary

sqlitegraph provides a **minimal, in-process, best-effort** pub/sub system designed for graph event notification. Redis provides a **feature-rich, networked, distributed** pub/sub system with persistence options.

| Feature | sqlitegraph | Redis Pub/Sub | Redis Streams |
|---------|-------------|---------------|---------------|
| Delivery Model | Best-effort | Fire-and-forget | Persistent |
| Networking | In-process only | TCP/IP | TCP/IP |
| Message Persistence | None | None | Configurable |
| Consumer Groups | No | No | Yes |
| Pattern Matching | No | Yes (glob) | No |
| Keyspace Events | Limited | Yes | N/A |
| Backpressure | Drop messages | Drop messages | Block/claim |
| Replay | No | No | Yes |
| Ordering | Per-commit | Per-channel | Per-stream |

---

## Feature-by-Feature Comparison

### 1. Delivery Model

#### sqlitegraph
- **Best-effort delivery** via `mpsc::channel`
- Events dropped if channel is full or receiver gone
- Commit path never blocks on slow subscribers
- Source: `publisher.rs:170` - `let _ = sender.send(event.clone())`

#### Redis Pub/Sub
- **Fire-and-forget** model
- Messages dropped if no active subscribers
- No persistence or delivery guarantees
- Source: [Redis Pub/Sub Documentation](https://redis.io/docs/latest/develop/pubsub/)

#### Redis Streams
- **Persistent** log structure
- Messages survive disconnections
- Configurable retention (MAXLEN)
- Consumer groups with ACK tracking

**Verdict:** sqlitegraph's best-effort model is appropriate for its use case (graph events), but Redis Streams offers a more robust alternative for persistent messaging.

---

### 2. Networking & Distribution

#### sqlitegraph
```rust
// In-process ONLY - no IPC, no networking
pub struct Publisher {
    senders: Arc<Mutex<Vec<(SubscriberId, Sender<PubSubEvent>, SubscriptionFilter)>>>,
    // Sender is std::sync::mpsc::Sender - in-process only
}
```

#### Redis Pub/Sub
```bash
# Network-capable - subscribers can be remote
redis-clisubscribe my_channel
# Other process:
redis-cli PUBLISH my_channel "hello"
```

#### Redis Streams
```bash
# Same network capability
redis-cli XADD mystream * sensor_id 1 temp 72.5
redis-cli XREAD STREAMS mystream $
```

**Verdict:** sqlitegraph is intentionally single-process. Adding IPC/networking is a **significant feature addition** beyond current scope.

---

### 3. Message Persistence & Replay

#### sqlitegraph
- **No persistence** - events exist only in-flight
- No replay capability
- If you miss an event (subscribed late, channel full), it's gone forever

#### Redis Pub/Sub
- **No persistence** - same as sqlitegraph
- Subscribers only receive messages published **after** subscription

#### Redis Streams
```bash
# Full replay capability
XADD mystream * field value
XREAD STREAMS mystream 0        # Read from beginning
XREAD STREAMS mystream $        # Read new messages only
XREAD COUNT 10 BLOCK 5000 STREAMS mystream $  # Blocking read
```

**Verdict:** For event replay and durable subscriptions, **Redis Streams is the gold standard**. sqlitegraph could add persistence via:
1. Event log table in SQLite
2. WAL-based event replay
3. Snapshot-based queries (current approach - consumers use snapshot_id)

---

### 4. Pattern Matching

#### sqlitegraph
```rust
// No pattern matching - exact ID matches only
pub struct SubscriptionFilter {
    pub node_ids: Option<Vec<i64>>,     // Exact IDs only
    pub edge_ids: Option<Vec<i64>>,     // Exact IDs only
    pub key_hashes: Option<Vec<u64>>,   // Exact hashes only
    pub event_types: Option<Vec<PubSubEventType>>,
}
```

#### Redis Pub/Sub
```bash
# Pattern subscriptions with glob patterns
PSUBSCRIBE news.*          # Match news.tech, news.sports, etc.
PSUBSCRIBE users:*:events  # Match users:123:events, users:456:events
```

**Verdict:** sqlitegraph could benefit from **kind-based pattern matching**:
```rust
// Potential enhancement
pub struct PatternFilter {
    pub kinds: Option<Vec<String>>,  // Match nodes with kind="user:*"
    pub names: Option<Vec<String>>,  // Match nodes with name="agent:*"
}
```

---

### 5. Consumer Groups & Load Balancing

#### sqlitegraph
- **No consumer groups** - each subscriber receives every matched event
- Fan-out model (one publisher → N subscribers)
- No coordination between consumers

#### Redis Streams
```bash
# Consumer groups enable competing consumers
XGROUP CREATE mystream mygroup 0
XREADGROUP GROUP mygroup consumer1 STREAMS mystream >
XACK mystream mygroup message_id  # Acknowledge processing
```

**Verdict:** For distributed task processing, **consumer groups are essential**. sqlitegraph doesn't need this for its graph notification use case.

---

### 6. Backpressure Handling

#### sqlitegraph
```rust
// Drop message if channel full - never blocks
let _ = sender.send(event.clone());
```
- Pro: Commit path never blocks
- Con: Events can be silently dropped

#### Redis Pub/Sub
- Same behavior - drops if no subscribers

#### Redis Streams
- **Blocks** on XREAD with BLOCK
- **Pending entries list** for unacknowledged messages
- **XCLAIM** for transferring ownership

**Verdict:** sqlitegraph's approach is correct for its use case (database commits must never block on subscribers).

---

### 7. Keyspace Notifications

#### sqlitegraph
```rust
// Limited to Node/Edge/KVChanged events
pub enum PubSubEvent {
    NodeChanged { node_id, snapshot_id },
    EdgeChanged { edge_id, snapshot_id },
    KVChanged { key_hash, snapshot_id },
    SnapshotCommitted { snapshot_id },
}
// Note: key_hash only - no key name (privacy design)
```

#### Redis
```bash
# Rich keyspace events
CONFIG SET notify-keyspace-events KEA
# K=Keyspace commands, E=Key events, A=glob-*, $=string commands
SUBSCRIBE __keyspace@0__:mykey    # Events on specific key
PSUBSCRIBE __keyevent@0__:del     # All DEL events
```

**Verdict:** sqlitegraph has equivalent functionality but **less granular** (no operation-specific filtering like "only deletions").

---

## Recommendations for sqlitegraph Improvements

### Priority 1: High Value, Low Complexity

#### 1.1 KV Prefix Scanning
**Problem:** Cannot query messages by recipient without prefix scanning.
**Impact:** Agent messaging requires manual KV index management.

**Proposed Solution:**
```rust
// Add to NativeGraphBackend
pub fn kv_prefix_scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    // SELECT key, value FROM kv WHERE key LIKE 'prefix%'
}

// Use case: Get all messages for agent-123
let messages = graph.kv_prefix_scan(b"agent_to:agent-123:")?;
```

**Redis Equivalent:**
```bash
KEYS agent_to:agent-123:*  # Or better: SCAN with MATCH
```

---

#### 1.2 Query by Name/Kind
**Problem:** Cannot find nodes by `kind` or `name` without full scan.

**Proposed Solution:**
```rust
// Add to NativeGraphBackend
pub fn query_nodes_by_kind(&self, kind: &str) -> Result<Vec<Node>> {
    // SELECT * FROM nodes WHERE kind = ?
}

pub fn query_nodes_by_name_pattern(&self, pattern: &str) -> Result<Vec<Node>> {
    // SELECT * FROM nodes WHERE name LIKE ?
}
```

**Rationale:** Enables pub/sub pattern subscriptions like `kind="agent:*"`.

---

### Priority 2: Medium Value, Medium Complexity

#### 2.1 Event Persistence (WAL Replay)
**Problem:** No event replay for subscribers that were offline.

**Proposed Solution:**
```rust
// Event log table
CREATE TABLE pubsub_events (
    seq_id INTEGER PRIMARY KEY,
    event_type TEXT,
    payload_json TEXT,
    created_at INTEGER
);

// Replay API
pub fn replay_events_since(&self, seq_id: u64) -> Result<Vec<PubSubEvent>>;
```

**Redis Equivalent:** Redis Streams (XREAD from specific ID).

---

#### 2.2 Pattern Subscription Filters
**Problem:** Cannot subscribe to "all agents" or "all nodes with kind=user:*".

**Proposed Solution:**
```rust
pub struct SubscriptionFilter {
    // Existing fields...
    pub kind_patterns: Option<Vec<String>>,  // "agent:*", "user:*"
    pub name_patterns: Option<Vec<String>>,  // "msg_index:*"
}

impl SubscriptionFilter {
    pub fn matches_kind(&self, kind: &str) -> bool {
        if let Some(ref patterns) = self.kind_patterns {
            patterns.iter().any(|p| pattern_matches(p, kind))
        } else {
            true
        }
    }
}
```

---

### Priority 3: Lower Priority (Significant Work)

#### 3.1 Consumer Groups
**Use Case:** Distributed task processing across workers.

**Complexity:** Requires:
- Persistent state (consumer group membership)
- ACK/NACK protocol
- Pending message tracking
- Claim/recovery mechanism

**Recommendation:** Use Redis Streams or message queue (RabbitMQ, Kafka) for this use case. Keep sqlitegraph focused on graph events.

---

#### 3.2 IPC / Networking
**Use Case:** Cross-process event delivery.

**Complexity:** Requires:
- Network protocol design
- Authentication/security
- Reconnection handling
- Serialization across processes

**Recommendation:** Use existing pub/sub systems (Redis, NATS, ZeroMQ) for cross-process communication. Keep sqlitegraph in-process.

---

## Conclusion: sqlitegraph's Design Philosophy

sqlitegraph pub/sub is **intentionally minimal**:

1. **In-process:** No networking complexity
2. **Best-effort:** No blocking on slow subscribers
3. **ID-only:** Events carry identifiers, not data (snapshot-based reads)
4. **Commit-triggered:** Events emitted only on transaction commit

This design is **correct for its use case**: real-time notification of graph changes within a single process.

**What Redis does better:**
- Pattern subscriptions (glob matching)
- Persistent messaging (Streams)
- Consumer groups (load balancing)
- Cross-process delivery

**What sqlitegraph does better:**
- Snapshot-based consistency (snapshot_id ensures consistent reads)
- Zero network overhead
- Simplicity (no external dependencies)

**Recommended improvements:**
1. KV prefix scanning (enables message queues)
2. Query by name/kind (enables pattern subscriptions)
3. Optional event log (for replay, if needed)

---

## Sources

- [Redis Pub/Sub Documentation](https://redis.io/docs/latest/develop/pubsub/)
- [Redis Keyspace Notifications](https://redis.io/docs/latest/develop/pubsub/keyspace-notifications/)
- [Redis Streams vs Pub/Sub](https://oneuptime.com/blog/post/2026-01-21-redis-streams-vs-pubsub/view)
- sqlitegraph source code:
  - `sqlitegraph/src/backend/native/v2/pubsub/event.rs`
  - `sqlitegraph/src/backend/native/v2/pubsub/subscriber.rs`
  - `sqlitegraph/src/backend/native/v2/pubsub/publisher.rs`
