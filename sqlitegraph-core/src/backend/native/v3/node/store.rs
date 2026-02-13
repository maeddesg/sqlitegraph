//! NodeStore V3 - B+Tree Lookup Integration (Task 66-01)
//!
//! This module implements NodeStore for O(log n) node_id -> page_id lookups
//! using B+Tree index traversal. Includes TraversalCache for per-traversal
//! page caching.
//!
//! # Architecture
//!
//! 1. **NodeStore**: B+Tree-based node lookup with page caching
//! 2. **TraversalCache**: LRU cache for NodePage instances
//! 3. **Page Loading**: Loads NodePage from disk using page_id
//! 4. **Decompression**: Extracts NodeRecordV3 from compressed page data
//!
//! # B+Tree Lookup Algorithm
//!
//! The B+Tree provides O(log n) lookup:
//! - Start at root page
//! - For internal pages: binary search keys, descend to child
//! - For leaf pages: binary search entries for exact match

use crate::backend::native::NativeBackendError;
use crate::backend::native::NativeResult;
use crate::backend::native::v3::allocator::PageAllocator;
use crate::backend::native::v3::btree::BTreeManager;
use crate::backend::native::v3::constants::{DEFAULT_PAGE_SIZE, V3_HEADER_SIZE};
use crate::backend::native::v3::header::PersistentHeaderV3;
use crate::backend::native::v3::index::IndexPage;
use crate::backend::native::v3::node::{NodePage, NodeRecordV3};
use crate::backend::native::v3::wal::WALWriter;
use crate::backend::native::types::NodeFlags;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;

//=============================================================================
// Constants
//=============================================================================

/// Maximum B+Tree height for safety
const MAX_TREE_HEIGHT: u32 = 10;

/// Page cache size for NodeStore
const PAGE_CACHE_SIZE: usize = 16;

/// Default capacity for TraversalCache
pub const DEFAULT_CACHE_CAPACITY: usize = 16;

/// Maximum capacity for TraversalCache
pub const MAX_CACHE_CAPACITY: usize = 256;

/// Minimum capacity for TraversalCache
pub const MIN_CACHE_CAPACITY: usize = 1;

//=============================================================================
// TraversalCache: LRU cache for NodePage instances
//=============================================================================

/// LRU cache for NodePage instances during graph traversal
#[derive(Debug, Clone)]
pub struct TraversalCache {
    cache: HashMap<u64, Arc<NodePage>>,
    access_order: Vec<u64>,
    capacity: usize,
    hits: u64,
    misses: u64,
}

impl TraversalCache {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity >= MIN_CACHE_CAPACITY && capacity <= MAX_CACHE_CAPACITY);
        Self {
            cache: HashMap::with_capacity(capacity),
            access_order: Vec::with_capacity(capacity),
            capacity,
            hits: 0,
            misses: 0,
        }
    }

    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_CACHE_CAPACITY)
    }

    pub fn get(&mut self, page_id: u64) -> Option<Arc<NodePage>> {
        if let Some(page) = self.cache.remove(&page_id) {
            self.access_order.retain(|&id| id != page_id);
            self.access_order.push(page_id);
            self.cache.insert(page_id, page.clone());
            self.hits += 1;
            Some(page)
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn insert(&mut self, page_id: u64, page: Arc<NodePage>) {
        if self.cache.contains_key(&page_id) {
            self.access_order.retain(|&id| id != page_id);
        }
        while self.cache.len() >= self.capacity {
            if let Some(oldest_id) = self.access_order.first() {
                self.cache.remove(&oldest_id);
                self.access_order.remove(0);
            } else {
                break;
            }
        }
        self.access_order.push(page_id);
        self.cache.insert(page_id, page);
    }

    pub fn invalidate(&mut self, page_id: u64) -> bool {
        let was_present = self.cache.remove(&page_id).is_some();
        self.access_order.retain(|&id| id != page_id);
        was_present
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.access_order.clear();
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn contains(&self, page_id: &u64) -> bool {
        self.cache.contains_key(page_id)
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }

    pub fn misses(&self) -> u64 {
        self.misses
    }

    pub fn hit_ratio(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        self.hits as f64 / total as f64
    }

    pub fn reset_stats(&mut self) {
        self.hits = 0;
        self.misses = 0;
    }
}

impl Default for TraversalCache {
    fn default() -> Self {
        Self::with_default_capacity()
    }
}

//=============================================================================
// NodeStore: B+Tree Lookup Integration
//=============================================================================

/// NodeStore for B+Tree-based node lookup
pub struct NodeStore {
    db_path: PathBuf,
    root_page_id: u64,
    tree_height: u32,
    total_pages: u64,
    page_cache: HashMap<u64, Vec<u8>>,
    cache_capacity: usize,
    index_cache: HashMap<u64, IndexPage>,
    /// B+Tree manager for index operations
    btree_manager: Option<BTreeManager>,
    /// Page allocator for page management (shared with BTreeManager)
    page_allocator: Option<Arc<RwLock<PageAllocator>>>,
    /// Optional WAL writer for durability
    wal_writer: Option<WALWriter>,
    /// Next available node ID
    next_node_id: i64,
    /// Dirty page buffer for batch writes (page_id -> NodePage)
    dirty_pages: HashMap<u64, NodePage>,
    /// Whether batch mode is active (defer disk writes)
    batch_mode: bool,
}

impl NodeStore {
    pub fn new(header: &PersistentHeaderV3, db_path: PathBuf) -> Self {
        NodeStore {
            db_path,
            root_page_id: header.root_index_page,
            tree_height: header.btree_height,
            total_pages: header.total_pages,
            page_cache: HashMap::with_capacity(PAGE_CACHE_SIZE),
            cache_capacity: PAGE_CACHE_SIZE,
            index_cache: HashMap::with_capacity(PAGE_CACHE_SIZE),
            btree_manager: None,
            page_allocator: None,
            wal_writer: None,
            next_node_id: 1, // Start from 1
            dirty_pages: HashMap::new(),
            batch_mode: false,
        }
    }

    pub fn with_capacity(header: &PersistentHeaderV3, db_path: PathBuf, cache_capacity: usize) -> Self {
        NodeStore {
            db_path,
            root_page_id: header.root_index_page,
            tree_height: header.btree_height,
            total_pages: header.total_pages,
            page_cache: HashMap::with_capacity(cache_capacity),
            cache_capacity,
            index_cache: HashMap::with_capacity(cache_capacity),
            btree_manager: None,
            page_allocator: None,
            wal_writer: None,
            next_node_id: 1,
            dirty_pages: HashMap::new(),
            batch_mode: false,
        }
    }

    /// Initialize the store with BTreeManager, PageAllocator and optional WAL
    pub fn initialize(
        &mut self,
        btree_manager: BTreeManager,
        page_allocator: Arc<RwLock<PageAllocator>>,
        wal_writer: Option<WALWriter>,
    ) {
        self.btree_manager = Some(btree_manager);
        self.page_allocator = Some(page_allocator);
        self.wal_writer = wal_writer;
    }

    /// Set the WAL writer
    pub fn set_wal_writer(&mut self, wal: WALWriter) {
        self.wal_writer = Some(wal);
    }
    
    /// Enable batch mode (defer disk writes until commit)
    ///
    /// When batch mode is enabled, page writes are staged in memory
    /// and flushed to disk with a single fsync on commit.
    pub fn begin_batch(&mut self) {
        self.batch_mode = true;
        self.dirty_pages.clear();
    }
    
    /// Commit all staged dirty pages with single fsync
    ///
    /// Returns the number of pages flushed.
    pub fn commit_batch(&mut self) -> NativeResult<usize> {
        if !self.batch_mode {
            return Ok(0);
        }
        
        let page_count = self.dirty_pages.len();
        
        if page_count > 0 {
            // Open file once for all writes
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .open(&self.db_path)
                .map_err(|e| NativeBackendError::IoError {
                    context: format!("Failed to open database file for batch commit: {}", self.db_path.display()),
                    source: e,
                })?;
            
            // Write all dirty pages
            for (page_id, page) in &self.dirty_pages {
                let page_bytes = page.pack()?;
                let offset = Self::page_offset(*page_id);
                
                file.seek(SeekFrom::Start(offset)).map_err(|e| NativeBackendError::IoError {
                    context: format!("Failed to seek to page {} offset {}", page_id, offset),
                    source: e,
                })?;
                
                file.write_all(&page_bytes).map_err(|e| NativeBackendError::IoError {
                    context: format!("Failed to write page {} during batch commit", page_id),
                    source: e,
                })?;
                
                // Update cache
                self.page_cache.insert(*page_id, page_bytes.to_vec());
            }
            
            // Single fsync for all pages
            file.sync_all().map_err(|e| NativeBackendError::IoError {
                context: "Failed to sync batch commit to disk".to_string(),
                source: e,
            })?;
            
            // Clear dirty pages
            self.dirty_pages.clear();
        }
        
        self.batch_mode = false;
        Ok(page_count)
    }
    
    /// Rollback batch - discard staged pages without writing
    pub fn rollback_batch(&mut self) {
        self.dirty_pages.clear();
        self.batch_mode = false;
    }
    
    /// Check if batch mode is active
    pub fn is_batch_mode(&self) -> bool {
        self.batch_mode
    }
    
    /// Get count of dirty pages staged for commit
    pub fn dirty_page_count(&self) -> usize {
        self.dirty_pages.len()
    }

    /// Get mutable reference to BTreeManager
    fn btree_manager_mut(&mut self) -> NativeResult<&mut BTreeManager> {
        self.btree_manager.as_mut().ok_or_else(|| NativeBackendError::InvalidHeader {
            field: "btree_manager".to_string(),
            reason: "BTreeManager not initialized".to_string(),
        })
    }

    /// Get reference to BTreeManager
    fn btree_manager(&self) -> NativeResult<&BTreeManager> {
        self.btree_manager.as_ref().ok_or_else(|| NativeBackendError::InvalidHeader {
            field: "btree_manager".to_string(),
            reason: "BTreeManager not initialized".to_string(),
        })
    }

    /// Get write lock on PageAllocator
    fn page_allocator_mut(&self) -> NativeResult<parking_lot::RwLockWriteGuard<'_, PageAllocator>> {
        self.page_allocator.as_ref()
            .ok_or_else(|| NativeBackendError::InvalidHeader {
                field: "page_allocator".to_string(),
                reason: "PageAllocator not initialized".to_string(),
            })
            .map(|arc| arc.write())
    }

    /// Get read lock on PageAllocator
    fn page_allocator(&self) -> NativeResult<parking_lot::RwLockReadGuard<'_, PageAllocator>> {
        self.page_allocator.as_ref()
            .ok_or_else(|| NativeBackendError::InvalidHeader {
                field: "page_allocator".to_string(),
                reason: "PageAllocator not initialized".to_string(),
            })
            .map(|arc| arc.read())
    }

    /// Get the next available node ID and increment
    pub fn allocate_node_id(&mut self) -> i64 {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }

    /// Set the next node ID (for recovery/initialization)
    pub fn set_next_node_id(&mut self, next_id: i64) {
        self.next_node_id = next_id;
    }

    /// Insert a new node into the store
    ///
    /// # Arguments
    ///
    /// * `node` - The node record to insert (node.id will be assigned)
    ///
    /// # Returns
    ///
    /// * `Ok(node_id)` - The assigned node ID
    /// * `Err(...)` - Error during insert
    pub fn insert_node(&mut self, mut node: NodeRecordV3) -> NativeResult<i64> {
        // 1. Allocate new node_id
        let node_id = self.allocate_node_id();
        node.id = node_id;

        // 2. Find or create a page for the node
        let page_id = self.find_or_create_page_for_node(&node)?;

        // 3. Load the page
        let mut page = self.load_node_page(page_id)?;

        // 4. Add node to page
        page.add_node(node)?;

        // 5. Write page back to disk (or stage in batch mode)
        self.write_node_page(&page)?;

        // 6. Update B+Tree index (node_id -> page_id)
        let btree = self.btree_manager_mut()?;
        btree.insert(node_id, page_id)?;

        // 7. Log to WAL if configured (skip in batch mode - will be handled at commit)
        if !self.batch_mode {
            if let Some(ref mut wal) = self.wal_writer {
                let page_bytes = page.pack()?;
                wal.page_write(page_id, 0, page_bytes.to_vec())?;
            }
            
            // 8. Update cache (only in immediate mode; batch updates cache at commit)
            self.page_cache.insert(page_id, page.pack()?.to_vec());
        }

        Ok(node_id)
    }

    /// Update an existing node
    ///
    /// # Arguments
    ///
    /// * `node_id` - The ID of the node to update
    /// * `node` - The new node record data
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Update successful
    /// * `Err(...)` - Error during update or node not found
    pub fn update_node(&mut self, node_id: i64, node: NodeRecordV3) -> NativeResult<()> {
        // 1. Lookup existing page
        let page_id = match self.lookup_page(node_id)? {
            Some(pid) => pid,
            None => {
                return Err(NativeBackendError::InvalidHeader {
                    field: "update_node".to_string(),
                    reason: format!("Node {} not found", node_id),
                });
            }
        };

        // 2. Load the page
        let mut page = self.load_node_page(page_id)?;

        // 3. Find and replace the node
        let mut found = false;
        for (i, existing_node) in page.nodes.iter().enumerate() {
            if existing_node.id() == node_id {
                page.nodes[i] = node;
                found = true;
                break;
            }
        }

        if !found {
            return Err(NativeBackendError::InvalidHeader {
                field: "update_node".to_string(),
                reason: format!("Node {} not found in page {}", node_id, page_id),
            });
        }

        // 4. Recalculate used_bytes
        page.used_bytes = page.nodes.iter()
            .map(|n| self.estimate_node_size(n))
            .sum::<NativeResult<u16>>()?;

        // 5. Write page back to disk
        self.write_node_page(&page)?;

        // 6. Log to WAL if configured
        if let Some(ref mut wal) = self.wal_writer {
            let page_bytes = page.pack()?;
            wal.page_write(page_id, 0, page_bytes.to_vec())?;
        }

        // 7. Update cache
        self.page_cache.insert(page_id, page.pack()?.to_vec());

        Ok(())
    }

    /// Delete a node (soft delete with tombstone)
    ///
    /// # Arguments
    ///
    /// * `node_id` - The ID of the node to delete
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Node was found and deleted
    /// * `Ok(false)` - Node was not found
    /// * `Err(...)` - Error during delete
    pub fn delete_node(&mut self, node_id: i64) -> NativeResult<bool> {
        // 1. Lookup page
        let page_id = match self.lookup_page(node_id)? {
            Some(pid) => pid,
            None => return Ok(false),
        };

        // 2. Load the page
        let mut page = self.load_node_page(page_id)?;

        // 3. Find and mark as deleted (tombstone)
        let mut found = false;
        for node in page.nodes.iter_mut() {
            if node.id() == node_id {
                node.flags = NodeFlags::DELETED;
                found = true;
                break;
            }
        }

        if !found {
            return Ok(false);
        }

        // 4. Write page back to disk
        self.write_node_page(&page)?;

        // 5. Remove from B+Tree index
        let btree = self.btree_manager_mut()?;
        btree.delete(node_id)?;

        // 6. Log to WAL if configured
        if let Some(ref mut wal) = self.wal_writer {
            let page_bytes = page.pack()?;
            wal.page_write(page_id, 0, page_bytes.to_vec())?;
        }

        // 7. Update cache
        self.page_cache.insert(page_id, page.pack()?.to_vec());

        Ok(true)
    }

    /// Find or create a page for a new node
    fn find_or_create_page_for_node(&mut self, node: &NodeRecordV3) -> NativeResult<u64> {
        // Try to find an existing page with space
        let node_size = self.estimate_node_size(node)?;

        // First, check dirty pages (batch mode - most recent changes)
        for (&page_id, page) in &self.dirty_pages {
            let cap = page.capacity();
            if cap >= node_size {
                return Ok(page_id);
            }
        }

        // Next, check the page cache (skip pages already in dirty_pages)
        for (&page_id, page_bytes) in &self.page_cache {
            // Skip if this page is already in dirty_pages (stale cache entry)
            if self.dirty_pages.contains_key(&page_id) {
                continue;
            }
            if let Ok(page) = NodePage::unpack(page_bytes) {
                let cap = page.capacity();
                if cap >= node_size {
                    return Ok(page_id);
                }
            }
        }

        // Allocate a new page
        let new_page_id = {
            let mut allocator = self.page_allocator_mut()?;
            allocator.allocate()?
        };
        
        // Create empty page in cache (NOT in dirty_pages - it's not dirty yet)
        let new_page = NodePage::new(new_page_id);
        let page_bytes = new_page.pack()?;
        self.page_cache.insert(new_page_id, page_bytes.to_vec());

        Ok(new_page_id)
    }

    /// Load a NodePage from disk
    fn load_node_page(&mut self, page_id: u64) -> NativeResult<NodePage> {
        // Check dirty pages first (read-your-own-writes during batch)
        if let Some(page) = self.dirty_pages.get(&page_id) {
            return Ok(page.clone());
        }
        
        // Try cache next
        if let Some(cached) = self.page_cache.get(&page_id) {
            return NodePage::unpack(cached);
        }

        // Load from disk
        let page_bytes = self.load_page_from_disk(page_id)?;
        NodePage::unpack(&page_bytes)
    }

    /// Write a NodePage to disk
    fn write_node_page(&mut self, page: &NodePage) -> NativeResult<()> {
        let page_id = page.page_id;
        
        // In batch mode, stage to dirty_pages instead of writing immediately
        if self.batch_mode {
            self.dirty_pages.insert(page_id, page.clone());
            return Ok(());
        }
        
        // Immediate write mode (original behavior)
        let page_bytes = page.pack()?;
        let offset = Self::page_offset(page_id);

        // Open file for writing
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&self.db_path)
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to open database file for writing: {}", self.db_path.display()),
                source: e,
            })?;

        // Seek to position
        file.seek(SeekFrom::Start(offset)).map_err(|e| NativeBackendError::IoError {
            context: format!("Failed to seek to page {} offset {}", page_id, offset),
            source: e,
        })?;

        // Write page data
        file.write_all(&page_bytes).map_err(|e| NativeBackendError::IoError {
            context: format!("Failed to write page {}", page_id),
            source: e,
        })?;

        // Sync to disk
        file.sync_all().map_err(|e| NativeBackendError::IoError {
            context: format!("Failed to sync page {} to disk", page_id),
            source: e,
        })?;

        Ok(())
    }

    /// Estimate the compressed size of a node record
    fn estimate_node_size(&self, node: &NodeRecordV3) -> NativeResult<u16> {
        use crate::backend::native::v3::compression::delta::encode_id_delta;
        use crate::backend::native::v3::compression::varint::varint_size;

        let mut size: usize = 0;
        let base_id = 0; // Conservative estimate

        // ID delta (varint, usually 1-4 bytes)
        let delta = encode_id_delta(node.id(), base_id);
        size += varint_size(delta as u64);

        // Flags: 4 bytes (fixed)
        size += 4;

        // kind_offset: varint u16 (usually 1-2 bytes)
        size += varint_size(node.kind_offset as u64);

        // name_offset: varint u16 (usually 1-2 bytes)
        size += varint_size(node.name_offset as u64);

        // data_len: varint u16 (usually 1 byte for small data)
        size += varint_size(node.data_len() as u64);

        // outgoing_cluster_offset: varint u64 (1-10 bytes)
        size += varint_size(node.outgoing_cluster_offset);

        // outgoing_edge_count: varint u32 (usually 1-3 bytes)
        size += varint_size(node.outgoing_edge_count as u64);

        // incoming_cluster_offset: varint u64 (1-10 bytes)
        size += varint_size(node.incoming_cluster_offset);

        // incoming_edge_count: varint u32 (usually 1-3 bytes)
        size += varint_size(node.incoming_edge_count as u64);

        // Inline data (if any)
        if let Some(ref data) = node.data_inline {
            size += data.len();
        }

        // Ensure we don't overflow u16
        if size > u16::MAX as usize {
            return Err(NativeBackendError::InvalidHeader {
                field: "compressed_size".to_string(),
                reason: format!("compressed size {} exceeds u16::MAX", size),
            });
        }

        Ok(size as u16)
    }

    pub fn has_index(&self) -> bool {
        self.root_page_id != 0
    }

    pub fn root_page_id_pub(&self) -> u64 {
        self.root_page_id
    }

    pub fn tree_height_pub(&self) -> u32 {
        self.tree_height
    }

    pub fn lookup_page(&mut self, node_id: i64) -> NativeResult<Option<u64>> {
        // Use BTreeManager for lookup if available
        if let Some(ref btree) = self.btree_manager {
            return btree.lookup(node_id);
        }
        
        // Fallback to direct B+Tree traversal (for backward compatibility)
        if self.root_page_id == 0 {
            return Ok(None);
        }

        let search_key = node_id as u64;
        let mut current_page_id = self.root_page_id;
        let mut depth = 0;

        while depth < self.tree_height as usize + MAX_TREE_HEIGHT as usize {
            let index_page = if let Some(cached) = self.index_cache.get(&current_page_id) {
                cached.clone()
            } else {
                let page_bytes = self.load_page_from_disk(current_page_id)?;
                let index = IndexPage::unpack(&page_bytes)?;
                self.evict_index_cache_if_needed();
                self.index_cache.insert(current_page_id, index.clone());
                index
            };

            match index_page {
                IndexPage::Leaf { entries, next_leaf, .. } => {
                    let result = IndexPage::binary_search_leaf(&entries, search_key);
                    return match result {
                        Ok(idx) => {
                            if let Some((_, page_id)) = entries.get(idx) {
                                Ok(Some(*page_id))
                            } else {
                                Err(NativeBackendError::InvalidHeader {
                                    field: "btree_leaf".to_string(),
                                    reason: "entry index out of bounds".to_string(),
                                })
                            }
                        }
                        Err(idx) => {
                            if next_leaf == 0 {
                                Ok(None)
                            } else {
                                current_page_id = next_leaf;
                                continue;
                            }
                        }
                    };
                }
                IndexPage::Internal { keys, children, .. } => {
                    let child_idx = IndexPage::find_child_index(&keys, search_key);
                    if child_idx < children.len() {
                        current_page_id = children[child_idx];
                    } else {
                        return Err(NativeBackendError::InvalidHeader {
                            field: "btree_internal".to_string(),
                            reason: format!("child index {} out of bounds", child_idx),
                        });
                    }
                }
            }

            depth += 1;
        }

        Err(NativeBackendError::InvalidHeader {
            field: "btree_depth".to_string(),
            reason: format!("exceeded maximum depth {}", MAX_TREE_HEIGHT),
        })
    }

    pub fn lookup_node(&mut self, node_id: i64) -> NativeResult<Option<NodeRecordV3>> {
        let page_id = match self.lookup_page(node_id)? {
            Some(pid) => pid,
            None => return Ok(None),
        };

        // Use load_node_page to check dirty_pages first (batch mode support)
        let node_page = self.load_node_page(page_id)?;

        for node in &node_page.nodes {
            if node.id() == node_id {
                return Ok(Some(node.clone()));
            }
        }

        Ok(None)
    }

    fn load_page_from_disk(&mut self, page_id: u64) -> NativeResult<Vec<u8>> {
        if let Some(cached) = self.page_cache.get(&page_id) {
            return Ok(cached.clone());
        }

        let page_offset = Self::page_offset(page_id);

        let mut file = File::open(&self.db_path).map_err(|e| {
            NativeBackendError::IoError {
                context: format!("Failed to open database file: {}", self.db_path.display()),
                source: e,
            }
        })?;

        file.seek(SeekFrom::Start(page_offset)).map_err(|e| {
            NativeBackendError::IoError {
                context: format!("Failed to seek to page {}", page_id),
                source: e,
            }
        })?;

        let mut buffer = vec![0u8; DEFAULT_PAGE_SIZE as usize];
        file.read_exact(&mut buffer).map_err(|e| {
            NativeBackendError::IoError {
                context: format!("Failed to read page {}", page_id),
                source: e,
            }
        })?;

        self.evict_page_cache_if_needed();
        self.page_cache.insert(page_id, buffer.clone());

        Ok(buffer)
    }

    fn page_offset(page_id: u64) -> u64 {
        if page_id == 0 {
            return 0;
        }
        let data_page_index = page_id.saturating_sub(1);
        V3_HEADER_SIZE + data_page_index * DEFAULT_PAGE_SIZE
    }

    fn evict_page_cache_if_needed(&mut self) {
        if self.page_cache.len() >= self.cache_capacity {
            // Get first key by cloning a key from the map
            if let Some(key) = self.page_cache.keys().next().copied() {
                self.page_cache.remove(&key);
            }
        }
    }

    fn evict_index_cache_if_needed(&mut self) {
        if self.index_cache.len() >= self.cache_capacity {
            // Get first key by cloning a key from the map
            if let Some(key) = self.index_cache.keys().next().copied() {
                self.index_cache.remove(&key);
            }
        }
    }

    pub fn clear_cache(&mut self) {
        self.page_cache.clear();
        self.index_cache.clear();
    }

    pub fn cache_stats(&self) -> (usize, usize) {
        (self.page_cache.len(), self.index_cache.len())
    }

    pub fn update_root(&mut self, new_root: u64) {
        self.root_page_id = new_root;
        self.index_cache.clear();
    }

    pub fn update_tree_height(&mut self, new_height: u32) {
        self.tree_height = new_height;
    }

    pub fn is_valid_node_id(&self, node_id: i64) -> bool {
        if node_id <= 0 {
            return false;
        }
        if !self.has_index() {
            return false;
        }
        true
    }
}

//=============================================================================
// PageLoader: Load NodePage from disk with decompression
//=============================================================================

/// PageLoader for loading NodePage structures from disk
///
/// Provides efficient page-aligned I/O for reading 4KB NodePages
/// from V3 database files with automatic checksum validation.
pub struct PageLoader {
    /// Underlying V3 database file
    file: Arc<File>,

    /// Page size in bytes (4KB default)
    page_size: usize,

    /// V3 header size for offset calculations
    header_size: u64,
}

impl PageLoader {
    /// Create a new PageLoader for the given file
    ///
    /// # Arguments
    ///
    /// * `file` - V3 database file handle
    /// * `page_size` - Page size in bytes (default 4096)
    ///
    /// # Returns
    ///
    /// Initialized PageLoader ready for page reads
    pub fn new(file: Arc<File>, page_size: usize) -> Self {
        PageLoader {
            file,
            page_size,
            header_size: V3_HEADER_SIZE,
        }
    }

    /// Create a PageLoader with default 4KB page size
    ///
    /// # Arguments
    ///
    /// * `file` - V3 database file handle
    pub fn with_default_page_size(file: Arc<File>) -> Self {
        Self::new(file, DEFAULT_PAGE_SIZE as usize)
    }

    /// Load a full NodePage from disk by page ID
    ///
    /// # Arguments
    ///
    /// * `page_id` - Page ID to load (1-based, 0 = header)
    ///
    /// # Returns
    ///
    /// NodePage with decompressed NodeRecordV3 records
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Page ID is 0 (header page, not a data page)
    /// - I/O error reading page
    /// - Page checksum validation fails
    /// - Page decompression fails
    pub fn load_page(&self, page_id: u64) -> NativeResult<NodePage> {
        // Validate page_id (0 is header, not data page)
        if page_id == 0 {
            return Err(NativeBackendError::InvalidHeader {
                field: "page_id".to_string(),
                reason: "Cannot load header page (page 0) as NodePage".to_string(),
            });
        }

        // Load raw page bytes
        let page_bytes = self.load_page_bytes(page_id)?;

        // Decompress and validate using NodePage::unpack()
        let page = NodePage::unpack(&page_bytes)?;

        Ok(page)
    }

    /// Load raw page bytes from disk
    ///
    /// # Arguments
    ///
    /// * `page_id` - Page ID to load (1-based)
    ///
    /// # Returns
    ///
    /// Raw 4KB page byte array
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Page ID is 0 (header page)
    /// - I/O error reading page
    /// - Read returns incorrect number of bytes
    ///
    /// # Note
    ///
    /// This method only performs I/O and alignment. No checksum
    /// validation or decompression is done - use load_page() for that.
    pub fn load_page_bytes(&self, page_id: u64) -> NativeResult<Vec<u8>> {
        // Validate page_id
        if page_id == 0 {
            return Err(NativeBackendError::InvalidHeader {
                field: "page_id".to_string(),
                reason: "Cannot load header page (page 0) bytes".to_string(),
            });
        }

        // Calculate page offset
        let offset = Self::page_offset(page_id);

        // Clone Arc to get new File reference for I/O
        let mut file = self.file.try_clone()
            .map_err(|_| NativeBackendError::IoError {
                context: "Failed to clone file handle for page read".to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Arc clone failed",
                ),
            })?;

        file.seek(SeekFrom::Start(offset))
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to seek to page {} offset {}", page_id, offset),
                source: e,
            })?;

        // Read page into buffer
        let mut buffer = vec![0u8; self.page_size];

        let bytes_read = file.read(&mut buffer)
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to read page {} at offset {}", page_id, offset),
                source: e,
            })?;

        // Verify we read the full page
        if bytes_read != self.page_size {
            return Err(NativeBackendError::IoError {
                context: format!(
                    "Incomplete page read: expected {} bytes, got {}",
                    self.page_size, bytes_read
                ),
                source: std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Page truncated",
                ),
            });
        }

        Ok(buffer)
    }

    /// Calculate the file offset for a given page ID
    ///
    /// # Arguments
    ///
    /// * `page_id` - Page ID (must be >= 1)
    ///
    /// # Returns
    ///
    /// Byte offset in file where page begins
    ///
    /// # Formula
    ///
    /// ```text
    /// offset = V3_HEADER_SIZE + (page_id - 1) * page_size
    /// ```
    ///
    /// Page 0 is the header (not a data page).
    /// Data pages start at page_id = 1.
    pub fn page_offset(page_id: u64) -> u64 {
        if page_id == 0 {
            return 0;
        }
        let data_page_index = page_id.saturating_sub(1);
        V3_HEADER_SIZE + data_page_index * DEFAULT_PAGE_SIZE
    }

    /// Validate page checksum without full decompression
    ///
    /// # Arguments
    ///
    /// * `page_bytes` - Raw page bytes (typically 4KB)
    ///
    /// # Returns
    ///
    /// Ok(()) if checksum is valid, Err otherwise
    ///
    /// # Note
    ///
    /// This is a lightweight checksum check that reads the checksum
    /// field from the page header and validates it. For full
    /// page loading with decompression, use load_page() instead.
    pub fn validate_page_checksum(&self, page_bytes: &[u8]) -> NativeResult<()> {
        use crate::backend::native::v3::node::page::constants;

        // Ensure we have a full page header
        if page_bytes.len() < constants::PAGE_HEADER_SIZE {
            return Err(NativeBackendError::InvalidHeader {
                field: "page_checksum".to_string(),
                reason: format!(
                    "Insufficient bytes for checksum: need at least {}, got {}",
                    constants::PAGE_HEADER_SIZE,
                    page_bytes.len()
                ),
            });
        }

        // Read stored checksum from page header
        let checksum_offset = constants::CHECKSUM_OFFSET;
        let stored_checksum = u32::from_be_bytes(
            page_bytes[checksum_offset..checksum_offset + 4]
                .try_into()
                .map_err(|_| NativeBackendError::InvalidHeader {
                    field: "checksum".to_string(),
                    reason: "Failed to read checksum bytes".to_string(),
                })?,
        );

        // Calculate checksum over header + node data
        let calculated_checksum = crate::backend::native::v3::constants::checksum::xor_checksum(page_bytes) as u32;

        if calculated_checksum != stored_checksum {
            return Err(NativeBackendError::InvalidChecksum {
                expected: stored_checksum as u64,
                found: calculated_checksum as u64,
            });
        }

        Ok(())
    }

    /// Get the page size
    ///
    /// # Returns
    ///
    /// Page size in bytes
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// Get the V3 header size
    ///
    /// # Returns
    ///
    /// V3 header size in bytes (for offset calculations)
    pub fn header_size(&self) -> u64 {
        self.header_size
    }

    /// Load multiple pages in sequence
    ///
    /// # Arguments
    ///
    /// * `page_ids` - Iterator of page IDs to load
    ///
    /// # Returns
    ///
    /// Vector of NodePage results (pages that failed to load return Err)
    ///
    /// # Note
    ///
    /// Pages are loaded sequentially.
    pub fn load_pages<'a, I>(&self, page_ids: I) -> Vec<NativeResult<NodePage>>
    where
        I: IntoIterator<Item = &'a u64>,
    {
        page_ids
            .into_iter()
            .map(|&page_id| self.load_page(page_id))
            .collect()
    }
}

//=============================================================================
// TraversalCacheBuilder: Builder for TraversalCache
//=============================================================================

/// Builder for creating TraversalCache with custom configuration
pub struct TraversalCacheBuilder {
    capacity: Option<usize>,
}

impl TraversalCacheBuilder {
    pub fn new() -> Self {
        Self { capacity: None }
    }

    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = Some(capacity);
        self
    }

    pub fn build(self) -> NativeResult<TraversalCache> {
        let capacity = self.capacity.unwrap_or(DEFAULT_CACHE_CAPACITY);
        if capacity < MIN_CACHE_CAPACITY || capacity > MAX_CACHE_CAPACITY {
            return Err(NativeBackendError::InvalidParameter {
                context: "TraversalCache capacity".to_string(),
                source: None,
            });
        }
        Ok(TraversalCache::new(capacity))
    }
}

impl Default for TraversalCacheBuilder {
    fn default() -> Self {
        Self::new()
    }
}

//=============================================================================
// Tests
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::native::types::NodeFlags;

    fn test_page(page_id: u64) -> Arc<NodePage> {
        Arc::new(NodePage::new(page_id))
    }

    #[test]
    fn test_cache_creation() {
        let cache = TraversalCache::new(16);
        assert_eq!(cache.capacity(), 16);
    }

    #[test]
    fn test_node_store_new() {
        let header = PersistentHeaderV3::new_v3();
        let db_path = PathBuf::from("/tmp/test.db");
        let store = NodeStore::new(&header, db_path);
        assert_eq!(store.root_page_id_pub(), 0);
    }

    #[test]
    fn test_page_offset_calculation() {
        assert_eq!(NodeStore::page_offset(1), V3_HEADER_SIZE);
        assert_eq!(NodeStore::page_offset(2), V3_HEADER_SIZE + DEFAULT_PAGE_SIZE);
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_TREE_HEIGHT, 10);
        assert_eq!(PAGE_CACHE_SIZE, 16);
    }

    #[test]
    fn test_page_loader_creation() {
        let header = PersistentHeaderV3::new_v3();
        let db_path = PathBuf::from("/tmp/test.db");
        let _ = std::fs::File::create(&db_path).unwrap();
        let file = Arc::new(File::open(&db_path).unwrap());
        let page_size = 4096;

        let loader = PageLoader::new(file.clone(), page_size);
        assert_eq!(loader.page_size(), 4096);
        assert_eq!(loader.header_size(), V3_HEADER_SIZE);

        let loader_default = PageLoader::with_default_page_size(file);
        assert_eq!(loader_default.page_size(), 4096);
    }

    #[test]
    fn test_page_loader_offset_calculation() {
        assert_eq!(PageLoader::page_offset(1), V3_HEADER_SIZE);
        assert_eq!(PageLoader::page_offset(2), V3_HEADER_SIZE + DEFAULT_PAGE_SIZE);
        assert_eq!(PageLoader::page_offset(0), 0);
    }

    #[test]
    fn test_traversal_cache_builder() {
        let builder = TraversalCacheBuilder::new();
        assert!(builder.capacity.is_none());

        let cache = builder.with_capacity(32).build().unwrap();
        assert_eq!(cache.capacity(), 32);
    }

    #[test]
    fn test_traversal_cache_builder_invalid_capacity() {
        let builder = TraversalCacheBuilder::new();
        let result = builder.with_capacity(MAX_CACHE_CAPACITY + 1).build();
        assert!(result.is_err());
    }

    #[test]
    fn test_traversal_cache_builder_default() {
        let cache = TraversalCacheBuilder::default().build().unwrap();
        assert_eq!(cache.capacity(), DEFAULT_CACHE_CAPACITY);
    }
}
