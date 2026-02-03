# Query API Enhancements Guide

**Last Updated:** 2026-02-03
**Version:** v1.4.2

This guide explains the query API enhancements introduced in v1.4.0 for efficient data retrieval and pattern-based subscriptions.

---

## Overview

The query API enhancements provide efficient ways to query data without full graph scans:

| API | Purpose | Backend |
|-----|---------|---------|
| `kv_prefix_scan()` | Scan KV store by key prefix | Native V2, SQLite |
| `query_nodes_by_kind()` | Find all nodes of a given kind | Native V2, SQLite |
| `query_nodes_by_name_pattern()` | Find nodes matching glob pattern | Native V2, SQLite |

These APIs support the pub/sub pattern subscription feature (e.g., subscribe to all nodes with `kind="agent:*"`).

---

## Module Structure

```
src/backend/
├── backend.rs           # GraphBackend trait with query methods
├── native/
│   ├── pattern.rs       # Glob pattern matching utility
│   └── graph_backend.rs # Native V2 query implementations
└── sqlite/
    └── graph_backend.rs # SQLite query implementations

src/backend/native/v2/kv_store/
├── types.rs            # KvValue enum
├── store.rs            # KvStore with prefix_scan()
└── snapshot_tests.rs   # Query API tests
```

---

## KV Prefix Scan

### Purpose

Efficiently enumerate all KV entries matching a key prefix. Useful for:
- **Secondary indexes**: Store index entries as `prefix:key` → `value`
- **Namespaced data**: Organize KV entries with prefixes like `agent:123:metadata`
- **Range queries**: Find all keys in a specific range

### API Signature

```rust
// GraphBackend trait
fn kv_prefix_scan(
    &self,
    snapshot_id: SnapshotId,
    prefix: &[u8],
) -> Result<Vec<(Vec<u8>, KvValue)>, SqliteGraphError>
```

### Native V2 Implementation

Located in `src/backend/native/v2/kv_store/store.rs`:

```rust
impl KvStore {
    pub fn prefix_scan(
        &self,
        snapshot_id: SnapshotId,
        prefix: &[u8],
    ) -> Result<Vec<(Vec<u8>, KvValue)>, KvStoreError> {
        let store = self.data.read();

        let mut results = Vec::new();

        // Iterate through all entries
        for (key, entry) in store.data.iter() {
            // Filter by prefix
            if key.starts_with(prefix) {
                // Check TTL (filter expired entries)
                if let Some(expiry) = entry.expiry {
                    if expiry < snapshot_id {
                        continue; // Skip expired
                    }
                }
                results.push((key.clone(), entry.value.clone()));
            }
        }

        // Sort results lexicographically
        results.sort_by_key(|(k, _)| k.clone());

        Ok(results)
    }
}
```

### SQLite Implementation

```rust
// Uses LIKE query for prefix matching
fn kv_prefix_scan(
    &self,
    _snapshot_id: SnapshotId,
    prefix: &[u8],
) -> Result<Vec<(Vec<u8>, KvValue)>, SqliteGraphError> {
    let prefix_str = String::from_utf8_lossy(prefix);
    let pattern = format!("{}%", prefix_str); // SQLite LIKE pattern

    let query = "SELECT key, value FROM kv_store WHERE key LIKE ? ORDER BY key";

    // Execute query and deserialize results...
}
```

### Usage Example

```rust
use sqlitegraph::{GraphConfig, open_graph};

let cfg = GraphConfig::native();
let graph = open_graph("graph.db", &cfg)?;
let snapshot = graph.snapshot()?;

// Scan for all agent messages
let prefix = b"agent_to:agent-123:";
let messages = graph.kv_prefix_scan(snapshot.id(), prefix)?;

for (key, value) in messages {
    println!("Key: {:?}, Value: {:?}", key, value);
}
```

---

## Query Nodes by Kind

### Purpose

Find all nodes with a given kind (e.g., all "Class" nodes, all "Function" nodes). Useful for:
- **Type-based queries**: Get all entities of a specific type
- **Agent discovery**: Find all agents without maintaining external ID tracking
- **Schema queries**: Understand what kinds of nodes exist

### API Signature

```rust
fn query_nodes_by_kind(
    &self,
    snapshot_id: SnapshotId,
    kind: &str,
) -> Result<Vec<i64>, SqliteGraphError>
```

### Native V2 Implementation

Located in `src/backend/native/graph_backend.rs`:

```rust
fn query_nodes_by_kind(
    &self,
    _snapshot_id: SnapshotId,
    kind: &str,
) -> Result<Vec<i64>, SqliteGraphError> {
    self.with_graph_file(|graph_file| {
        let header = graph_file.header();
        let node_count = header.node_count as i64;

        let mut node_store = NodeStore::new(graph_file);
        let mut results = Vec::new();

        // O(N) scan - acceptable for MVP
        // Future optimization: add kind index
        for node_id in 1..=node_count {
            match node_store.read_node(node_id as NativeNodeId) {
                Ok(record) => {
                    if record.kind == kind {
                        results.push(node_id);
                    }
                }
                Err(_) => continue, // Skip unreadable nodes
            }
        }

        results.sort_unstable();
        Ok(results)
    })
}
```

### Performance Notes

| Backend | Complexity | Notes |
|---------|-----------|-------|
| Native V2 | O(N) where N = node_count | Linear scan through node slots |
| SQLite | O(N) but with index potential | Can add index on `kind` column |

### Usage Example

```rust
// Find all Class nodes in the graph
let class_node_ids = graph.query_nodes_by_kind(snapshot.id(), "Class")?;

for node_id in class_node_ids {
    let node = graph.get_node(snapshot.id(), node_id)?;
    println!("Class: {}", node.name);
}
```

---

## Query Nodes by Name Pattern

### Purpose

Find nodes matching a glob pattern on their name. Supports:
- **Wildcard matching**: `*` matches any sequence
- **Single character**: `?` matches exactly one character
- **Literal matching**: `\*` and `\?` for literal asterisk/question mark

### API Signature

```rust
fn query_nodes_by_name_pattern(
    &self,
    snapshot_id: SnapshotId,
    pattern: &str,
) -> Result<Vec<i64>, SqliteGraphError>
```

### Pattern Syntax

| Pattern | Matches | Does Not Match |
|---------|---------|----------------|
| `agent-*` | `agent-123`, `agent-abc` | `user-123`, `agent-123-extra` |
| `user:???` | `user:abc`, `user:123` | `user:ab`, `user:abcd` |
| `*-test` | `agent-test`, `123-test` | `agent-test-extra` |
| `*` | Anything | (nothing) |
| `literal\*` | `literal*` | `literalX` |

### Implementation

Uses `glob_matches()` from `src/backend/native/pattern.rs`:

```rust
fn query_nodes_by_name_pattern(
    &self,
    _snapshot_id: SnapshotId,
    pattern: &str,
) -> Result<Vec<i64>, SqliteGraphError> {
    use crate::backend::native::pattern::glob_matches;

    self.with_graph_file(|graph_file| {
        let header = graph_file.header();
        let node_count = header.node_count as i64;

        let mut node_store = NodeStore::new(graph_file);
        let mut results = Vec::new();

        for node_id in 1..=node_count {
            match node_store.read_node(node_id as NativeNodeId) {
                Ok(record) => {
                    if glob_matches(pattern, &record.name) {
                        results.push(node_id);
                    }
                }
                Err(_) => continue,
            }
        }

        results.sort_unstable();
        Ok(results)
    })
}
```

### Usage Example

```rust
// Find all nodes with names like "agent-123", "agent-456", etc.
let agent_node_ids = graph.query_nodes_by_name_pattern(snapshot.id(), "agent-*")?;

// Find all nodes with exactly 3-character names
let short_names = graph.query_nodes_by_name_pattern(snapshot.id(), "???")?;

// Find all nodes with name starting with "test_func_"
let test_functions = graph.query_nodes_by_name_pattern(snapshot.id(), "test_func_*")?;
```

---

## Glob Pattern Implementation

### Algorithm

Located in `src/backend/native/pattern.rs`:

```rust
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    let mut pattern_chars = pattern.chars().peekable();
    let mut text_chars = text.chars().peekable();

    while let Some(pc) = pattern_chars.next() {
        match pc {
            '*' => {
                // Greedy match: consume all until next pattern char matches
                let next_pattern: String = pattern_chars.collect();
                if next_pattern.is_empty() {
                    return true; // Trailing * matches everything
                }
                // Try to match rest of pattern at each position
                while text_chars.peek().is_some() {
                    let remaining: String = text_chars.clone().collect();
                    if glob_matches_impl(&next_pattern, &remaining) {
                        return true;
                    }
                    text_chars.next();
                }
                return glob_matches_impl(&next_pattern, "");
            }
            '?' => {
                if text_chars.next().is_none() {
                    return false;
                }
            }
            '\\' => {
                // Escaped character
                if let Some(literal) = pattern_chars.next() {
                    if text_chars.next() != Some(literal) {
                        return false;
                    }
                }
            }
            c => {
                if text_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    // All pattern consumed - check if all text consumed
    text_chars.next().is_none()
}
```

### Complexity

| Pattern Type | Complexity |
|--------------|------------|
| No wildcards | O(n) where n = text length |
| With `*` | O(n × m) worst case (n = text, m = pattern) |
| With `?` | O(n) |

---

## Pattern-Based Subscriptions

### Combining Query API with Pub/Sub

The query APIs enable pattern-based pub/sub subscriptions:

```rust
use sqlitegraph::{GraphConfig, open_graph};
use sqlitegraph::backend::SubscriptionFilter;

let cfg = GraphConfig::native();
let graph = open_graph("graph.db", &cfg)?;

// Subscribe to all agents using kind pattern
let filter = SubscriptionFilter {
    kind_patterns: vec!["agent:*".to_string()],
    ..Default::default()
};

let (sub_id, rx) = graph.subscribe(filter)?;

// In a separate task
while let Ok(event) = rx.recv() {
    match event {
        PubSubEvent::NodeChanged { node_id, snapshot_id } => {
            // Read the actual node data
            let node = graph.get_node(snapshot_id, node_id)?;
            println!("Agent changed: {:?}", node.name);
        }
        _ => {}
    }
}
```

### Publisher-Side Pattern Matching

Located in `src/backend/native/v2/pubsub/subscriber.rs`:

```rust
impl SubscriptionFilter {
    pub fn matches(&self, event: &PubSubEvent, metadata: Option<&NodeMetadata>) -> bool {
        // ... type and ID checking ...

        // Check pattern filters (requires metadata)
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

---

## Testing

### Test Template

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kv_prefix_scan_empty() {
        let store = KvStore::new();
        let snapshot_id = 1;

        let results = store.prefix_scan(snapshot_id, b"agent:");
        assert!(results.is_empty());
    }

    #[test]
    fn test_kv_prefix_scan_multiple_matches() {
        let mut store = KvStore::new();

        // Insert test data
        store.set(b"agent:123:state".to_vec(), KvValue::String("active".into()), None).unwrap();
        store.set(b"agent:123:metadata".to_vec(), KvValue::String("worker".into()), None).unwrap();
        store.set(b"agent:456:state".to_vec(), KvValue::String("idle".into()), None).unwrap();
        store.set(b"other:key".to_vec(), KvValue::Integer(0), None).unwrap();

        let snapshot_id = 1;
        let results = store.prefix_scan(snapshot_id, b"agent:123:");

        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|(k, _)| k == b"agent:123:state"));
        assert!(results.iter().any(|(k, _)| k == b"agent:123:metadata"));
    }

    #[test]
    fn test_query_nodes_by_kind() {
        let (backend, node_ids, _temp_dir) = create_test_graph().unwrap();

        let snapshot = backend.snapshot().unwrap();
        let class_nodes = backend.query_nodes_by_kind(snapshot.id(), "Class").unwrap();

        assert!(!class_nodes.is_empty());
    }

    #[test]
    fn test_query_nodes_by_name_pattern() {
        let (backend, node_ids, _temp_dir) = create_test_graph().unwrap();

        let snapshot = backend.snapshot().unwrap();
        let agent_nodes = backend.query_nodes_by_name_pattern(snapshot.id(), "agent-*").unwrap();

        assert!(!agent_nodes.is_empty());
    }

    #[test]
    fn test_glob_matches() {
        use crate::backend::native::pattern::glob_matches;

        assert!(glob_matches("agent-*", "agent-123"));
        assert!(glob_matches("user:???", "user:abc"));
        assert!(!glob_matches("agent-*", "user-123"));
    }
}
```

---

## Adding New Query Methods

### Step 1: Add to GraphBackend Trait

Update `src/backend/backend.rs`:

```rust
pub trait GraphBackend {
    // ... existing methods

    /// Your new query method
    fn your_query_method(
        &self,
        snapshot_id: SnapshotId,
        param: YourParamType,
    ) -> Result<YourReturnType, SqliteGraphError>;
}
```

### Step 2: Implement for Native V2

Update `src/backend/native/graph_backend.rs`:

```rust
fn your_query_method(
    &self,
    snapshot_id: SnapshotId,
    param: YourParamType,
) -> Result<YourReturnType, SqliteGraphError> {
    self.with_graph_file(|graph_file| {
        // Your implementation
        Ok(results)
    })
}
```

### Step 3: Implement for SQLite

Update `src/backend/sqlite/graph_backend.rs`:

```rust
fn your_query_method(
    &self,
    snapshot_id: SnapshotId,
    param: YourParamType,
) -> Result<YourReturnType, SqliteGraphError> {
    let conn = self.conn.lock().unwrap();

    let query = "SELECT ... FROM nodes WHERE ...";
    let mut stmt = conn.prepare(query)?;

    // Execute and return results
    Ok(results)
}
```

### Step 4: Add CLI Command (Optional)

Update `sqlitegraph-cli/src/main.rs`:

```rust
YourQuery {
    #[arg(long)]
    param: String,
} => {
    let client = BackendClient::new(args.backend, args.db)?;
    let snapshot = client.snapshot()?;

    let results = client.your_query_method(snapshot.id(), &args.param)?;

    for item in results {
        println!("{:?}", item);
    }

    Ok(())
}
```

---

## Performance Optimization Opportunities

### Current Limitations

| API | Current Complexity | Potential Optimization |
|-----|-------------------|----------------------|
| `kv_prefix_scan` | O(K) where K = total keys | Add prefix index (trie) |
| `query_nodes_by_kind` | O(N) linear scan | Add kind index (hash map) |
| `query_nodes_by_name_pattern` | O(N) linear scan | Add name index (with pattern support) |

### Future Index Structures

```rust
// Potential kind index (not implemented)
struct KindIndex {
    by_kind: HashMap<String, Vec<i64>>, // kind -> node_ids
}

// Potential name trie for patterns (not implemented)
struct NameTrie {
    root: TrieNode,
}

enum TrieNode {
    Branch {
        children: HashMap<char, TrieNode>,
        wildcard: Vec<i64>, // Nodes matching * at this level
    },
    Leaf(Vec<i64>),
}
```

### Adding an Index

If you need to add an index:

1. **Choose index structure** based on query pattern
2. **Update on writes** - maintain index in `insert_node()`, `delete_node()`
3. **Use index in queries** - check index first before scanning
4. **Test thoroughly** - verify index stays consistent

---

## Common Patterns

### Secondary Index with KV Prefix Scan

```rust
// Store inverted index: kind_name -> node_id
let index_key = format!("index:kind:{}:{}", node.kind, node.name);
graph.kv_set(
    index_key.as_bytes(),
    KvValue::Integer(node.id),
    None
)?;

// Query: find all nodes of kind "Class" named "Test*"
let prefix = b"index:kind:Class:test_";
let results = graph.kv_prefix_scan(snapshot.id(), prefix)?;

for (key, value) in results {
    if let KvValue::Integer(node_id) = value {
        let node = graph.get_node(snapshot.id(), node_id)?;
        println!("Found: {:?}", node.name);
    }
}
```

### Agent Messaging Pattern

```rust
// Send message to agent-123
let msg_key = format!("agent_to:agent-123:{}", timestamp);
graph.kv_set(
    msg_key.as_bytes(),
    KvValue::String(message_content),
    None
)?;

// Agent receives messages
let prefix = b"agent_to:agent-123:";
let messages = graph.kv_prefix_scan(snapshot.id(), prefix)?;
```

### Dynamic Entity Discovery

```rust
// Find all agents without maintaining external list
let agent_node_ids = graph.query_nodes_by_kind(snapshot.id(), "agent:worker")?;

// Or by name pattern
let agent_node_ids = graph.query_nodes_by_name_pattern(snapshot.id(), "agent-worker-*")?;

// Then subscribe to their updates
let filter = SubscriptionFilter {
    node_ids: agent_node_ids,
    ..Default::default()
};

let (sub_id, rx) = graph.subscribe(filter)?;
```

---

## References

- **Source:** `src/backend/native/pattern.rs` - Glob pattern matching
- **Source:** `src/backend/native/v2/kv_store/store.rs` - KV prefix scan
- **Source:** `src/backend/native/graph_backend.rs` - Query implementations
- **Tests:** `src/backend/native/v2/kv_store/snapshot_tests.rs` - Query API tests
- **Pub/Sub Guide:** `docs/DEVELOPMENT_GUIDES/pubsub-implementation.md`
- **User Docs:** `MANUAL.md` - User-facing query API documentation

---

## Changelog

| Version | Changes |
|---------|---------|
| v1.4.0 | Initial release of query API enhancements |
| v1.4.2 | Documentation updates, zero compiler warnings |
