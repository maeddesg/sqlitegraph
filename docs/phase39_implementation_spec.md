# Phase 40: Conservative Mmap Implementation Specification

## Overview

This specification provides the authoritative blueprint for implementing the conservative mmap fix identified in Phase 39. The objective is to eliminate mmap corruption while preserving the Phase 38 basic I/O benefits.

## Exact Method Signatures

### **File: `sqlitegraph/src/backend/native/graph_file.rs`**

#### **Method: `write_bytes(offset: u64, data: &[u8]) -> NativeResult<()>`**
- **Lines to modify:** 344-367
- **Current LOC:** ~25 LOC
- **Target LOC:** ~30 LOC (+5 LOC)
- **Change type:** Conservative mmap remapping logic

#### **Method: `flush_write_buffer() -> NativeResult<()>`**
- **Lines to modify:** 437-468
- **Current LOC:** ~25 LOC
- **Target LOC:** ~35 LOC (+10 LOC)
- **Change type:** Enhanced mmap validation after buffer flush

#### **Method: `open(path: P) -> NativeResult<Self>`**
- **Lines to modify:** 85-110
- **Current LOC:** ~25 LOC
- **Target LOC:** ~30 LOC (+5 LOC)
- **Change type:** Improved mmap initialization validation

### **File: `sqlitegraph/src/backend/native/edge_store.rs`**

#### **Method: `read_clustered_edges(cluster_offset: FileOffset, cluster_size: u32, direction: Direction) -> NativeResult<Vec<CompactEdgeRecord>>`**
- **Lines to modify:** 796-814
- **Current LOC:** ~20 LOC
- **Target LOC:** ~30 LOC (+10 LOC)
- **Change type:** Cluster header corruption detection

### **File: `sqlitegraph/src/backend/native/adjacency.rs`**

#### **Method: `clustered_neighbors() -> NativeResult<Vec<NativeNodeId>>`**
- **Lines to modify:** 255-275
- **Current LOC:** ~20 LOC
- **Target LOC:** ~25 LOC (+5 LOC)
- **Change type:** Enhanced fallback with corruption detection

## Pseudocode Implementation

### **graph_file.rs - Conservative Mmap Remapping**

```rust
impl GraphFile {
    /// Write bytes with conservative mmap management
    pub fn write_bytes(&mut self, offset: u64, data: &[u8]) -> NativeResult<()> {
        // Existing write buffer logic unchanged
        if data.len() <= 256 && self.write_buffer.add(offset, data.to_vec()) {
            return Ok(());
        }

        self.flush_write_buffer()?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(data)?;

        // CONSERVATIVE MMAP MANAGEMENT (NEW)
        #[cfg(feature = "v2_experimental")]
        {
            let end_offset = offset + data.len() as u64;
            if self.mmap.is_some() {
                let current_mmap_size = self.mmap.as_ref().unwrap().len() as u64;

                // Only remap if growing by >4KB (prevents frequent remapping)
                if end_offset > current_mmap_size + 4096 {
                    // Ensure write buffer coherence before remapping
                    self.flush_write_buffer()?;

                    // Align to 4KB boundaries for efficiency
                    let new_size = (end_offset + 4095) & !4095;

                    self.mmap = unsafe {
                        Some(MmapOptions::new()
                            .len(new_size as usize)
                            .map_mut(&self.file)?)
                    };
                }
            }
        }

        Ok(())
    }

    /// Enhanced flush with mmap validation
    pub fn flush_write_buffer(&mut self) -> NativeResult<()> {
        let operations = self.write_buffer.flush();

        // Sort operations by offset for better I/O patterns
        let mut sorted_ops: Vec<_> = operations.into_iter().collect();
        sorted_ops.sort_by_key(|(offset, _)| *offset);

        let mut max_end_offset = 0u64;
        for (offset, data) in sorted_ops {
            let end_offset = offset + data.len() as u64;
            max_end_offset = max_end_offset.max(end_offset);

            self.file.seek(SeekFrom::Start(offset))?;
            self.file.write_all(&data)?;
        }

        // ENHANCED MMAP VALIDATION (NEW)
        #[cfg(feature = "v2_experimental")]
        {
            if self.mmap.is_some() && max_end_offset > 0 {
                let current_file_size = self.file_size()?;
                let current_mmap_size = self.mmap.as_ref().unwrap().len() as u64;

                // Remap if actual file size exceeds mmap size (catches corruption)
                if current_file_size > current_mmap_size {
                    self.mmap = unsafe {
                        Some(MmapOptions::new()
                            .map_mut(&self.file)?)
                    };
                }
            }
        }

        Ok(())
    }
}
```

### **edge_store.rs - Cluster Corruption Detection**

```rust
impl EdgeStore {
    /// Read clustered edges with corruption detection
    pub fn read_clustered_edges(
        &mut self,
        cluster_offset: FileOffset,
        cluster_size: u32,
        direction: Direction,
    ) -> NativeResult<Vec<CompactEdgeRecord>> {
        if cluster_offset == 0 || cluster_size == 0 {
            return Ok(Vec::new());
        }

        let mut cluster_data = vec![0u8; cluster_size as usize];
        self.graph_file.read_bytes(cluster_offset, &mut cluster_data)?;

        // CORRUPTION DETECTION (NEW)
        #[cfg(debug_assertions)]
        {
            println!("DEBUG: Reading cluster at offset {}, size {} bytes", cluster_offset, cluster_size);
            if cluster_data.len() >= 16 {
                println!("DEBUG: First 16 bytes: {:02X?}", &cluster_data[..16.min(cluster_data.len())]);
            }
        }

        // Validate cluster header integrity
        if cluster_data.len() >= 8 {
            let edge_count = u32::from_be_bytes([
                cluster_data[0], cluster_data[1], cluster_data[2], cluster_data[3]
            ]);
            let payload_size = u32::from_be_bytes([
                cluster_data[4], cluster_data[5], cluster_data[6], cluster_data[7]
            ]);

            // Detect mmap corruption patterns
            if cluster_size > 8 && edge_count == 0 && payload_size == 0 {
                return Err(NativeBackendError::CorruptEdgeRecord {
                    edge_id: -1,
                    reason: "Cluster header corruption detected - likely mmap aliasing. Header indicates no edges but cluster size > 8".to_string(),
                });
            }

            // Detect byte-swapped corruption (common mmap issue)
            if edge_count == 33554432 || edge_count > 1000000 {
                return Err(NativeBackendError::CorruptEdgeRecord {
                    edge_id: -1,
                    reason: format!("Cluster header corruption detected - edge_count appears byte-swapped: {}", edge_count),
                });
            }
        }

        // Existing deserialization logic unchanged
        let cluster = crate::backend::native::v2::edge_cluster::EdgeCluster::deserialize(&cluster_data)?;
        let edges = cluster.edges().to_vec();

        Ok(edges)
    }
}
```

### **adjacency.rs - Enhanced Fallback Logic**

```rust
impl AdjacencyIterator {
    /// Enhanced clustered neighbors with corruption handling
    pub fn clustered_neighbors(&mut self) -> NativeResult<Vec<NativeNodeId>> {
        // Existing V2 logic unchanged up to cluster read
        match edge_store.get_clustered_neighbors(
            cluster_offset,
            cluster_size,
            cluster_direction,
            self.node_id,
        ) {
            Ok(neighbors) => {
                // CORRUPTION DETECTION (NEW)
                if neighbors.iter().any(|&id| id == 1099511627776 || id > 1000000000) {
                    #[cfg(debug_assertions)]
                    {
                        println!("DEBUG: Mmap corruption detected in neighbors, falling back to V1");
                    }

                    // Fall back to V1 logic
                    self.cached_clustered_neighbors = None;
                    return self.v1_neighbors();
                }

                // Success path unchanged
                self.cached_clustered_neighbors = Some(neighbors.clone());
                self.total_count = edge_count;
                Ok(neighbors)
            }
            Err(NativeBackendError::CorruptEdgeRecord { reason, .. }) if reason.contains("mmap") => {
                #[cfg(debug_assertions)]
                {
                    println!("DEBUG: Mmap corruption in cluster read: {}, falling back to V1", reason);
                }

                // Graceful fallback to V1
                self.cached_clustered_neighbors = None;
                self.v1_neighbors()
            }
            Err(e) => {
                // Existing error handling unchanged
                Err(e)
            }
        }
    }
}
```

## Error Propagation Contract

### **Write Operations**
- **Success**: Write completes, mmap conservatively expanded if needed
- **Mmap Remap Failure**: Log warning, continue with standard I/O (no data loss)
- **File I/O Failure**: Propagate error immediately (existing behavior)

### **Read Operations**
- **Success**: Valid cluster data returned
- **Corruption Detected**: Return specific `CorruptEdgeRecord` error with detailed reason
- **Header Mismatch**: Return existing validation errors with enhanced context

### **Cluster Operations**
- **Corruption Detection**: Automatic V1 fallback with debug logging
- **Validation Failure**: Detailed error message for debugging
- **Fallback Success**: V1 path provides data consistency guarantee

## Expected Side Effects

### **Positive Effects**
1. **Eliminates mmap corruption** through conservative remapping strategy
2. **Preserves Phase 38 benefits** for basic I/O operations
3. **Provides graceful degradation** when mmap issues are detected
4. **Maintains V2 API compatibility** - no breaking changes

### **Negative Effects**
1. **Slightly less aggressive mmap growth** (4KB threshold)
2. **Minimal performance impact** from corruption detection (debug-only in most cases)
3. **Additional validation overhead** (<1% performance impact expected)

### **Neutral Effects**
- **No API changes** - all modifications are internal
- **Backward compatibility preserved** - existing behavior unchanged
- **Feature flag behavior maintained** - only affects v2_experimental builds

## Cluster Behavior Guarantees

### **Before Conservative Fix (Current State)**
- ❌ Cluster headers can be corrupted by mmap aliasing
- ❌ Magic number corruption occurs during GraphFile reopen
- ❌ Node IDs can become corrupted in large operations
- ❌ 2/24 V2 tests passing (8.3% success rate)

### **After Conservative Fix (Expected State)**
- ✅ Cluster headers maintain integrity across all operations
- ✅ Magic number corruption eliminated through careful mmap lifecycle management
- ✅ Node IDs remain valid during complex cluster operations
- ✅ Target: 20+/24 V2 tests passing (80%+ success rate)

## Validation Criteria

### **Phase 40 Acceptance Criteria**
1. **All Phase 39 TDD tests pass** (5/5 new tests)
2. **V2 regression tests improve significantly** (target: 80%+ pass rate)
3. **No regression in Phase 38 basic I/O tests** (must remain 4/4 passing)
4. **No new memory leaks or safety issues** introduced

### **Success Metrics**
- **Magic number corruption eliminated**: 0 instances in test suite
- **Cluster header corruption eliminated**: 0 instances in test suite
- **Node ID corruption eliminated**: 0 instances in test suite
- **Performance impact minimal**: <5% overhead on V2 operations
- **Memory safety maintained**: no new unsafe code issues

This specification provides the complete implementation blueprint for Phase 40, ensuring all changes are surgical, well-tested, and focused specifically on the mmap corruption issues identified in Phase 39.