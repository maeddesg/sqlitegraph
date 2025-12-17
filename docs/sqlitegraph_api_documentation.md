# SQLiteGraph API Documentation

## Overview
SQLiteGraph provides a unified API for graph database operations with dual backend support (SQLite and Native).

## Core Public API Exports

### Configuration and Factory Functions
- **`BackendKind`** - Runtime backend selection enum
- **`GraphConfig`** - Unified configuration for both backends
- **`NativeConfig`** - Native-specific options
- **`SqliteConfig`** - SQLite-specific options
- **`open_graph()`** - Unified factory function

### Graph Operations
- **`GraphBackend`** trait - Unified trait for backend implementations
- **`insert_node()`, `insert_edge()`** - Single entity/edge insertion
- **`bulk_insert_entities()`, `bulk_insert_edges()`** - Batch operations
- **`neighbors()`** - Direct neighbor queries
- **`bfs()`, `k_hop()`, `shortest_path()`** - Graph traversal algorithms

### Core Types
- **`GraphEntity`** - Graph node/vertex representation
- **`GraphEdge`** - Graph edge/relationship representation
- **`NodeSpec`** - Node specification for insertion operations
- **`EdgeSpec`** - Edge specification for insertion operations
- **`BackendDirection`** - Direction specification for graph traversal operations
- **`NeighborQuery`** - Query configuration for neighbor lookups with optional filtering
- **`SqliteGraphBackend`** - SQLite backend implementation
- **`NativeGraphBackend`** - Native backend implementation

### NodeSpec Structure
```rust
pub struct NodeSpec {
    pub kind: String,                    // Node type/category
    pub name: String,                    // Human-readable name
    pub file_path: Option<String>,       // Optional file path
    pub data: serde_json::Value,          // JSON metadata
}
```

### EdgeSpec Structure
```rust
pub struct EdgeSpec {
    pub from: i64,                       // Source node ID
    pub to: i64,                         // Target node ID
    pub edge_type: String,               // Edge type/relationship
    pub data: serde_json::Value,          // JSON metadata
}
```

### BackendDirection Enum
```rust
pub enum BackendDirection {
    Outgoing,
    Incoming,
}
```

### NeighborQuery Structure
```rust
pub struct NeighborQuery {
    pub direction: BackendDirection,
    pub edge_type: Option<String>,
}

impl Default for NeighborQuery {
    fn default() -> Self {
        Self {
            direction: BackendDirection::Outgoing, // Default direction
            edge_type: None,
        }
    }
}
```

### Usage Examples

#### Creating a Graph
```rust
use sqlitegraph::{open_graph, GraphConfig};

// Native backend (V2 by default)
let config = GraphConfig::native();
let graph = open_graph("my_graph.db", &config)?;

// SQLite backend (default)
let config = GraphConfig::sqlite();
let graph = open_graph("my_graph.db", &config)?;
```

#### Inserting Nodes
```rust
let node_id = graph.insert_node(NodeSpec {
    kind: "Person".to_string(),
    name: "John Doe".to_string(),
    file_path: None,
    data: serde_json::json!({"age": 30, "city": "New York"}),
})?;
```

#### Inserting Edges
```rust
let edge_id = graph.insert_edge(EdgeSpec {
    from: source_node_id,
    to: target_node_id,
    edge_type: "knows".to_string(),
    data: serde_json::json!({"since": 2020}),
})?;
```

#### Querying Neighbors
```rust
// Using default query (outgoing direction, no edge type filter)
let neighbors = graph.neighbors(node_id, Default::default())?;

// Custom query for incoming neighbors of specific edge type
let incoming_friends = graph.neighbors(node_id, NeighborQuery {
    direction: BackendDirection::Incoming,
    edge_type: Some("friend".to_string()),
})?;
```

## Backend-Specific Details

### Native Backend V2 Features
- **V2 is the default** for native backend
- **4096-byte node slots** for efficient storage
- **Clustered adjacency** for edge storage
- **Memory-mapped I/O** for high performance
- **Transaction write-set** for rollback consistency

### Native Backend Constants
- `NODE_SLOT_SIZE: u64 = 4096` - Fixed size for each node slot
- Node slot offset: `node_data_offset + ((node_id - 1) as u64 * 4096)`

## Modules Available for Testing

### Public Test Modules
- `algo` - Algorithm implementations
- `bfs` - BFS algorithm tests
- `cache` - Cache tests
- `dual_runner` - Backend comparison tests
- `fault_injection` - Fault injection utilities (test-only)
- `graph_opt` - Graph optimization tests
- `index` - Index tests
- `multi_hop` - Multi-hop traversal tests
- `query_cache` - Query cache tests
- `schema` - Schema tests

### Internal Modules (Not Public API)
- `api_ergonomics` - Internal utilities
- `client` - CLI client
- `reasoning` - Reasoning engine
- Various backend-specific modules

## Error Handling
- **`SqliteGraphError`** - Comprehensive error type for all operations

## Advanced Features
- **MVCC Snapshots** - `GraphSnapshot` for read isolation
- **Pattern Engine** - `PatternTriple` for pattern matching
- **Recovery Utilities** - Database backup and restore
- **Query Interface** - `GraphQuery` for high-level queries