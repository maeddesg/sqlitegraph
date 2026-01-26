# V2 Field Mapping Table

## NodeRecordV2 Mappings (sqlitegraph/src/backend/native/v2/node_record_v2/record.rs:7-19)

| V1 Field | V2 Field | Type | Semantic Change | Notes |
|----------|----------|------|------------------|-------|
| `outgoing_count` | `outgoing_edge_count` | `u32` | **Same semantics** | Exact rename, same counter behavior |
| `incoming_count` | `incoming_edge_count` | `u32` | **Same semantics** | Exact rename, same counter behavior |
| `outgoing_offset` | `outgoing_cluster_offset` | `FileOffset` | **Same semantics** | More descriptive name |
| `incoming_offset` | `incoming_cluster_offset` | `FileOffset` | **Same semantics** | More descriptive name |
| N/A | `outgoing_cluster_size` | `u32` | **New V2 field** | Size of outgoing cluster in bytes |
| N/A | `incoming_cluster_size` | `u32` | **New V2 field** | Size of incoming cluster in bytes |

**NodeRecordV2 Constructor**: `NodeRecordV2::new(id, kind, name, data)` - same signature as V1

---

## EdgeRecord Mappings

### Critical Architectural Change

**V1 EdgeRecord** (removed): Full bidirectional record with `id`, `from_id`, `to_id`, `edge_type`, `flags`, `data`

**V2 CompactEdgeRecord** (sqlitegraph/src/backend/native/v2/edge_cluster/compact_record.rs:12-18): Direction-agnostic compact record

| V1 Field | V2 Equivalent | Type | Semantic Change | Notes |
|----------|---------------|------|------------------|-------|
| `from_id` | Context-dependent | `i64` | **Major semantic change** | Only stored as `neighbor_id` after direction resolution |
| `to_id` | `neighbor_id` | `i64` | **Context-dependent** | For outgoing clusters: `neighbor_id = to_id` |
| N/A | `neighbor_id` | `i64` | **New V2 field** | Target of adjacency direction (resolved at insertion) |
| `edge_type` | `edge_type_offset` | `u16` | **Different representation** | Offset into cluster's string table, not stored string |
| `data` | `edge_data` | `Vec<u8>` | **Same semantics, different type** | Serialized JSON bytes instead of Value |
| `id` | Not stored | N/A | **Removed** | Edge IDs are implicit in cluster position |
| `flags` | Not stored | N/A | **Removed** | Edge flags moved to cluster metadata |

---

## Edge Insertion Path Mapping

### V2 Pattern (sqlitegraph/src/backend/native/v2/edge_cluster/compact_record.rs:81-112)

**Constructor**: `CompactEdgeRecord::from_edge_record(edge: &EdgeRecord, direction: Direction, string_table: &mut StringTable)`

```rust
let neighbor_id = match direction {
    Direction::Outgoing => edge.to_id,    // For outgoing cluster, store target
    Direction::Incoming => edge.from_id,  // For incoming cluster, store source
};
```

**Key Insight**: V2 CompactEdgeRecord is **directional** - it only stores the neighbor relevant to the cluster direction. The context (which node's adjacency we're reading) provides the other endpoint.

---

## String Table Integration

**V1**: Stored `edge_type` as `String` in each edge record
**V2**: Stores `edge_type_offset: u16` pointing to shared string table in cluster

**Conversion**: `string_table.get_or_add_offset(&edge.edge_type)` returns offset for compact storage

---

## Serialization Changes

### V1 EdgeRecord (removed)
```rust
EdgeRecord::new(id: NativeEdgeId, from_id: NativeNodeId, to_id: NativeNodeId, edge_type: String, data: serde_json::Value)
```

### V2 CompactEdgeRecord
```rust
CompactEdgeRecord::new(neighbor_id: i64, edge_type_offset: u16, edge_data: Vec<u8>)
```

**Layout**: `[neighbor_id: 8 bytes][edge_type_offset: 2 bytes][edge_data_len: 2 bytes][edge_data: variable]`

---

## Required Logic Changes

### 1. NodeRecordV2 Usage
**Simple rename**: Replace all `outgoing_count` → `outgoing_edge_count`, `incoming_count` → `incoming_edge_count`

### 2. Edge Storage Logic
**Complex change**: Cannot use V1 field access directly. Must use:
- `from_edge_record()` method for conversions
- Context-aware neighbor resolution (which node's adjacency are we reading?)
- String table for edge type storage/retrieval

### 3. Edge Reading Logic
**Complex change**:
- Read CompactEdgeRecord from cluster
- Use context (source node + direction) to reconstruct full edge semantics
- Resolve `neighbor_id` back to full edge information with string table

### 4. Iterator Logic
**Context dependency**: Iterators must know which node they're iterating from to properly interpret `neighbor_id`

---

## Verification Requirements

1. **NodeRecordV2**: Simple field rename verification
2. **Edge paths**: Must verify `from_edge_record()` usage in all insertion points
3. **String table**: Must verify string table is properly maintained alongside clusters
4. **Adjacency context**: Must verify all CompactEdgeRecord readers have proper node context

---

## Critical Files Requiring Changes

1. **sqlitegraph/src/backend/native/adjacency.rs** - NodeRecordV2 field renames
2. **sqlitegraph/src/backend/native/edge_store.rs** - Complete edge logic rewrite for V2
3. **Any files using EdgeRecord directly** - Update to use CompactEdgeRecord with proper context

**Total Field Renames**: 2 simple (NodeRecord) + ~80 complex (EdgeRecord)