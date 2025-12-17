# Phase 34: V2 Cluster Architecture Specification

## Overview

This document defines the new V2 cluster architecture that makes **EdgeCluster the single source of truth** for adjacency data, eliminating the corrupting reconstruction pipeline that converts between CompactEdgeRecord and EdgeRecord.

## Current V2 Cluster Issues (Analysis)

### 1. Data Loss Corruption Pipeline
The current `update_v2_clustered_adjacency()` function implements a corrupting cycle:
```
EdgeRecord → CompactEdgeRecord → (data loss) → EdgeRecord → EdgeCluster
```

**Specific corruption locations:**
- `edge_store.rs:185-187`: Hardcoded `"reconstructed"` edge_type
- `edge_store.rs:231-233`: Empty JSON payload `{}` instead of original data
- Loss of original `edge.edge_type` and `edge.data` during reconstruction

### 2. CompactEdgeRecord vs EdgeRecord Information Gap
**CompactEdgeRecord preserves:**
- `neighbor_id: i64` ✓
- `edge_type_offset: u16` ✓ (references StringTable)
- `edge_data: Vec<u8>` ✓ (serialized JSON)

**What's lost during reconstruction:**
- Original `edge.edge_type: String` (replaced with "reconstructed")
- Original `edge.data: serde_json::Value` (replaced with `{}`)

### 3. StringTable Integration Mismatch
- StringTable properly stores edge type → offset mapping (`table.rs:50-72`)
- EdgeCluster creation correctly uses string table (`cluster.rs:52`)
- But reconstruction bypasses this system entirely

## New V2 Cluster Architecture Specification

### 1. Single Source of Truth Principle

**EdgeCluster IS the authoritative data structure.**
- No reconstruction from CompactEdgeRecord to EdgeRecord
- No data loss during cluster updates
- StringTable integration throughout the pipeline

### 2. Cluster Accumulation Pipeline

**New cluster update algorithm:**
```
Existing Cluster (CompactEdgeRecord[]) + New Edge → Updated Cluster (CompactEdgeRecord[])
```

**Implementation steps:**
1. Read existing cluster as `Vec<CompactEdgeRecord>` (no EdgeRecord conversion)
2. Create new `CompactEdgeRecord` directly from `EdgeRecord` + StringTable
3. Append to existing compact edges
4. Create new `EdgeCluster` directly from `CompactEdgeRecord[]` + StringTable
5. Serialize and write updated cluster

### 3. CompactEdgeRecord Creation API

**New function needed:**
```rust
impl CompactEdgeRecord {
    pub fn from_edge_record(
        edge: &EdgeRecord,
        direction: Direction,
        string_table: &mut StringTable,
    ) -> NativeResult<Self> {
        let neighbor_id = match direction {
            Direction::Outgoing => edge.to_id,
            Direction::Incoming => edge.from_id,
        };
        let type_offset = string_table.get_or_add_offset(&edge.edge_type)?;
        let data = serde_json::to_vec(&edge.data)?;
        Ok(Self::new(neighbor_id, type_offset, data))
    }
}
```

### 4. Direct Cluster Creation from Compact Records

**New EdgeCluster method needed:**
```rust
impl EdgeCluster {
    pub fn create_from_compact_edges(
        compact_edges: Vec<CompactEdgeRecord>,
        node_id: i64,
        direction: Direction,
    ) -> NativeResult<Self> {
        let serialized_size = compact_edges.iter().map(|c| c.size_bytes()).sum();
        Ok(Self {
            offset: 0,
            serialized_size,
            edges: compact_edges,
        })
    }
}
```

### 5. Updated Cluster Update Pipeline

**Replace `update_v2_clustered_adjacency()` implementation:**

```rust
fn update_v2_clustered_adjacency(
    &mut self,
    edge: &EdgeRecord,
    source_node: &mut NodeRecordV2,
    target_node: &mut NodeRecordV2,
) -> NativeResult<()> {
    let mut string_table = self.load_or_create_string_table()?;

    // Update outgoing cluster for source_node
    self.update_single_direction_cluster(
        source_node,
        edge,
        Direction::Outgoing,
        &mut string_table,
    )?;

    // Update incoming cluster for target_node
    self.update_single_direction_cluster(
        target_node,
        edge,
        Direction::Incoming,
        &mut string_table,
    )?;

    Ok(())
}

fn update_single_direction_cluster(
    &mut self,
    node: &mut NodeRecordV2,
    edge: &EdgeRecord,
    direction: Direction,
    string_table: &mut StringTable,
) -> NativeResult<()> {
    // 1. Read existing compact edges (no EdgeRecord conversion)
    let mut compact_edges = if node.has_cluster_for_direction(direction) {
        self.read_clustered_edges(node.cluster_offset(direction), node.cluster_size(direction), direction)?
    } else {
        Vec::new()
    };

    // 2. Create new compact edge directly from EdgeRecord
    let new_compact = CompactEdgeRecord::from_edge_record(edge, direction, string_table)?;
    compact_edges.push(new_compact);

    // 3. Create cluster directly from compact edges (no EdgeRecord reconstruction)
    let cluster = EdgeCluster::create_from_compact_edges(compact_edges, node.id, direction)?;

    // 4. Serialize and write cluster
    let cluster_data = cluster.serialize();
    let cluster_offset = self.graph_file.file_size()?;
    let cluster_size = cluster_data.len() as u32;

    self.graph_file.write_bytes(cluster_offset, &cluster_data)?;
    self.graph_file.flush()?;

    // 5. Update node metadata
    node.set_cluster(direction, cluster_offset, cluster_size, cluster.edge_count());

    Ok(())
}
```

### 6. NodeRecordV2 Helper Methods

**Add convenience methods:**
```rust
impl NodeRecordV2 {
    pub fn has_cluster_for_direction(&self, direction: Direction) -> bool {
        match direction {
            Direction::Outgoing => self.has_outgoing_edges(),
            Direction::Incoming => self.has_incoming_edges(),
        }
    }

    pub fn cluster_offset(&self, direction: Direction) -> FileOffset {
        match direction {
            Direction::Outgoing => self.outgoing_cluster_offset,
            Direction::Incoming => self.incoming_cluster_offset,
        }
    }

    pub fn cluster_size(&self, direction: Direction) -> u32 {
        match direction {
            Direction::Outgoing => self.outgoing_cluster_size,
            Direction::Incoming => self.incoming_cluster_size,
        }
    }

    pub fn set_cluster(&mut self, direction: Direction, offset: FileOffset, size: u32, count: u32) {
        match direction {
            Direction::Outgoing => self.set_outgoing_cluster(offset, size, count),
            Direction::Incoming => self.set_incoming_cluster(offset, size, count),
        }
    }
}
```

### 7. StringTable Persistence

**StringTable needs to be persisted across graph file operations:**
- Load existing StringTable from graph file header
- Update StringTable during cluster operations
- Persist StringTable back to graph file header

## Implementation Requirements

### 1. No Breaking Changes
- Keep existing EdgeCluster API for backward compatibility
- Add new methods alongside existing ones
- Maintain current file format specification

### 2. Performance Requirements
- Sequential I/O for cluster operations
- Minimal memory allocations during updates
- Preserve cluster compactness heuristics

### 3. Data Integrity
- Zero data loss during cluster updates
- Preserve original edge_type and edge_data exactly
- StringTable consistency across cluster operations

### 4. Testing Requirements
- TDD approach with comprehensive test coverage
- Roundtrip consistency validation
- Multi-edge cluster accumulation tests
- Corruption detection and prevention

## Success Metrics

1. **Zero Data Loss**: Original edge_type and edge_data preserved in all cluster operations
2. **Performance**: No regression in cluster read/write performance
3. **Correctness**: All neighbor queries return identical results before/after migration
4. **Maintainability**: Clean separation between cluster and edge record layers

## Migration Strategy

1. Implement new cluster pipeline alongside existing corrupt pipeline
2. Add feature flag to switch between pipelines
3. Run comprehensive A/B testing
4. Once validated, remove corrupt pipeline entirely
5. Clean up dead code and unused reconstruction methods

This specification ensures that EdgeCluster remains the authoritative source of truth while eliminating the data corruption introduced by EdgeRecord reconstruction.