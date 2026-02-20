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
    header::PersistentHeaderV3,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::io::{Write, Seek, SeekFrom};
use std::fs::OpenOptions;
use std::path::PathBuf;

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

/// V3 Edge Store - PERFORMANCE FIX: Store Arc<[i64]> in cache to avoid cloning
///
/// This change makes neighbor queries faster by:
/// 1. Using RwLock instead of &mut self (allows concurrent reads)
/// 2. Storing Arc<[i64]> instead of Vec<i64> - no cloning on read!
/// 3. Direct cache lookup without indirection
pub struct V3EdgeStore {
    /// B+Tree index: (src, dir) → page_id
    #[cfg(test)]
    pub btree: parking_lot::RwLock<BTreeManager>,
    #[cfg(not(test))]
    btree: parking_lot::RwLock<BTreeManager>,
    /// Optional WAL writer for durability
    #[cfg(test)]
    pub wal: Option<RwLock<WALWriter>>,
    #[cfg(not(test))]
    wal: Option<RwLock<WALWriter>>,
    /// In-memory cache of neighbor lists - using Arc<[i64]> for zero-copy reads
    /// This matches SQLite's AdjacencyCache pattern but with Arc for zero-copy
    cache: RwLock<HashMap<(i64, Direction), Arc<[i64]>>>,
    /// Performance counters
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    /// Hit time accumulator (nanoseconds) - for profiling
    hit_time_ns: AtomicU64,
    /// Miss time accumulator (nanoseconds) - for profiling
    miss_time_ns: AtomicU64,
    /// Track dirty clusters that need to be flushed
    #[cfg(test)]
    pub dirty_clusters: RwLock<HashMap<(i64, Direction), V3EdgeCluster>>,
    #[cfg(not(test))]
    dirty_clusters: RwLock<HashMap<(i64, Direction), V3EdgeCluster>>,
    /// Path to database file for writing pages
    db_path: Option<PathBuf>,
}

impl V3EdgeStore {
    /// Create new edge store (in-memory only)
    pub fn new(btree: BTreeManager, wal: Option<WALWriter>) -> Self {
        Self {
            btree: parking_lot::RwLock::new(btree),
            wal: wal.map(|w| RwLock::new(w)),
            cache: RwLock::new(HashMap::new()),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            hit_time_ns: AtomicU64::new(0),
            miss_time_ns: AtomicU64::new(0),
            dirty_clusters: RwLock::new(HashMap::new()),
            db_path: None,
        }
    }
    
    /// Create new edge store with disk persistence path
    pub fn with_path(btree: BTreeManager, wal: Option<WALWriter>, db_path: PathBuf) -> Self {
        Self {
            btree: parking_lot::RwLock::new(btree),
            wal: wal.map(|w| RwLock::new(w)),
            cache: RwLock::new(HashMap::new()),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            hit_time_ns: AtomicU64::new(0),
            miss_time_ns: AtomicU64::new(0),
            dirty_clusters: RwLock::new(HashMap::new()),
            db_path: Some(db_path),
        }
    }

    /// Get neighbors from cache - returns Arc<[i64]> for zero-copy!
    /// 
    /// IMPROVED: On cache miss, attempts to load from disk if db_path is set.
    /// This enables recovery after reopening the edge store.
    pub fn neighbors(&self, src: i64, dir: Direction) -> NativeResult<Arc<[i64]>> {
        let key = (src, dir);
        
        // First check in-memory cache
        {
            let cache = self.cache.read();
            if let Some(neighbors) = cache.get(&key) {
                self.cache_hits.fetch_add(1, Ordering::Relaxed);
                return Ok(neighbors.clone()); // Arc clone is just pointer bump, no data copy
            }
        }
        
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        
        // Cache miss - try to load from disk if we have a db_path
        if let Some(ref db_path) = self.db_path {
            if let Ok(neighbors) = self.load_neighbors_from_disk(src, dir, db_path) {
                if !neighbors.is_empty() {
                    // Cache the loaded neighbors
                    let mut cache = self.cache.write();
                    cache.insert(key, neighbors.clone());
                    return Ok(neighbors);
                }
            }
        }
        
        Ok(Arc::from([])) // Empty slice, no allocation
    }
    
    /// Load neighbors from disk for recovery
    fn load_neighbors_from_disk(&self, src: i64, dir: Direction, db_path: &PathBuf) -> NativeResult<Arc<[i64]>> {
        use std::fs::File;
        use std::io::Read;
        use crate::backend::native::v3::constants::V3_HEADER_SIZE;
        
        // Calculate page ID (same formula as in flush)
        let page_id = (src as u64) * 2 + if dir == Direction::Outgoing { 100 } else { 200 };
        let offset = V3_HEADER_SIZE as u64 + (page_id - 1) * DEFAULT_PAGE_SIZE;
        
        // Try to open file and read page
        let mut file = match File::open(db_path) {
            Ok(f) => f,
            Err(_) => return Ok(Arc::from([])), // File doesn't exist yet
        };
        
        // Seek to page offset
        if let Err(_) = file.seek(SeekFrom::Start(offset)) {
            return Ok(Arc::from([]));
        }
        
        // Read page data
        let mut buffer = vec![0u8; 4096]; // Read a full page
        match file.read(&mut buffer) {
            Ok(n) if n > 0 => {
                // Try to deserialize cluster from page
                match V3EdgeCluster::deserialize(&buffer, page_id) {
                    Ok(cluster) => {
                        let neighbors: Vec<i64> = cluster.dsts();
                        Ok(Arc::from(neighbors.into_boxed_slice()))
                    }
                    Err(_) => Ok(Arc::from([])), // Deserialization failed
                }
            }
            _ => Ok(Arc::from([])), // Read failed or empty
        }
    }

    /// Get outgoing neighbors
    pub fn outgoing(&self, src: i64) -> NativeResult<Arc<[i64]>> {
        self.neighbors(src, Direction::Outgoing)
    }

    /// Get incoming neighbors
    pub fn incoming(&self, src: i64) -> NativeResult<Arc<[i64]>> {
        self.neighbors(src, Direction::Incoming)
    }

    /// Insert an edge - uses interior mutability via RwLock, takes &self!
    pub fn insert_edge(&self, src: i64, dst: i64, dir: Direction) -> NativeResult<()> {
        let cache_key = (src, dir);
        let mut cache = self.cache.write();

        // Get or create entry
        if let Some(neighbors) = cache.get_mut(&cache_key) {
            // Existing entry - need to convert Arc back to Vec, modify, then re-Arc
            let mut vec: Vec<i64> = neighbors.to_vec();
            if !vec.contains(&dst) {
                vec.push(dst);
                *neighbors = Arc::from(vec);
            }
        } else {
            // Create new entry - wrap in Arc
            cache.insert(cache_key, Arc::from(vec![dst]));
        }

        // Mark cluster as dirty for later flush
        {
            let mut dirty = self.dirty_clusters.write();
            let cluster = dirty.entry(cache_key).or_insert_with(|| {
                V3EdgeCluster::new(src, dir, 0) // page_id will be assigned during flush
            });
            cluster.add_edge(dst);
        }

        // Log to WAL if configured
        if let Some(ref wal) = self.wal {
            // CRITICAL TODO: Create proper WAL record for edge insert
            // For now, we need to implement a custom edge insert WAL record
            let mut wal_guard = wal.write();
            // Write a PageWrite record as placeholder for edge data
            // In the full implementation, we'd have a dedicated EdgeInsert record type
            let edge_data = format!("EDGE:{}:{}:{}", src, dst, dir as u8).into_bytes();
            let _ = wal_guard.page_write(0, 0, edge_data);
        }

        Ok(())
    }

    /// Clear in-memory cache
    pub fn clear_cache(&self) {
        self.cache.write().clear();
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
    }

    /// Print cache statistics for debugging/benchmarking
    pub fn print_stats(&self) {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let cache_size = self.cache.read().len();
        let hit_ns = self.hit_time_ns.load(Ordering::Relaxed);
        let miss_ns = self.miss_time_ns.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total > 0 { (hits as f64 / total as f64) * 100.0 } else { 0.0 };
        let avg_hit_ns = if hits > 0 { hit_ns / hits } else { 0 };
        let avg_miss_ns = if misses > 0 { miss_ns / misses } else { 0 };

        println!("Cache stats:");
        println!("  Entries: {}", cache_size);
        println!("  Hits: {} ({:.1}%)", hits, hit_rate);
        println!("  Misses: {}", misses);
        println!("  Avg hit time: {} ns", avg_hit_ns);
        println!("  Avg miss time: {} ns", avg_miss_ns);
    }

    /// Flush dirty clusters to disk
    /// 
    /// IMPLEMENTATION:
    /// 1. Write dirty clusters to pages
    /// 2. Update B+Tree index  
    /// 3. WAL checkpoint (if configured)
    pub fn flush(&self) -> NativeResult<()> {
        let db_path = match &self.db_path {
            Some(path) => path.clone(),
            None => {
                // In-memory mode: just clear dirty clusters
                self.dirty_clusters.write().clear();
                return Ok(());
            }
        };
        
        let mut dirty = self.dirty_clusters.write();
        
        if dirty.is_empty() {
            return Ok(()); // Nothing to flush
        }
        
        // Get mutable access to btree for index updates
        // Note: We use unsafe here because we need to mutate through &self
        // In production, this would use interior mutability patterns
        // For now, we use a simple approach: clone dirty clusters and process them
        let clusters_to_flush: Vec<((i64, Direction), V3EdgeCluster)> = dirty
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        
        // Drop the lock before doing I/O
        drop(dirty);
        
        // Process each dirty cluster
        for ((src, dir), mut cluster) in clusters_to_flush {
            // Serialize cluster to bytes
            let cluster_bytes = cluster.serialize()?;
            
            // For now, use a simple mapping: src node ID -> page ID
            // In full implementation, we'd allocate pages dynamically
            let page_id = (src as u64) * 2 + if dir == Direction::Outgoing { 100 } else { 200 };
            
            // Write cluster data to page on disk
            self.write_page_to_disk(&db_path, page_id, &cluster_bytes)?;
            
            // Update cluster with assigned page_id
            cluster.page_id = page_id;
            
            // Update B+Tree index: map source node ID to page ID
            // Note: We use a simple key scheme where the node ID maps to edge page
            // In full implementation, B+Tree would support composite keys (src, dir)
            {
                let mut btree = self.btree.write();
                // Insert mapping: source node ID -> page ID
                // Using src as the key, page_id as value
                let _ = btree.insert(src, page_id);
            }
        }
        
        // Clear dirty clusters after successful flush
        self.dirty_clusters.write().clear();
        
        // Write WAL checkpoint if configured
        if let Some(ref wal) = self.wal {
            let header = PersistentHeaderV3::new_v3();
            let mut wal_guard = wal.write();
            let btree = self.btree.read();
            // Write checkpoint record
            let _ = wal_guard.checkpoint(
                btree.root_page_id(),
                100, // total_pages - placeholder
                btree.tree_height(),
                0, // free_page_list_head
                &header,
            );
        }
        
        Ok(())
    }
    
    /// Write a page of data to disk
    fn write_page_to_disk(&self, db_path: &PathBuf, page_id: u64, data: &[u8]) -> NativeResult<()> {
        use crate::backend::native::v3::constants::V3_HEADER_SIZE;
        
        // Calculate page offset (page 0 is header, data pages start at 1)
        let offset: u64 = if page_id == 0 {
            0
        } else {
            (V3_HEADER_SIZE as u64) + (page_id - 1) * DEFAULT_PAGE_SIZE
        };
        
        // Open file and write data
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(db_path)
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to open db file for page write: {}", page_id),
                source: e,
            })?;
        
        file.seek(SeekFrom::Start(offset as u64))
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to seek to page {} offset {}", page_id, offset),
                source: e,
            })?;
        
        file.write_all(data)
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to write page {} data", page_id),
                source: e,
            })?;
        
        file.sync_data()
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to sync page {} write", page_id),
                source: e,
            })?;
        
        Ok(())
    }
    
    /// Flush WAL buffer to disk (for durability testing)
    #[cfg(test)]
    pub fn flush_wal(&self) -> NativeResult<()> {
        if let Some(ref wal) = self.wal {
            let mut wal_guard = wal.write();
            wal_guard.flush()
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use std::sync::Arc;
    use parking_lot::RwLock;
    use crate::backend::native::v3::{
        allocator::PageAllocator,
        header::PersistentHeaderV3,
        btree::BTreeManager,
    };
    use tempfile::TempDir;
    use std::path::PathBuf;

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

    //========================================================================
    // TDD Tests for Edge Store Durability TODOs
    // These tests verify the critical production issues:
    // 1. WAL record for edge insert
    // 2. Dirty cluster flush to pages
    // 3. B+Tree index update
    // 4. WAL checkpoint
    //========================================================================

    /// Test helper: Create a V3EdgeStore with WAL for durability testing
    fn create_test_edge_store(db_path: Option<PathBuf>) -> (V3EdgeStore, Arc<RwLock<PageAllocator>>) {
        let header = PersistentHeaderV3::new_v3();
        let allocator = Arc::new(RwLock::new(PageAllocator::new(&header)));
        
        // Create BTreeManager with the allocator
        let btree = if let Some(ref path) = db_path {
            BTreeManager::new(allocator.clone(), None, path.clone())
        } else {
            BTreeManager::new(allocator.clone(), None, None::<PathBuf>)
        };
        
        // Create edge store with or without persistence path
        let edge_store = if let Some(ref path) = db_path {
            // Create WAL writer
            let wal_path = path.with_extension("v3wal");
            let mut writer = WALWriter::new(wal_path, 1).expect("Failed to create WAL writer");
            writer.write_header().expect("Failed to write WAL header");
            V3EdgeStore::with_path(btree, Some(writer), path.clone())
        } else {
            V3EdgeStore::new(btree, None)
        };
        
        (edge_store, allocator)
    }

    /// TODO Test 1: Edge insert should write WAL record for durability
    /// 
    /// CRITICAL: This test verifies that insert_edge() creates a proper WAL record.
    /// Without this, edges inserted via cache are lost on crash.
    #[test]
    fn test_edge_insert_creates_wal_record() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.graph");
        let wal_path = db_path.with_extension("v3wal");
        
        // Create edge store with WAL
        let (edge_store, _allocator) = create_test_edge_store(Some(db_path.clone()));
        
        // Insert an edge - this should create a WAL record
        edge_store.insert_edge(1, 2, Direction::Outgoing).expect("Insert failed");
        
        // Flush WAL to ensure record is written
        edge_store.flush_wal().expect("WAL flush failed");
        
        // CRITICAL TODO FIX: Verify WAL file exists and contains edge insert record
        // Currently this fails because insert_edge() does NOT write WAL records
        assert!(
            wal_path.exists(),
            "CRITICAL TODO: WAL file should exist after edge insert with WAL enabled"
        );
        
        // Read WAL and verify edge insert record exists
        let wal_content = std::fs::read(&wal_path).expect("Failed to read WAL file");
        assert!(
            wal_content.len() > 64, // Header is 64 bytes, records add more
            "CRITICAL TODO: WAL should contain edge insert record beyond header"
        );
        
        // TODO: Parse WAL and verify edge-specific record type exists
        // This requires implementing EdgeInsert record type in WAL
    }

    /// TODO Test 2: Flush should write dirty clusters to pages
    ///
    /// CRITICAL: This test verifies that flush() actually persists edge data.
    /// Currently flush() is a no-op that returns Ok(()) immediately.
    #[test]
    fn test_flush_writes_dirty_clusters_to_pages() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.graph");
        
        // Create the database file first
        std::fs::write(&db_path, vec![0u8; 4096]).expect("Failed to create db file");
        
        // Create edge store with disk persistence
        let (edge_store, _allocator) = create_test_edge_store(Some(db_path.clone()));
        
        // Insert edges into cache
        edge_store.insert_edge(1, 2, Direction::Outgoing).expect("Insert 1->2 failed");
        edge_store.insert_edge(1, 3, Direction::Outgoing).expect("Insert 1->3 failed");
        edge_store.insert_edge(2, 4, Direction::Outgoing).expect("Insert 2->4 failed");
        
        // Flush should write dirty clusters to disk pages
        let result = edge_store.flush();
        assert!(result.is_ok(), "Flush should succeed");
        
        // CRITICAL TODO FIX: After flush, edge data should be on disk
        // Currently this fails because flush() does nothing
        let file_size = std::fs::metadata(&db_path)
            .expect("Failed to read file metadata")
            .len();
        
        assert!(
            file_size > 4096,
            "CRITICAL TODO: Database file should grow after flush writes dirty clusters"
        );
        
        // Verify we can read back the edges after reopening
        // This requires implementing cluster deserialization from pages
    }

    /// TODO Test 3: Flush should update B+Tree index
    ///
    /// CRITICAL: The B+Tree index maps (src_node_id, direction) -> page_id.
    /// Without this update, edge lookups will fail after recovery.
    #[test]
    fn test_flush_updates_btree_index() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.graph");
        
        // Create database file
        std::fs::write(&db_path, vec![0u8; 4096]).expect("Failed to create db file");
        
        // Create edge store
        let (edge_store, _allocator) = create_test_edge_store(Some(db_path.clone()));
        
        // Insert edges for node 1
        edge_store.insert_edge(1, 2, Direction::Outgoing).expect("Insert failed");
        edge_store.insert_edge(1, 3, Direction::Outgoing).expect("Insert failed");
        
        // Flush should update B+Tree index
        edge_store.flush().expect("Flush failed");
        
        // CRITICAL TODO FIX: B+Tree should contain mapping for node 1
        // Currently btree only tracks node_id -> page_id, not edge lookups
        // Need to implement (src, direction) composite key lookup
        
        // After fix: verify B+Tree contains edge cluster mapping
        let btree = edge_store.btree.read();
        let lookup_result = btree.lookup(1); // Looking up node 1's edge page
        
        assert!(
            lookup_result.is_ok(),
            "CRITICAL TODO: B+Tree lookup should succeed"
        );
        assert!(
            lookup_result.unwrap().is_some(),
            "CRITICAL TODO: B+Tree should contain edge page mapping for node 1 after flush"
        );
    }

    /// TODO Test 4: WAL checkpoint should truncate WAL after successful flush
    ///
    /// CRITICAL: After flush() persists data to pages, WAL should be checkpointed
    /// to enable truncation and prevent unbounded WAL growth.
    #[test]
    fn test_wal_checkpoint_after_flush() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.graph");
        let wal_path = db_path.with_extension("v3wal");
        
        // Create database file
        std::fs::write(&db_path, vec![0u8; 4096]).expect("Failed to create db file");
        
        // Create edge store with WAL
        let (edge_store, _allocator) = create_test_edge_store(Some(db_path.clone()));
        
        // Insert and flush multiple times
        for i in 0..5 {
            edge_store.insert_edge(1, i as i64 + 10, Direction::Outgoing)
                .expect(&format!("Insert iteration {} failed", i));
            edge_store.flush().expect("Flush failed");
        }
        
        // CRITICAL TODO FIX: After checkpoint, WAL should be truncated or checkpointed
        // Currently no checkpoint happens
        
        // Verify WAL contains checkpoint record
        // For now, just verify WAL exists and has content
        assert!(wal_path.exists(), "WAL file should exist");
        
        // After implementing checkpoint: verify WAL is truncated or has checkpoint record
        // let wal_content = std::fs::read(&wal_path).expect("Failed to read WAL");
        // Parse for checkpoint record type...
    }

    /// Test 5: Edge data should survive crash (recovery test)
    ///
    /// CRITICAL: This test verifies that edges persisted to disk can be recovered
    /// after reopening the edge store.
    #[test]
    fn test_edge_recovery_after_crash() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.graph");
        let wal_path = db_path.with_extension("v3wal");
        
        // Create database file
        std::fs::write(&db_path, vec![0u8; 4096]).expect("Failed to create db file");
        
        // Phase 1: Create edges and persist to disk
        {
            let (edge_store, _allocator) = create_test_edge_store(Some(db_path.clone()));
            
            edge_store.insert_edge(1, 2, Direction::Outgoing).expect("Insert failed");
            edge_store.insert_edge(1, 3, Direction::Outgoing).expect("Insert failed");
            edge_store.insert_edge(2, 4, Direction::Outgoing).expect("Insert failed");
            
            // CRITICAL: Call flush() to write dirty clusters to disk pages
            // This ensures data survives after the edge store is dropped
            edge_store.flush().expect("Flush failed");
            
            // Also flush WAL for durability
            edge_store.flush_wal().expect("WAL flush failed");
        }
        
        // Verify WAL exists with content
        assert!(
            wal_path.exists(),
            "WAL file should exist after inserts with WAL enabled"
        );
        
        // Phase 2: "Recover" by creating new edge store
        // The new store should load edges from disk on cache miss
        {
            let (recovered_store, _allocator) = create_test_edge_store(Some(db_path.clone()));
            
            // Load neighbors for node 1 - should read from disk since cache is empty
            let neighbors = recovered_store.outgoing(1).expect("Failed to get neighbors");
            
            // After implementing disk read, this should return the persisted edges
            assert!(
                neighbors.len() >= 2,
                "After recovery, node 1 should have at least 2 outgoing neighbors"
            );
            
            // Verify specific neighbors are present
            let neighbor_vec: Vec<i64> = neighbors.iter().copied().collect();
            assert!(neighbor_vec.contains(&2), "Node 1 should have edge to node 2");
            assert!(neighbor_vec.contains(&3), "Node 1 should have edge to node 3");
        }
    }

    /// TODO Test 6: Empty flush should not error
    ///
    /// Edge case: flush() with no dirty clusters should succeed gracefully.
    #[test]
    fn test_flush_with_no_dirty_clusters() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.graph");
        
        // Create edge store without inserting any edges
        let (edge_store, _allocator) = create_test_edge_store(Some(db_path));
        
        // Flush with empty cache should succeed
        let result = edge_store.flush();
        assert!(result.is_ok(), "Flush with empty cache should succeed");
    }

    /// TODO Test 7: Multiple flushes should be idempotent
    ///
    /// Calling flush() multiple times should not corrupt data.
    #[test]
    fn test_multiple_flushes_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.graph");
        std::fs::write(&db_path, vec![0u8; 4096]).expect("Failed to create db file");
        
        let (edge_store, _allocator) = create_test_edge_store(Some(db_path.clone()));
        
        // Insert edges
        edge_store.insert_edge(1, 2, Direction::Outgoing).expect("Insert failed");
        
        // Flush multiple times
        for _ in 0..3 {
            edge_store.flush().expect("Flush failed");
        }
        
        // After implementing flush: verify edges are still queryable
        // Currently this just verifies no panic occurs
    }

    //========================================================================
    // End TDD Tests
    //========================================================================
}
