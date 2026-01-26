# Phase 74 Source Grounding Snippets

## Edge Insertion Path
sqlitegraph/src/backend/native/graph_backend.rs:90:
```rust
    fn insert_edge(&self, edge: EdgeSpec) -> Result<i64, SqliteGraphError> {
```

## V2 Cluster Writer Entry Points

### write_v2_edge_clusters()
sqlitegraph/src/backend/native/edge_store.rs:131:
```rust
    /// Write V2 edge clusters for source and target nodes
    fn write_v2_edge_clusters(&mut self, edge: &EdgeRecord) -> NativeResult<()> {
```

sqlitegraph/src/backend/native/edge_store.rs:137-151:
```rust
        // For outgoing cluster (source node)
        let outgoing_edge = CompactEdgeRecord::new(
            edge.to_id,
            0, // edge_type_offset (will be set by string table)
            edge.data.clone(),
        );
        self.write_or_update_v2_cluster(edge.from_id, outgoing_edge, Direction::Outgoing)?;

        // For incoming cluster (target node)
        let incoming_edge = CompactEdgeRecord::new(
            edge.from_id,
            0, // edge_type_offset (will be set by string table)
            edge.data.clone(),
        );
        self.write_or_update_v2_cluster(edge.to_id, incoming_edge, Direction::Incoming)?;
```

### write_or_update_v2_cluster()
sqlitegraph/src/backend/native/edge_store.rs:157:
```rust
    /// Write or update a V2 cluster for a specific node and direction
    fn write_or_update_v2_cluster(
        &mut self,
        node_id: NativeNodeId,
        edge: CompactEdgeRecord,
        direction: super::v2::edge_cluster::Direction,
    ) -> NativeResult<()>
```

sqlitegraph/src/backend/native/edge_store.rs:179-190:
```rust
        // Create cluster using the proper EdgeRecord->CompactEdgeRecord conversion with string table
        let cluster = EdgeCluster::create_from_edges(
            &[edge],
            &string_table,
            matches!(direction, super::v2::edge_cluster::Direction::Outgoing),
        )?;

        // Serialize the cluster with proper framing
        let cluster_data = cluster.serialize();

        // Write cluster data to end of file (simplified allocation)
        let current_file_size = self.graph_file.file_size()?;
        let cluster_offset = if current_file_size == 0 {
            header.cluster_floor()
        } else {
            current_file_size
        };

        self.graph_file.write_bytes(cluster_offset, &cluster_data)?;
```

## V2 Cluster Reader Entry Points

### read_v2_clustered_neighbors()
sqlitegraph/src/backend/native/edge_store.rs:694:
```rust
    /// Read V2 clustered neighbors from the file (Phase 69 implementation)
    fn read_v2_clustered_neighbors(
        &mut self,
        node_id: NativeNodeId,
        cluster_offset: u64,
        cluster_size: u32,
        direction: crate::backend::native::v2::edge_cluster::Direction,
    ) -> NativeResult<Vec<NativeNodeId>>
```

sqlitegraph/src/backend/native/edge_store.rs:713-728:
```rust
        // Read cluster bytes from file
        let mut cluster_bytes = vec![0u8; cluster_size as usize];
        self.graph_file.read_bytes(cluster_offset, &mut cluster_bytes)?;

        // Create trace context for strict V2 framed mode debugging
        let _trace_guard = TraceGuard::new(
            TraceContext {
                operation: "read_v2_clustered_neighbors",
                node_id,
                cluster_offset,
                payload_size: cluster_size,
                direction: direction.clone(),
            },
        );

        // Deserialize cluster using strict V2 framed mode
        let cluster = EdgeCluster::deserialize(&cluster_bytes)?;
```

## Cluster Serialization/Deserialization

### EdgeCluster::create_from_edges()
sqlitegraph/src/backend/native/v2/edge_cluster/cluster.rs:
```rust
    pub fn create_from_edges(
        edges: &[CompactEdgeRecord],
        string_table: &StringTable,
        is_outgoing: bool,
    ) -> Result<Self, ClusterError>
```

### EdgeCluster::serialize()
sqlitegraph/src/backend/native/v2/edge_cluster/cluster.rs:
```rust
    pub fn serialize(&self) -> Vec<u8>
```

### EdgeCluster::deserialize()
sqlitegraph/src/backend/native/v2/edge_cluster/cluster.rs:
```rust
    pub fn deserialize(data: &[u8]) -> Result<Self, ClusterError>
```