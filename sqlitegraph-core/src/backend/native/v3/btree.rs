//! BTreeManager for V3 native backend
//!
//! Manages B+Tree operations for node_id -> page_id index mapping.
//! Provides O(log n) lookup, insert, and delete operations.
//!
//! ## Architecture
//!
//! 1. **B+Tree Structure**: Internal nodes with keys/children, leaf nodes with entries
//! 2. **Page Management**: Uses PageAllocator for page lifecycle
//! 3. **Durability**: Optional WAL integration for crash recovery
//! 4. **Splitting**: Automatic page splitting when capacity exceeded
//!
//! ## Operations
//!
//! - `lookup`: Traverse B+Tree to find page_id for a node_id
//! - `insert`: Add new key->value mapping, split pages as needed
//! - `delete`: Remove key->value mapping, handle underflow (future)
//! - `split_page`: Split full page into two, propagate up if needed

use crate::backend::native::NativeBackendError;
use crate::backend::native::NativeResult;
use crate::backend::native::v3::allocator::PageAllocator;
use crate::backend::native::v3::constants::{V3_HEADER_SIZE, DEFAULT_PAGE_SIZE};
use crate::backend::native::v3::index::page::{MAX_ENTRIES, MAX_KEYS};
use crate::backend::native::v3::index::IndexPage;
use crate::backend::native::v3::wal::WALWriter;
use crate::backend::native::v3::write_batch::WriteBatch;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

/// Maximum B+Tree height for safety
const MAX_TREE_HEIGHT: u32 = 10;

/// Marker for empty tree (no root page)
const EMPTY_TREE_ROOT: u64 = u64::MAX;

/// BTreeManager for B+Tree index operations
///
/// Manages the B+Tree structure for mapping node_id -> page_id.
/// Uses PageAllocator for page management and optional WAL for durability.
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Clone)]
pub struct BTreeManager {
    /// Page allocator for page lifecycle management (shared with NodeStore)
    allocator: Arc<RwLock<PageAllocator>>,
    /// Optional WAL writer for durability (Arc<RwLock> for Clone + mutable access)
    wal: Option<Arc<RwLock<WALWriter>>>,
    /// Root page ID of the B+Tree (EMPTY_TREE_ROOT if tree is empty)
    root_page_id: u64,
    /// Current tree height (0 for empty tree)
    tree_height: u32,
    /// In-memory page cache for index pages
    page_cache: HashMap<u64, IndexPage>,
    /// Cache capacity
    cache_capacity: usize,
    /// Database file path for disk I/O (None for in-memory/test mode)
    db_path: Option<PathBuf>,
    /// Page size for disk operations
    page_size: u64,
}

impl BTreeManager {
    /// Create a new BTreeManager
    ///
    /// # Arguments
    ///
    /// * `allocator` - Arc<RwLock<PageAllocator>> for shared page management
    /// * `wal` - Optional WALWriter for durability
    /// * `db_path` - Optional path to database file for disk I/O (None for in-memory/test mode)
    ///
    /// # Returns
    ///
    /// New BTreeManager instance with empty tree
    pub fn new<P: Into<Option<PathBuf>>>(allocator: Arc<RwLock<PageAllocator>>, wal: Option<WALWriter>, db_path: P) -> Self {
        Self {
            allocator,
            wal: wal.map(|w| Arc::new(RwLock::new(w))),
            root_page_id: EMPTY_TREE_ROOT,
            tree_height: 0,
            page_cache: HashMap::with_capacity(16),
            cache_capacity: 16,
            db_path: db_path.into(),
            page_size: DEFAULT_PAGE_SIZE,
        }
    }

    /// Create a BTreeManager with an existing root page
    ///
    /// # Arguments
    ///
    /// * `allocator` - Arc<RwLock<PageAllocator>> for shared page management
    /// * `wal` - Optional WALWriter for durability
    /// * `root_page_id` - Existing root page ID
    /// * `tree_height` - Current tree height
    /// * `db_path` - Optional path to database file for disk I/O (None for in-memory/test mode)
    ///
    /// # Returns
    ///
    /// BTreeManager instance with existing tree state
    pub fn with_root<P: Into<Option<PathBuf>>>(
        allocator: Arc<RwLock<PageAllocator>>,
        wal: Option<WALWriter>,
        root_page_id: u64,
        tree_height: u32,
        db_path: P,
    ) -> Self {
        Self {
            allocator,
            wal: wal.map(|w| Arc::new(RwLock::new(w))),
            root_page_id,
            tree_height,
            page_cache: HashMap::with_capacity(16),
            cache_capacity: 16,
            db_path: db_path.into(),
            page_size: DEFAULT_PAGE_SIZE,
        }
    }

    /// Get the root page ID
    pub fn root_page_id(&self) -> u64 {
        self.root_page_id
    }

    /// Get the current tree height
    pub fn tree_height(&self) -> u32 {
        self.tree_height
    }

    /// Check if tree is empty (no root page)
    pub fn is_empty(&self) -> bool {
        self.root_page_id == EMPTY_TREE_ROOT
    }

    /// Lookup page containing key (node_id -> page_id)
    ///
    /// Traverses the B+Tree from root to leaf to find the page_id
    /// associated with the given node_id.
    ///
    /// # Arguments
    ///
    /// * `key` - Node ID to look up
    ///
    /// # Returns
    ///
    /// * `Ok(Some(page_id))` - Found the key, returns associated page_id
    /// * `Ok(None)` - Key not found in tree
    /// * `Err(...)` - Error during lookup
    pub fn lookup(&self, key: i64) -> NativeResult<Option<u64>> {
        if self.root_page_id == EMPTY_TREE_ROOT {
            return Ok(None);
        }

        let search_key = key as u64;
        let mut current_page_id = self.root_page_id;
        let mut depth = 0;

        while depth < MAX_TREE_HEIGHT as usize {
            // Get the index page (from cache or load from disk)
            let index_page = self.load_page(current_page_id)?;

            match &index_page {
                IndexPage::Leaf { entries, next_leaf, .. } => {
                    // Binary search for key in leaf entries
                    let result = IndexPage::binary_search_leaf(entries, search_key);
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
                        Err(_idx) => {
                            // Key not found in this leaf
                            if *next_leaf == 0 {
                                Ok(None)
                            } else {
                                // Continue to next leaf (for range queries, not needed for exact match)
                                current_page_id = *next_leaf;
                                continue;
                            }
                        }
                    };
                }
                IndexPage::Internal { keys, children, .. } => {
                    // Find child index using binary search
                    let child_idx = IndexPage::find_child_index(keys, search_key);
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

    /// Insert key->value mapping into B+Tree
    ///
    /// Inserts a new node_id -> page_id mapping into the B+Tree.
    /// Uses preemptive splitting (top-down) to ensure nodes are never full during insertion.
    ///
    /// # Arguments
    ///
    /// * `key` - Node ID to insert
    /// * `value` - Page ID associated with the node
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Insert successful
    /// * `Err(...)` - Error during insert
    pub fn insert(&mut self, key: i64, value: u64) -> NativeResult<()> {
        // Handle empty tree case
        if self.root_page_id == EMPTY_TREE_ROOT {
            return self.insert_into_empty_tree(key, value);
        }

        let search_key = key as u64;

        // Check if root needs splitting first
        let root_page = self.load_page(self.root_page_id)?;
        if root_page.needs_split_internal() || root_page.needs_split_leaf() {
            self.split_root()?;
        }

        // Descend with preemptive splitting
        self.insert_non_full(self.root_page_id, search_key, value)
    }

    /// Insert into a non-full page, splitting children as needed during descent
    fn insert_non_full(&mut self, page_id: u64, key: u64, value: u64) -> NativeResult<()> {
        let page = self.load_page(page_id)?;

        #[cfg(debug_assertions)]
        page.verify_invariants()?;

        let is_root = page.is_root();
        
        match page {
            IndexPage::Leaf { mut entries, page_id: pid, next_leaf, checksum, .. } => {
                // Check if key already exists (update case)
                match IndexPage::binary_search_leaf(&entries, key) {
                    Ok(idx) => {
                        entries[idx] = (key, value);
                    }
                    Err(idx) => {
                        entries.insert(idx, (key, value));
                    }
                }
                
                // Reconstruct page and write
                let updated_page = IndexPage::Leaf {
                    page_id: pid,
                    entries,
                    next_leaf,
                    checksum,
                    is_root,
                };
                self.write_page(&updated_page)?;
                Ok(())
            }
            IndexPage::Internal { keys, children, .. } => {
                // Find which child to descend to
                let child_idx = IndexPage::find_child_index(&keys, key);
                let child_id = children[child_idx];

                // Load the child and check if it needs splitting
                let child_page = self.load_page(child_id)?;
                
                if child_page.needs_split_internal() || child_page.needs_split_leaf() {
                    // Split the child
                    let (new_child_id, separator_key) = self.split_child(page_id, child_idx)?;
                    
                    // Reload the parent (it was modified by split_child)
                    let updated_parent = self.load_page(page_id)?;
                    
                    // Determine which child to use after split
                    let new_child_idx = if key >= separator_key {
                        child_idx + 1
                    } else {
                        child_idx
                    };
                    
                    if let IndexPage::Internal { children: new_children, .. } = &updated_parent {
                        let next_child_id = new_children[new_child_idx];
                        return self.insert_non_full(next_child_id, key, value);
                    }
                }

                // Descend to the child
                self.insert_non_full(child_id, key, value)
            }
        }
    }

    /// Split the root page, creating a new root
    fn split_root(&mut self) -> NativeResult<()> {
        let old_root_id = self.root_page_id;
        let old_root = self.load_page(old_root_id)?;

        // Allocate new root (internal node)
        let new_root_id = self.allocator.write().allocate()?;
        let mut new_root = IndexPage::new_internal_root(new_root_id);

        // Allocate new sibling page
        let sibling_id = self.allocator.write().allocate()?;

        match &old_root {
            IndexPage::Internal { keys, children, .. } => {
                let split_idx = keys.len() / 2;
                let separator_key = keys[split_idx];

                // Create the new sibling internal node
                let mut sibling = IndexPage::new_internal(sibling_id);
                if let IndexPage::Internal { keys: sib_keys, children: sib_children, .. } = &mut sibling {
                    // Move upper half to sibling (excluding separator)
                    *sib_keys = keys[split_idx + 1..].to_vec();
                    *sib_children = children[split_idx + 1..].to_vec();
                }

                // Truncate old root (keep lower half, excluding separator)
                let mut truncated_old_root = IndexPage::new_internal(old_root_id);
                if let IndexPage::Internal { keys: old_keys, children: old_children, .. } = &mut truncated_old_root {
                    *old_keys = keys[..split_idx].to_vec();
                    *old_children = children[..split_idx + 1].to_vec();
                }

                // Set up new root
                if let IndexPage::Internal { keys: root_keys, children: root_children, .. } = &mut new_root {
                    root_keys.push(separator_key);
                    root_children.push(old_root_id);
                    root_children.push(sibling_id);
                }

                // Write all pages
                self.write_page(&truncated_old_root)?;
                self.write_page(&sibling)?;
                self.write_page(&new_root)?;

                // Update root tracking
                self.root_page_id = new_root_id;
                self.tree_height += 1;
            }
            IndexPage::Leaf { entries, next_leaf, .. } => {
                let split_idx = entries.len() / 2;
                let separator_key = entries[split_idx].0;

                // Create the new sibling leaf
                let mut sibling = IndexPage::new_leaf(sibling_id);
                if let IndexPage::Leaf { entries: sib_entries, next_leaf: sib_next, .. } = &mut sibling {
                    *sib_entries = entries[split_idx..].to_vec();
                    *sib_next = *next_leaf;
                }

                // Truncate old root
                let mut truncated_old_root = IndexPage::new_leaf_root(old_root_id);
                if let IndexPage::Leaf { entries: old_entries, next_leaf: old_next, .. } = &mut truncated_old_root {
                    *old_entries = entries[..split_idx].to_vec();
                    *old_next = sibling_id;
                }

                // Set up new root (internal node with one key)
                if let IndexPage::Internal { keys: root_keys, children: root_children, .. } = &mut new_root {
                    root_keys.push(separator_key);
                    root_children.push(old_root_id);
                    root_children.push(sibling_id);
                }

                // Write all pages
                self.write_page(&truncated_old_root)?;
                self.write_page(&sibling)?;
                self.write_page(&new_root)?;

                // Update root tracking
                self.root_page_id = new_root_id;
                self.tree_height += 1;
            }
        }

        // Log to WAL
        if let Some(ref wal) = self.wal {
            let mut wal_guard = wal.write();
            wal_guard.page_allocate(new_root_id)?;
            let page_bytes = self.load_page(new_root_id)?.pack()?;
            wal_guard.page_write(new_root_id, 0, page_bytes.to_vec())?;
        }

        Ok(())
    }

    /// Split a child page during descent
    /// Returns (new_child_id, separator_key)
    fn split_child(&mut self, parent_id: u64, child_idx: usize) -> NativeResult<(u64, u64)> {
        let parent = self.load_page(parent_id)?;
        let child_id = match &parent {
            IndexPage::Internal { children, .. } => children[child_idx],
            _ => return Err(NativeBackendError::InvalidHeader {
                field: "btree_split_child".to_string(),
                reason: "parent is not an internal node".to_string(),
            }),
        };

        let child = self.load_page(child_id)?;
        let new_page_id = self.allocator.write().allocate()?;

        match &child {
            IndexPage::Internal { keys, children, .. } => {
                let split_idx = keys.len() / 2;
                let separator_key = keys[split_idx];

                // Create sibling internal node
                let mut sibling = IndexPage::new_internal(new_page_id);
                if let IndexPage::Internal { keys: sib_keys, children: sib_children, .. } = &mut sibling {
                    *sib_keys = keys[split_idx + 1..].to_vec();
                    *sib_children = children[split_idx + 1..].to_vec();
                }

                // Truncate original child (keep lower half)
                let mut truncated_child = IndexPage::new_internal(child_id);
                if let IndexPage::Internal { keys: child_keys, children: child_children, .. } = &mut truncated_child {
                    *child_keys = keys[..split_idx].to_vec();
                    *child_children = children[..split_idx + 1].to_vec();
                }

                // Update parent - insert separator and new child
                let mut updated_parent = parent.clone();
                if let IndexPage::Internal { keys: p_keys, children: p_children, .. } = &mut updated_parent {
                    p_keys.insert(child_idx, separator_key);
                    p_children.insert(child_idx + 1, new_page_id);
                }

                // Write all modified pages
                self.write_page(&truncated_child)?;
                self.write_page(&sibling)?;
                self.write_page(&updated_parent)?;

                // Log to WAL
                if let Some(ref wal) = self.wal {
                    let mut wal_guard = wal.write();
                    wal_guard.btree_split(child_id, new_page_id, separator_key, false)?;
                }

                Ok((new_page_id, separator_key))
            }
            IndexPage::Leaf { entries, next_leaf, .. } => {
                let split_idx = entries.len() / 2;
                let separator_key = entries[split_idx].0;

                // Create sibling leaf node
                let mut sibling = IndexPage::new_leaf(new_page_id);
                if let IndexPage::Leaf { entries: sib_entries, next_leaf: sib_next, .. } = &mut sibling {
                    *sib_entries = entries[split_idx..].to_vec();
                    *sib_next = *next_leaf;
                }

                // Truncate original child (keep lower half)
                let mut truncated_child = IndexPage::new_leaf(child_id);
                if let IndexPage::Leaf { entries: child_entries, next_leaf: child_next, .. } = &mut truncated_child {
                    *child_entries = entries[..split_idx].to_vec();
                    *child_next = new_page_id;
                }

                // Update parent - insert separator and new child
                let mut updated_parent = parent.clone();
                if let IndexPage::Internal { keys: p_keys, children: p_children, .. } = &mut updated_parent {
                    p_keys.insert(child_idx, separator_key);
                    p_children.insert(child_idx + 1, new_page_id);
                }

                // Write all modified pages
                self.write_page(&truncated_child)?;
                self.write_page(&sibling)?;
                self.write_page(&updated_parent)?;

                // Log to WAL
                if let Some(ref wal) = self.wal {
                    let mut wal_guard = wal.write();
                    wal_guard.btree_split(child_id, new_page_id, separator_key, true)?;
                }

                Ok((new_page_id, separator_key))
            }
        }
    }

    /// Delete key from B+Tree
    ///
    /// Removes a key->value mapping from the B+Tree.
    /// Returns true if the key was found and deleted, false otherwise.
    ///
    /// # Arguments
    ///
    /// * `key` - Node ID to delete
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Key was found and deleted
    /// * `Ok(false)` - Key was not found
    /// * `Err(...)` - Error during delete
    pub fn delete(&mut self, key: i64) -> NativeResult<bool> {
        if self.root_page_id == EMPTY_TREE_ROOT {
            return Ok(false);
        }

        let search_key = key as u64;
        let leaf_page_id = self.find_leaf(self.root_page_id, search_key)?;

        let mut leaf_page = self.load_page(leaf_page_id)?;

        if let IndexPage::Leaf { entries, .. } = &mut leaf_page {
            match IndexPage::binary_search_leaf(entries, search_key) {
                Ok(idx) => {
                    entries.remove(idx);
                    self.write_page(&leaf_page)?;
                    Ok(true)
                }
                Err(_) => Ok(false), // Key not found
            }
        } else {
            Err(NativeBackendError::InvalidHeader {
                field: "btree_delete".to_string(),
                reason: "expected leaf page".to_string(),
            })
        }
    }

    /// Split page when full
    ///
    /// Splits a full page into two pages and propagates the split
    /// up the tree if necessary.
    ///
    /// # Arguments
    ///
    /// * `page_id` - Page ID to split
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Split successful
    /// * `Err(...)` - Error during split
    fn split_page(&mut self, page_id: u64) -> NativeResult<()> {
        let page = self.load_page(page_id)?;

        match page {
            IndexPage::Leaf { .. } => {
                // For leaf splits, we need the key and value
                // This is handled by split_and_insert_leaf
                Ok(())
            }
            IndexPage::Internal { .. } => {
                // Handle internal page split
                self.split_internal_page(page_id)?;
                Ok(())
            }
        }
    }

    //========================================================================
    // Private helper methods
    //========================================================================

    /// Insert into empty tree (create first leaf page as root)
    fn insert_into_empty_tree(&mut self, key: i64, value: u64) -> NativeResult<()> {
        // Allocate a new page for the root
        let page_id = self.allocator.write().allocate()?;

        // Create a new leaf page as root
        let mut leaf = IndexPage::new_leaf_root(page_id);
        
        // Add the entry
        if let IndexPage::Leaf { entries, .. } = &mut leaf {
            entries.push((key as u64, value));
        }

        // Write the page
        self.write_page(&leaf)?;

        // Update root
        self.root_page_id = page_id;
        self.tree_height = 1;

        // Log to WAL if enabled
        if let Some(ref wal) = self.wal {
            let mut wal_guard = wal.write();
            wal_guard.page_allocate(page_id)?;
            let page_bytes = leaf.pack()?;
            wal_guard.page_write(page_id, 0, page_bytes.to_vec())?;
        }

        Ok(())
    }

    /// Find leaf page for key, tracking path for potential splits
    fn find_leaf_path(
        &self,
        root_page_id: u64,
        search_key: u64,
        path: &mut Vec<(u64, usize)>,
    ) -> NativeResult<u64> {
        let mut current_page_id = root_page_id;
        let mut depth = 0;

        while depth < MAX_TREE_HEIGHT as usize {
            let page = self.load_page(current_page_id)?;

            match &page {
                IndexPage::Leaf { .. } => {
                    return Ok(current_page_id);
                }
                IndexPage::Internal { keys, children, .. } => {
                    let child_idx = IndexPage::find_child_index(keys, search_key);
                    path.push((current_page_id, child_idx));
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

    /// Find leaf page for key (without tracking path)
    fn find_leaf(&self, root_page_id: u64, search_key: u64) -> NativeResult<u64> {
        let mut current_page_id = root_page_id;
        let mut depth = 0;

        while depth < MAX_TREE_HEIGHT as usize {
            let page = self.load_page(current_page_id)?;

            match &page {
                IndexPage::Leaf { .. } => {
                    return Ok(current_page_id);
                }
                IndexPage::Internal { keys, children, .. } => {
                    let child_idx = IndexPage::find_child_index(keys, search_key);
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

    /// Split leaf page and insert new key
    fn split_and_insert_leaf(
        &mut self,
        leaf_page_id: u64,
        key: u64,
        value: u64,
        path: &[(u64, usize)],
    ) -> NativeResult<()> {
        // Allocate new page for the split
        let new_page_id = self.allocator.write().allocate()?;

        // Load and modify the original leaf
        let mut original_page = self.load_page(leaf_page_id)?;
        let mut new_page = IndexPage::new_leaf(new_page_id);

        if let (
            IndexPage::Leaf { entries: orig_entries, next_leaf: orig_next, .. },
            IndexPage::Leaf { entries: new_entries, next_leaf: new_next, .. }
        ) = (&mut original_page, &mut new_page) {
            // Find split point (middle)
            let split_idx = orig_entries.len() / 2;
            
            // Move second half to new page
            *new_entries = orig_entries.split_off(split_idx);
            *new_next = *orig_next;

            // Determine which page gets the new key
            let split_key = new_entries[0].0;
            
            if key < split_key {
                // Insert into original page
                let insert_idx = match IndexPage::binary_search_leaf(orig_entries, key) {
                    Ok(idx) => idx,
                    Err(idx) => idx,
                };
                orig_entries.insert(insert_idx, (key, value));
            } else {
                // Insert into new page
                let insert_idx = match IndexPage::binary_search_leaf(new_entries, key) {
                    Ok(idx) => idx,
                    Err(idx) => idx,
                };
                new_entries.insert(insert_idx, (key, value));
            }

            // Link original page to new page
            *orig_next = new_page_id;
        }

        // Write both pages
        self.write_page(&original_page)?;
        self.write_page(&new_page)?;

        // Get the split key for parent update
        let split_key = if let IndexPage::Leaf { entries, .. } = &new_page {
            entries.first().map(|e| e.0).unwrap_or(key)
        } else {
            key
        };

        // Update parent or create new root
        self.update_parent_after_split(path, split_key, new_page_id)?;

        // Log to WAL
        if let Some(ref wal) = self.wal {
            let mut wal_guard = wal.write();
            wal_guard.btree_split(leaf_page_id, new_page_id, split_key, true)?;
        }

        Ok(())
    }

    /// Split internal page
    fn split_internal_page(&mut self, page_id: u64) -> NativeResult<u64> {
        // Allocate new page
        let new_page_id = self.allocator.write().allocate()?;

        let mut original_page = self.load_page(page_id)?;
        let mut new_page = IndexPage::new_internal(new_page_id);

        if let (
            IndexPage::Internal { keys: orig_keys, children: orig_children, .. },
            IndexPage::Internal { keys: new_keys, children: new_children, .. }
        ) = (&mut original_page, &mut new_page) {
            // Find split point
            let split_idx = orig_keys.len() / 2;
            let split_key = orig_keys[split_idx];

            // Move keys and children after split point to new page
            *new_keys = orig_keys.split_off(split_idx + 1);
            *new_children = orig_children.split_off(split_idx + 1);
            
            // The middle key (split_key) is promoted to parent
            // Remove it from original keys
            orig_keys.pop();

            // Write both pages
            self.write_page(&original_page)?;
            self.write_page(&new_page)?;

            // Log to WAL
            if let Some(ref wal) = self.wal {
                let mut wal_guard = wal.write();
                wal_guard.btree_split(page_id, new_page_id, split_key, false)?;
            }

            return Ok(split_key);
        }

        Err(NativeBackendError::InvalidHeader {
            field: "btree_split".to_string(),
            reason: "failed to split internal page".to_string(),
        })
    }

    /// Update parent after split (or create new root)
    fn update_parent_after_split(
        &mut self,
        path: &[(u64, usize)],
        split_key: u64,
        new_page_id: u64,
    ) -> NativeResult<()> {
        if path.is_empty() {
            // Creating new root
            let new_root_id = self.allocator.write().allocate()?;
            let mut new_root = IndexPage::new_internal(new_root_id);

            if let IndexPage::Internal { keys, children, .. } = &mut new_root {
                keys.push(split_key);
                children.push(self.root_page_id);
                children.push(new_page_id);
            }

            self.write_page(&new_root)?;

            self.root_page_id = new_root_id;
            self.tree_height += 1;

            // Log to WAL
            if let Some(ref wal) = self.wal {
                let mut wal_guard = wal.write();
                wal_guard.page_allocate(new_root_id)?;
                let page_bytes = new_root.pack()?;
                wal_guard.page_write(new_root_id, 0, page_bytes.to_vec())?;
            }

            return Ok(());
        }

        // Update existing parent
        let (parent_id, child_idx) = path[path.len() - 1];
        let mut parent = self.load_page(parent_id)?;

        if let IndexPage::Internal { keys, children, .. } = &mut parent {
            // Insert split key and new child
            keys.insert(child_idx, split_key);
            children.insert(child_idx + 1, new_page_id);

            // Check if parent needs splitting
            if keys.len() > MAX_KEYS {
                // Parent is full, need to split it too
                // For simplicity, we'll handle this recursively
                // In a full implementation, we'd propagate splits upward
                return Err(NativeBackendError::InvalidHeader {
                    field: "btree_parent_split".to_string(),
                    reason: "parent page full - recursive split not yet implemented".to_string(),
                });
            }

            self.write_page(&parent)?;
        }

        Ok(())
    }

    /// Load page from cache or disk
    fn load_page(&self, page_id: u64) -> NativeResult<IndexPage> {
        // Try cache first
        if let Some(page) = self.page_cache.get(&page_id) {
            return Ok(page.clone());
        }

        // If no db_path, we can't load from disk (in-memory/test mode)
        let db_path = match &self.db_path {
            Some(path) => path,
            None => return Err(NativeBackendError::InvalidHeader {
                field: "page_cache".to_string(),
                reason: format!("page {} not in cache (no disk path configured)", page_id),
            }),
        };

        // Page 0 is the header page, data pages start at 1
        if page_id == 0 {
            return Err(NativeBackendError::InvalidHeader {
                field: "page_id".to_string(),
                reason: "Cannot load page 0 (reserved for header)".to_string(),
            });
        }

        // Load from disk
        let offset = V3_HEADER_SIZE + (page_id - 1) * self.page_size;
        let mut file = File::open(db_path).map_err(|e| NativeBackendError::IoError {
            context: format!("Failed to open db file for page load: {}", page_id),
            source: e,
        })?;

        file.seek(SeekFrom::Start(offset)).map_err(|e| NativeBackendError::IoError {
            context: format!("Failed to seek to page {} at offset {}", page_id, offset),
            source: e,
        })?;

        let mut buffer = vec![0u8; self.page_size as usize];
        file.read_exact(&mut buffer).map_err(|e| NativeBackendError::IoError {
            context: format!("Failed to read page {} from disk", page_id),
            source: e,
        })?;

        IndexPage::unpack(&buffer)
    }

    /// Write page to cache and disk (if db_path configured)
    fn write_page(&mut self, page: &IndexPage) -> NativeResult<()> {
        let page_id = page.page_id();
        
        // Serialize page to bytes
        let page_bytes = page.pack()?;
        
        // Write to disk if db_path is configured
        if let Some(db_path) = &self.db_path {
            // Page 0 is the header page, data pages start at 1
            // But IndexPages use 1-based IDs, so page_id 1 is first data page
            if page_id == 0 {
                return Err(NativeBackendError::InvalidHeader {
                    field: "page_id".to_string(),
                    reason: "Cannot write page 0 (reserved for header)".to_string(),
                });
            }
            let offset = V3_HEADER_SIZE + (page_id - 1) * self.page_size;
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(db_path)
                .map_err(|e| NativeBackendError::IoError {
                    context: format!("Failed to open db file for page write: {}", page_id),
                    source: e,
                })?;

            file.seek(SeekFrom::Start(offset)).map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to seek to page {} at offset {}", page_id, offset),
                source: e,
            })?;

            file.write_all(&page_bytes).map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to write page {} to disk", page_id),
                source: e,
            })?;

            file.sync_data().map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to sync page {} write", page_id),
                source: e,
            })?;
        }

        // Update cache
        if self.page_cache.len() >= self.cache_capacity && !self.page_cache.contains_key(&page_id) {
            if let Some(&oldest) = self.page_cache.keys().next() {
                self.page_cache.remove(&oldest);
            }
        }

        self.page_cache.insert(page_id, page.clone());
        Ok(())
    }

    /// Get reference to allocator (read lock)
    pub fn allocator(&self) -> parking_lot::RwLockReadGuard<'_, PageAllocator> {
        self.allocator.read()
    }

    /// Get mutable reference to allocator (write lock)
    pub fn allocator_mut(&self) -> parking_lot::RwLockWriteGuard<'_, PageAllocator> {
        self.allocator.write()
    }

    /// Clear the page cache
    pub fn clear_cache(&mut self) {
        self.page_cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        (self.page_cache.len(), self.cache_capacity)
    }

    /// Create a new write batch for batched page writes
    ///
    /// Use this to amortize fsync costs across multiple operations.
    /// Pages are staged in memory and written with single fsync on commit.
    pub fn create_write_batch(&self) -> WriteBatch {
        WriteBatch::new()
    }

    /// Stage a page write to a batch instead of writing immediately
    pub fn stage_page_to_batch(&self, batch: &mut WriteBatch, page: IndexPage) -> NativeResult<()> {
        batch.stage_page(page)
    }

    /// Commit a write batch to disk with single fsync
    ///
    /// This is the key performance optimization - one fsync for many pages.
    pub fn commit_batch(&mut self, batch: WriteBatch) -> NativeResult<()> {
        let db_path = self.db_path.as_ref().ok_or_else(|| NativeBackendError::InvalidOperation {
            context: "Cannot commit batch: no db_path configured".to_string(),
        })?;

        batch.commit(db_path)
    }

    /// Perform a simple insert that doesn't require splitting, staged to batch
    ///
    /// This is a simplified version for batching that only handles empty trees.
    /// For production use, would need full cache coherency management.
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Insert staged successfully
    /// * `Err(...)` - Error (including non-empty tree)
    pub fn insert_simple_to_batch(&mut self, batch: &mut WriteBatch, key: i64, value: u64) -> NativeResult<bool> {
        // Only handle empty tree case for this demo
        if self.root_page_id == EMPTY_TREE_ROOT {
            // Create new tree with single leaf
            let page_id = self.allocator.write().allocate()?;
            let mut leaf = IndexPage::new_leaf(page_id);
            
            if let IndexPage::Leaf { entries, .. } = &mut leaf {
                entries.push((key as u64, value));
            }
            
            // Stage to batch
            batch.stage_page(leaf.clone())?;
            
            // Also update cache so we can read it back
            self.page_cache.insert(page_id, leaf);
            
            // Update root
            self.root_page_id = page_id;
            self.tree_height = 1;
            
            return Ok(true);
        }

        // For non-empty tree, would need full cache coherency
        Err(NativeBackendError::InvalidOperation {
            context: "Batch insert to non-empty tree not yet implemented".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::native::v3::header::PersistentHeaderV3;

    fn create_test_allocator() -> Arc<RwLock<PageAllocator>> {
        let header = PersistentHeaderV3::new_v3();
        Arc::new(RwLock::new(PageAllocator::new(&header)))
    }

    #[test]
    fn test_btree_manager_new() {
        let allocator = create_test_allocator();
        let manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        assert_eq!(manager.root_page_id(), EMPTY_TREE_ROOT);
        assert_eq!(manager.tree_height(), 0);
        assert!(manager.is_empty());
    }

    #[test]
    fn test_btree_manager_with_root() {
        let allocator = create_test_allocator();
        let manager = BTreeManager::with_root(allocator, None, 1, 1, None::<PathBuf>);

        assert_eq!(manager.root_page_id(), 1);
        assert_eq!(manager.tree_height(), 1);
        assert!(!manager.is_empty());
    }

    #[test]
    fn test_insert_into_empty_tree() {
        let allocator = create_test_allocator();
        let mut manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        // Insert first key
        let result = manager.insert(1, 100);
        assert!(result.is_ok());
        
        assert!(!manager.is_empty());
        assert_eq!(manager.tree_height(), 1);
        assert!(manager.root_page_id() != EMPTY_TREE_ROOT);
    }

    #[test]
    fn test_lookup_empty_tree() {
        let allocator = create_test_allocator();
        let manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        let result = manager.lookup(1);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_insert_and_lookup_single() {
        let allocator = create_test_allocator();
        let mut manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        // Insert a key
        manager.insert(42, 100).unwrap();

        // Lookup should find it
        let result = manager.lookup(42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(100));

        // Lookup non-existent key
        let result = manager.lookup(99);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_insert_and_lookup_multiple() {
        let allocator = create_test_allocator();
        let mut manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        // Insert multiple keys
        for i in 1..=10 {
            manager.insert(i, i as u64 * 100).unwrap();
        }

        // Lookup each key
        for i in 1..=10 {
            let result = manager.lookup(i);
            assert!(result.is_ok(), "Failed to lookup key {}", i);
            assert_eq!(result.unwrap(), Some(i as u64 * 100), "Wrong value for key {}", i);
        }

        // Lookup non-existent key
        let result = manager.lookup(999);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_update_existing_key() {
        let allocator = create_test_allocator();
        let mut manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        // Insert key
        manager.insert(1, 100).unwrap();

        // Update same key
        manager.insert(1, 200).unwrap();

        // Lookup should return new value
        let result = manager.lookup(1);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(200));
    }

    #[test]
    fn test_delete_existing_key() {
        let allocator = create_test_allocator();
        let mut manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        // Insert and then delete
        manager.insert(1, 100).unwrap();
        let deleted = manager.delete(1).unwrap();
        
        assert!(deleted);
        
        // Lookup should return None
        let result = manager.lookup(1);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_delete_nonexistent_key() {
        let allocator = create_test_allocator();
        let mut manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        // Delete without inserting
        let deleted = manager.delete(999).unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_delete_from_empty_tree() {
        let allocator = create_test_allocator();
        let mut manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        let deleted = manager.delete(1).unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_cache_stats() {
        let allocator = create_test_allocator();
        let manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        let (len, capacity) = manager.cache_stats();
        assert_eq!(len, 0);
        assert_eq!(capacity, 16);
    }

    #[test]
    fn test_clear_cache() {
        let allocator = create_test_allocator();
        let mut manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        // Insert something to populate cache
        manager.insert(1, 100).unwrap();
        
        // Clear cache
        manager.clear_cache();
        
        let (len, _) = manager.cache_stats();
        assert_eq!(len, 0);
    }

    #[test]
    fn test_write_batch_basic() {
        let allocator = create_test_allocator();
        let mut manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        // Create batch and stage pages
        let mut batch = manager.create_write_batch();
        let page = IndexPage::new_leaf(1);
        manager.stage_page_to_batch(&mut batch, page).unwrap();
        
        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn test_insert_simple_to_batch_empty_tree() {
        let allocator = create_test_allocator();
        let mut manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        // Insert into empty tree via batch
        let mut batch = manager.create_write_batch();
        let result = manager.insert_simple_to_batch(&mut batch, 1, 100).unwrap();
        
        assert!(result, "Insert should succeed for empty tree");
        assert_eq!(batch.len(), 1);
        assert!(!manager.is_empty(), "Manager should now have root");
    }

    #[test]
    fn test_insert_simple_to_batch_single() {
        let allocator = create_test_allocator();
        let mut manager = BTreeManager::new(allocator, None, None::<PathBuf>);

        // Insert single key to empty tree via batch
        let mut batch = manager.create_write_batch();
        let result = manager.insert_simple_to_batch(&mut batch, 1, 100).unwrap();
        assert!(result, "Insert should succeed for empty tree");
        
        // Should have 1 page staged
        assert_eq!(batch.len(), 1);
    }
}
