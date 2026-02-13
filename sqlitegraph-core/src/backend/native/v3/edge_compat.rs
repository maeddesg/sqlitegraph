//! V3 Edge Compatibility Layer
//!
//! This module provides a compatibility layer for using V2 EdgeCluster format
//! within V3's page-based storage system. This is a temporary design to get
//! V3 working end-to-end quickly without re-inventing edge layout while
//! NodeStore/B+Tree/allocator/WAL are still settling.
//!
//! # Design Principles
//!
//! 1. **Logical NodeIDs only**: EdgeCluster references NodeID, not V2 slot assumptions
//!    Resolution is via B+Tree → page.
//!
//! 2. **V3 pages + allocator**: Edge storage lives in V3 pages allocated by V3 allocator.
//!    Only the record format is reused from V2.
//!
//! 3. **Separate PageType**: Edges get their own PageType::EDGE_CLUSTER.
//!    Node pages never embed edge blobs.
//!
//! 4. **WAL-first**: Write path is WAL'd (insert_edge/delete_edge/update adjacency)
//!    before any compaction/relocation.
//!
//! # Architecture
//!
//! ```
//! EdgeCluster { src: NodeId, dsts: Vec<NodeId>, dir: Out|In, metadata }
//!
//! B+Tree index: key = (src, dir) → value = edge_page_id
//!
//! Neighbor query: lookup_edge_page(src) → decode cluster → return iterator
//!
//! Insert edge: load cluster (or create), append, maybe split if page full
//! ```

use crate::backend::native::{
    types::{NativeBackendError, NativeResult},
    v2::edge_cluster::{
        cluster_trace::Direction as V2Direction,
        compact_record::CompactEdgeRecord,
    },
};
use crate::backend::native::v3::{
    btree::BTreeManager,
    constants::DEFAULT_PAGE_SIZE,
    wal::WALWriter,
};
use std::collections::HashMap;

/// Page type constants for V3 storage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PageType {
    /// Free/unallocated page
    Free = 0,
    /// B+Tree index page (node_id → page_id mapping)
    BTreeIndex = 1,
    /// Node data page (contains NodeRecordV3 entries)
    NodeData = 2,
    /// Edge cluster page (contains EdgeCluster entries)
    EdgeCluster = 3,
    /// WAL page (contains WAL records)
    Wal = 4,
    /// Checkpoint page
    Checkpoint = 5,
}

impl PageType {
    /// Convert from u8 to PageType
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(PageType::Free),
            1 => Some(PageType::BTreeIndex),
            2 => Some(PageType::NodeData),
            3 => Some(PageType::EdgeCluster),
            4 => Some(PageType::Wal),
            5 => Some(PageType::Checkpoint),
            _ => None,
        }
    }
}

/// Direction for edge traversal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Outgoing,
    Incoming,
}

impl Direction {
    /// Convert to V2 Direction for EdgeCluster compatibility
    pub fn to_v2(&self) -> V2Direction {
        match self {
            Direction::Outgoing => V2Direction::Outgoing,
            Direction::Incoming => V2Direction::Incoming,
        }
    }
}

/// Edge cluster entry for V3 storage
/// Uses V2 CompactEdgeRecord format for compatibility
#[derive(Debug, Clone)]
pub struct V3EdgeCluster {
    /// Source node ID (logical, not slot)
    pub src: i64,
    /// Destination node IDs with edge data
    pub edges: Vec<CompactEdgeRecord>,
    /// Edge direction
    pub direction: Direction,
    /// Format version for future migration
    pub format_version: u8,
    /// Page ID where this cluster is stored
    pub page_id: u64,
}

impl V3EdgeCluster {
    /// Create new empty edge cluster
    pub fn new(src: i64, direction: Direction, page_id: u64) -> Self {
        Self {
            src,
            edges: Vec::new(),
            direction,
            format_version: 1, // V2 compat format
            page_id,
        }
    }

    /// Add edge to cluster
    pub fn add_edge(&mut self, dst: i64) {
        let edge = CompactEdgeRecord::new(dst, 0, Vec::new());
        self.edges.push(edge);
    }

    /// Get destination node IDs
    pub fn dsts(&self) -> Vec<i64> {
        self.edges.iter().map(|e| e.neighbor_id).collect()
    }

    /// Serialize to bytes for page storage
    /// Format: [version: 1 byte] [edge_count: 4 bytes] [edges...]
    pub fn serialize(&self) -> NativeResult<Vec<u8>> {
        let mut result = Vec::new();
        
        // Header: format_version (1 byte)
        result.push(self.format_version);
        
        // Edge count (4 bytes, big-endian)
        let count = self.edges.len() as u32;
        result.extend_from_slice(&count.to_be_bytes());
        
        // Serialize each edge using V2 CompactEdgeRecord format
        for edge in &self.edges {
            let edge_bytes = edge.serialize();
            result.extend_from_slice(&edge_bytes);
        }
        
        Ok(result)
    }

    /// Deserialize from bytes
    /// Format: [version: 1 byte] [edge_count: 4 bytes] [edges...]
    pub fn deserialize(bytes: &[u8], page_id: u64) -> NativeResult<Self> {
        if bytes.len() < 5 {
            return Err(NativeBackendError::DeserializationError {
                context: "Edge cluster bytes too short".to_string(),
            });
        }

        let format_version = bytes[0];
        
        if format_version != 1 {
            return Err(NativeBackendError::DeserializationError {
                context: format!("Unknown edge cluster format version: {}", format_version),
            });
        }

        // Read edge count
        let count = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize;
        
        let mut edges = Vec::with_capacity(count);
        let mut pos = 5;
        
        // Deserialize each edge
        // CompactEdgeRecord format: [neighbor_id: 8 bytes] [type_offset: 2 bytes] [data_len: 2 bytes] [data: variable]
        for _ in 0..count {
            if pos + 12 > bytes.len() {
                return Err(NativeBackendError::DeserializationError {
                    context: "Edge data truncated".to_string(),
                });
            }
            
            let neighbor_id = i64::from_be_bytes(bytes[pos..pos+8].try_into().unwrap());
            pos += 8;
            
            let type_offset = u16::from_be_bytes(bytes[pos..pos+2].try_into().unwrap());
            pos += 2;
            
            let data_len = u16::from_be_bytes(bytes[pos..pos+2].try_into().unwrap()) as usize;
            pos += 2;
            
            let edge_data = if data_len > 0 {
                if pos + data_len > bytes.len() {
                    return Err(NativeBackendError::DeserializationError {
                        context: "Edge data truncated".to_string(),
                    });
                }
                bytes[pos..pos+data_len].to_vec()
            } else {
                Vec::new()
            };
            pos += data_len;
            
            edges.push(CompactEdgeRecord::new(neighbor_id, type_offset, edge_data));
        }

        // Direction and src need to be passed in or stored separately
        // For now, use placeholders - these should come from B+Tree key
        Ok(Self {
            src: 0, // Should be set by caller
            edges,
            direction: Direction::Outgoing, // Should be set by caller
            format_version,
            page_id,
        })
    }
}

/// V3 Edge Store using V2 EdgeCluster compatibility format
/// 
/// This is a temporary implementation to get V3 working end-to-end.
/// Future versions may replace EdgeCluster with native V3 edge pages.
pub struct V3EdgeStore {
    /// B+Tree index: (src, dir) → page_id
    btree: BTreeManager,
    /// Optional WAL writer for durability
    wal: Option<WALWriter>,
    /// In-memory cache of loaded clusters
    cache: HashMap<(i64, Direction), V3EdgeCluster>,
}

impl V3EdgeStore {
    /// Create new edge store
    pub fn new(btree: BTreeManager, wal: Option<WALWriter>) -> Self {
        Self {
            btree,
            wal,
            cache: HashMap::new(),
        }
    }

    /// Lookup edge cluster for a source node and direction
    /// 
    /// Returns the cluster if found, None if no edges exist
    pub fn lookup_cluster(&mut self, src: i64, dir: Direction) -> NativeResult<Option<&V3EdgeCluster>> {
        // Check cache first
        let cache_key = (src, dir);
        if let Some(cluster) = self.cache.get(&cache_key) {
            return Ok(Some(cluster));
        }

        // TODO: Load from disk via B+Tree lookup
        // For now, return None (cluster doesn't exist yet)
        Ok(None)
    }

    /// Insert an edge
    /// 
    /// Creates new cluster or appends to existing
    pub fn insert_edge(&mut self, src: i64, dst: i64, dir: Direction) -> NativeResult<()> {
        let cache_key = (src, dir);
        
        // Get or create cluster
        let cluster = if let Some(existing) = self.cache.get_mut(&cache_key) {
            existing
        } else {
            // Create new cluster
            // TODO: Allocate page via V3 allocator
            let page_id = 0; // Placeholder
            let new_cluster = V3EdgeCluster::new(src, dir, page_id);
            self.cache.insert(cache_key, new_cluster);
            self.cache.get_mut(&cache_key).unwrap()
        };

        // Add edge
        cluster.add_edge(dst);

        // Log to WAL if configured
        if let Some(ref mut wal) = self.wal {
            // TODO: Create proper WAL record for edge insert
            // wal.append(&V3WALRecord::edge_insert(...))?;
            let _ = wal;
        }

        Ok(())
    }

    /// Get neighbors (outgoing or incoming)
    /// 
    /// Returns iterator over destination node IDs
    pub fn neighbors(&mut self, src: i64, dir: Direction) -> NativeResult<Vec<i64>> {
        match self.lookup_cluster(src, dir)? {
            Some(cluster) => Ok(cluster.dsts()),
            None => Ok(Vec::new()),
        }
    }

    /// Get outgoing neighbors
    pub fn outgoing(&mut self, src: i64) -> NativeResult<Vec<i64>> {
        self.neighbors(src, Direction::Outgoing)
    }

    /// Get incoming neighbors
    pub fn incoming(&mut self, src: i64) -> NativeResult<Vec<i64>> {
        self.neighbors(src, Direction::Incoming)
    }

    /// Clear in-memory cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Flush dirty clusters to disk
    pub fn flush(&mut self) -> NativeResult<()> {
        // TODO: Write dirty clusters to pages
        // TODO: Update B+Tree index
        // TODO: WAL checkpoint
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_type_from_u8() {
        assert_eq!(PageType::from_u8(0), Some(PageType::Free));
        assert_eq!(PageType::from_u8(1), Some(PageType::BTreeIndex));
        assert_eq!(PageType::from_u8(2), Some(PageType::NodeData));
        assert_eq!(PageType::from_u8(3), Some(PageType::EdgeCluster));
        assert_eq!(PageType::from_u8(255), None);
    }

    #[test]
    fn test_direction_conversion() {
        assert_eq!(Direction::Outgoing.to_v2(), V2Direction::Outgoing);
        assert_eq!(Direction::Incoming.to_v2(), V2Direction::Incoming);
    }

    #[test]
    fn test_v3_edge_cluster_new() {
        let cluster = V3EdgeCluster::new(42, Direction::Outgoing, 100);
        assert_eq!(cluster.src, 42);
        assert!(cluster.edges.is_empty());
        assert_eq!(cluster.direction, Direction::Outgoing);
        assert_eq!(cluster.page_id, 100);
        assert_eq!(cluster.format_version, 1);
    }

    #[test]
    fn test_v3_edge_cluster_add_edge() {
        let mut cluster = V3EdgeCluster::new(1, Direction::Outgoing, 1);
        cluster.add_edge(2);
        cluster.add_edge(3);
        assert_eq!(cluster.dsts(), vec![2, 3]);
    }

    #[test]
    fn test_v3_edge_cluster_serialize_roundtrip() {
        let mut cluster = V3EdgeCluster::new(42, Direction::Outgoing, 100);
        cluster.add_edge(100);
        cluster.add_edge(200);
        
        let bytes = cluster.serialize().unwrap();
        let deserialized = V3EdgeCluster::deserialize(&bytes, 100).unwrap();
        
        assert_eq!(deserialized.format_version, 1);
        assert_eq!(deserialized.dsts(), vec![100, 200]);
        assert_eq!(deserialized.page_id, 100);
    }
}
