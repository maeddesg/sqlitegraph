# Phase 14 – Step 22.4: API Extraction Table

## Benchmarks API Usage

### bfs.rs
| Function | Exact Signature | File Location | Exists? | Notes |
|----------|----------------|---------------|---------|-------|
| `open_graph()` | `fn open_graph(path: &Path, config: GraphConfig) -> Result<GraphHandle>` | `sqlitegraph/src/lib.rs` | ✅ | Public API entry point |
| `insert_node()` | `fn insert_node(&mut self, label: &str) -> Result<NodeId>` | `sqlitegraph/src/lib.rs` | ✅ | Public API |
| `insert_edge()` | `fn insert_edge(&mut self, from: NodeId, to: NodeId, label: &str) -> Result<EdgeId>` | `sqlitegraph/src/lib.rs` | ✅ | Public API |
| `bfs()` | `fn bfs(&self, start: NodeId) -> Result<Vec<NodeId>>` | `sqlitegraph/src/bfs.rs` | ✅ | BFS algorithm |
| `neighbors()` | `fn neighbors(&self, node: NodeId) -> Result<Vec<NodeId>>` | `sqlitegraph/src/lib.rs` | ✅ | Graph traversal |

### k_hop.rs
| Function | Exact Signature | File Location | Exists? | Notes |
|----------|----------------|---------------|---------|-------|
| `open_graph()` | `fn open_graph(path: &Path, config: GraphConfig) -> Result<GraphHandle>` | `sqlitegraph/src/lib.rs` | ✅ | Public API entry point |
| `insert_node()` | `fn insert_node(&mut self, label: &str) -> Result<NodeId>` | `sqlitegraph/src/lib.rs` | ✅ | Public API |
| `insert_edge()` | `fn insert_edge(&mut self, from: NodeId, to: NodeId, label: &str) -> Result<EdgeId>` | `sqlitegraph/src/lib.rs` | ✅ | Public API |
| `k_hop()` | `fn k_hop(&self, start: NodeId, k: usize) -> Result<Vec<NodeId>>` | `sqlitegraph/src/multi_hop.rs` | ✅ | Multi-hop algorithm |

### native_disk_io.rs
| Function | Exact Signature | File Location | Exists? | Notes |
|----------|----------------|---------------|---------|-------|
| `open_graph()` | `fn open_graph(path: &Path, config: GraphConfig) -> Result<GraphHandle>` | `sqlitegraph/src/lib.rs` | ✅ | Public API entry point |
| `insert_node()` | `fn insert_node(&mut self, label: &str) -> Result<NodeId>` | `sqlitegraph/src/lib.rs` | ✅ | Public API |
| `insert_edge()` | `fn insert_edge(&mut self, from: NodeId, to: NodeId, label: &str) -> Result<EdgeId>` | `sqlitegraph/src/lib.rs` | ✅ | Public API |

## V2 Tests API Usage

### native_v2_edge_boundary_tests.rs
| Function | Exact Signature | File Location | Exists? | Notes |
|----------|----------------|---------------|---------|-------|
| `SqliteGraphBackend::new()` | `fn new(path: &Path) -> Result<Self>` | `sqlitegraph/src/backend/sqlite/mod.rs` | ✅ | SQLite backend constructor |
| `adjacent()` | `fn adjacent(&self, node: NodeId) -> Result<Vec<NodeId>>` | `sqlitegraph/src/backend.rs` | ✅ | Backend trait method |
| `NodeRecordV2Ext::to_v2()` | `fn to_v2(&self) -> NodeRecordV2` | Not found | ❌ | Missing extension trait |
| `NodeRecordV2::new()` | `fn new(id: NodeId, label: &str) -> Self` | `sqlitegraph/src/backend/native/types.rs` | ✅ | V2 node record |
| `EdgeCluster::new()` | `fn new() -> Self` | `sqlitegraph/src/backend/native/types.rs` | ✅ | V2 edge cluster |

### v2_clustered_adjacency_tdd_tests.rs
| Function | Exact Signature | File Location | Exists? | Notes |
|----------|----------------|---------------|---------|-------|
| `SqliteGraphBackend::new()` | `fn new(path: &Path) -> Result<Self>` | `sqlitegraph/src/backend/sqlite/mod.rs` | ✅ | SQLite backend constructor |
| `AdjacencyIterator::try_initialize_clustered_adjacency()` | `fn try_initialize_clustered_adjacency(&mut self) -> Result<()>` | `sqlitegraph/src/backend/native/adjacency.rs` | ✅ | V2 adjacency initialization |
| `cluster_metadata()` | `fn cluster_metadata(&self) -> &ClusterMetadata` | Not found | ❌ | Missing accessor method |
| `EdgeCluster::with_edges()` | `fn with_edges(edges: Vec<Edge>) -> Self` | Not found | ❌ | Missing constructor |

## Backend V2 Infrastructure (Source Files)

### node_store.rs
| Function | Exact Signature | File Location | Exists? | Notes |
|----------|----------------|---------------|---------|-------|
| `NodeStore::write_node_v2()` | `fn write_node_v2(&mut self, node: &NodeRecordV2) -> Result<()>` | `sqlitegraph/src/backend/native/node_store.rs` | ✅ | V2 node write |
| `NodeStore::read_node_v2()` | `fn read_node_v2(&self, id: NodeId) -> Result<Option<NodeRecordV2>>` | `sqlitegraph/src/backend/native/node_store.rs` | ✅ | V2 node read |
| `NodeStore::ensure_v2_format()` | `fn ensure_v2_format(&mut self) -> Result<()>` | `sqlitegraph/src/backend/native/node_store.rs` | ✅ | V2 format upgrade |

### edge_store.rs
| Function | Exact Signature | File Location | Exists? | Notes |
|----------|----------------|---------------|---------|-------|
| `EdgeStore::write_clustered_edges()` | `fn write_clustered_edges(&mut self, cluster: &EdgeCluster) -> Result<()>` | `sqlitegraph/src/backend/native/edge_store.rs` | ✅ | V2 clustered edge write |
| `EdgeStore::read_cluster()` | `fn read_cluster(&self, cluster_id: ClusterId) -> Result<Option<EdgeCluster>>` | `sqlitegraph/src/backend/native/edge_store.rs` | ✅ | V2 cluster read |
| `EdgeStore::ensure_v2_format()` | `fn ensure_v2_format(&mut self) -> Result<()>` | `sqlitegraph/src/backend/native/edge_store.rs` | ✅ | V2 format upgrade |

### adjacency.rs
| Function | Exact Signature | File Location | Exists? | Notes |
|----------|----------------|---------------|---------|-------|
| `AdjacencyIterator::try_initialize_clustered_adjacency()` | `fn try_initialize_clustered_adjacency(&mut self) -> Result<()>` | `sqlitegraph/src/backend/native/adjacency.rs` | ✅ | V2 adjacency init |
| `AdjacencyIterator::cluster_metadata()` | `fn cluster_metadata(&self) -> Option<&ClusterMetadata>` | Not found | ❌ | Missing accessor |
| `ClusterMetadata::new()` | `fn new() -> Self` | `sqlitegraph/src/backend/native/types.rs` | ✅ | Cluster metadata |

## Key Findings

### ✅ Existing APIs
- All high-level public APIs used by benchmarks exist and should work
- V2 backend infrastructure is largely implemented
- Core V2 types (`NodeRecordV2`, `EdgeCluster`, `ClusterMetadata`) exist

### ❌ Missing APIs
- `NodeRecordV2Ext::to_v2()` - Extension trait for V2 conversion
- `cluster_metadata()` accessor methods on adjacency iterator
- `EdgeCluster::with_edges()` constructor
- Some V2-specific accessor methods

### 🔧 Wiring Issues
- Public APIs may not be wired to use V2 backend methods
- Benchmarks likely use V1 backend despite V2 infrastructure existing
- V2 tests expect APIs that aren't fully implemented