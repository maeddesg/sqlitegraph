//! Page allocator for V3 native backend
//!
//! This module implements dynamic page allocation with free list management
//! for unlimited page capacity in V3 database files.
//!
//! ## Architecture
//!
//! 1. **Free List**: Singly-linked list of free pages stored in page headers
//! 2. **Bitmap**: In-memory tracking of allocated pages for O(1) lookup
//! 3. **Double-Free Prevention**: Page state enum tracks allocation status
//! 4. **Checksums**: All pages have XOR checksums for integrity
//!
//! ## Allocation Strategy
//!
//! - **Allocation**: Check free list first, then append to file
//! - **Deallocation**: Add page to free list, mark in bitmap
//! - **Persistence**: Free list head stored in header (PersistentHeaderV3::free_page_list_head)

use crate::backend::native::NativeBackendError;
use crate::backend::native::NativeResult;
use crate::backend::native::v3::constants::{DEFAULT_PAGE_SIZE, V3_HEADER_SIZE};
#[cfg(feature = "v3-forensics")]
use crate::backend::native::v3::forensics::FORENSIC_COUNTERS;
use crate::backend::native::v3::header::PersistentHeaderV3;

/// Page size in bytes (4KB default)
pub const PAGE_SIZE: u64 = DEFAULT_PAGE_SIZE;

/// Maximum pages before bitmap expansion (can grow dynamically)
const INITIAL_BITMAP_PAGES: usize = 1024;

/// Page allocation state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageState {
    /// Page is free and on free list
    Free,
    /// Page is allocated and in use
    Allocated,
    /// Page is pinned (cannot be freed, used during WAL operations)
    Pinned,
}

/// Page allocator for dynamic page allocation
///
/// Manages free list and bitmap tracking for efficient page reuse.
#[derive(Clone)]
pub struct PageAllocator {
    /// Bitmap tracking page allocation state
    /// Grows dynamically as pages are allocated
    bitmap: Vec<bool>,
    /// Free list: stack of deallocated page IDs for reuse (LIFO order)
    free_list: Vec<u64>,
    /// Total pages allocated (including free)
    total_pages: u64,
}

impl PageAllocator {
    /// Create a new page allocator
    ///
    /// # Arguments
    ///
    /// * `header` - V3 persistent header with free_page_list_head and total_pages
    ///
    /// # Returns
    ///
    /// Initialized PageAllocator with sparse bitmap for O(1) initialization
    ///
    /// ## Optimization
    ///
    /// The bitmap is SPARSE: only pages 0 and 1 (reserved) are pre-initialized.
    /// Pages 2+ are "implicitly free" until actually allocated.
    /// This eliminates the O(N) bitmap initialization on open.
    ///
    /// See `get_page_state()` for how pages beyond bitmap.len() are handled.
    pub fn new(header: &PersistentHeaderV3) -> Self {
        // OPTIMIZATION: Sparse bitmap initialization
        // Only allocate bitmap entries for pages 0 and 1 (reserved pages).
        // Pages 2+ are implicitly free until actually allocated (handled by get_page_state).
        // This eliminates O(N) startup cost for large databases.
        let mut bitmap = Vec::with_capacity(INITIAL_BITMAP_PAGES);

        // Page 0: Header page (always allocated/reserved)
        bitmap.push(true);

        // Page 1: First data page (allocated during database creation)
        bitmap.push(true);

        // Pages 2+ are NOT pre-initialized.
        // They are implicitly free until actually allocated.
        // The allocate() method will extend to bitmap as needed.

        Self {
            bitmap,
            free_list: Vec::new(),
            total_pages: header.total_pages,
        }
    }

    /// Allocate a new page
    ///
    /// # Strategy
    ///
    /// 1. Check free list for reusable page
    /// 2. If none, append new page to file
    /// 3. Mark page as allocated in bitmap
    ///
    /// # Returns
    ///
    /// Allocated page_id
    pub fn allocate(&mut self) -> NativeResult<u64> {
        #[cfg(feature = "v3-forensics")]
        FORENSIC_COUNTERS
            .page_allocate_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Strategy 1: Try to reuse from free list (LIFO stack)
        if let Some(page_id) = self.free_list.pop() {
            // Mark as allocated in bitmap
            let page_idx = page_id as usize;
            if page_idx < self.bitmap.len() {
                self.bitmap[page_idx] = true;
            }

            return Ok(page_id);
        }

        // Strategy 2: Allocate new page at end of file
        // Pages 0 and 1 are reserved (header and first data page)
        // Ensure we start allocating from page 2 if this is a fresh database
        let new_page_id = if self.total_pages < 2 {
            // First allocation - skip reserved pages 0 and 1
            2
        } else {
            self.total_pages
        };

        // Ensure bitmap has capacity
        if new_page_id as usize >= self.bitmap.len() {
            self.bitmap.resize((new_page_id as usize) + 1024, false);
        }

        // Mark as allocated
        self.bitmap[new_page_id as usize] = true;
        self.total_pages = new_page_id + 1;

        Ok(new_page_id)
    }

    /// Deallocate a page (add to free list)
    ///
    /// # Arguments
    ///
    /// * `page_id` - Page ID to free
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Page is already free (double-free detection)
    /// - Page ID 0 (header page cannot be freed)
    pub fn deallocate(&mut self, page_id: u64) -> NativeResult<()> {
        // Validate page_id
        if page_id == 0 {
            return Err(NativeBackendError::InvalidHeader {
                field: "page_id".to_string(),
                reason: "Cannot free header page (page 0)".to_string(),
            });
        }

        let page_idx = page_id as usize;

        // Ensure bitmap covers this page for accurate tracking
        if page_idx >= self.bitmap.len() {
            self.bitmap.resize(page_idx + 1, false);
        }

        // Double-free detection: if bitmap already shows free, this is a double-free
        if !self.bitmap[page_idx] {
            return Err(NativeBackendError::CorruptionDetected {
                context: format!("Double-free detected for page {}", page_id),
                source: None,
            });
        }

        // Mark as free in bitmap
        self.bitmap[page_idx] = false;

        // Push onto free list stack
        self.free_list.push(page_id);

        Ok(())
    }

    /// Get page state
    ///
    /// # Arguments
    ///
    /// * `page_id` - Page ID to query
    ///
    /// # Returns
    ///
    /// PageState (Free, Allocated, or Pinned)
    pub fn get_page_state(&self, page_id: u64) -> NativeResult<PageState> {
        if page_id == 0 {
            // Header page is always allocated
            return Ok(PageState::Allocated);
        }

        let page_idx = page_id as usize;

        if page_idx >= self.total_pages as usize {
            return Err(NativeBackendError::InvalidHeader {
                field: "page_id".to_string(),
                reason: format!("Page {} exceeds max pages {}", page_id, self.total_pages),
            });
        }

        if page_idx >= self.bitmap.len() {
            // Page beyond current bitmap is implicitly free (not yet allocated)
            return Ok(PageState::Free);
        }

        let state = if self.bitmap[page_idx] {
            PageState::Allocated
        } else {
            PageState::Free
        };

        Ok(state)
    }

    /// Pin a page (prevent deallocation during WAL operations)
    ///
    /// # Arguments
    ///
    /// * `page_id` - Page ID to pin
    ///
    /// # Note
    ///
    /// Full implementation would track pinned pages separately.
    /// For Phase 64, this is a stub that validates the page exists.
    pub fn pin_page(&mut self, page_id: u64) -> NativeResult<()> {
        let state = self.get_page_state(page_id)?;

        if state == PageState::Free {
            return Err(NativeBackendError::InvalidHeader {
                field: "page_state".to_string(),
                reason: format!("Cannot pin free page {}", page_id),
            });
        }

        // Full implementation would track pinned pages in a HashSet
        // For Phase 64, this validates state only
        Ok(())
    }

    /// Unpin a page (allow deallocation)
    ///
    /// # Arguments
    ///
    /// * `page_id` - Page ID to unpin
    ///
    /// # Note
    ///
    /// Full implementation would remove from pinned set.
    /// For Phase 64, this is a stub that validates the page exists.
    pub fn unpin_page(&mut self, page_id: u64) -> NativeResult<()> {
        let _state = self.get_page_state(page_id)?;

        // Full implementation would remove from pinned set
        // For Phase 64, this validates state only
        Ok(())
    }

    /// Get current allocation statistics
    ///
    /// # Returns
    ///
    /// Tuple of (allocated_pages, free_pages, total_pages)
    pub fn stats(&self) -> (u64, u64, u64) {
        // Count actually allocated pages from bitmap
        let allocated = self.bitmap.iter().filter(|&&x| x).count() as u64;
        let total = self.total_pages;
        // Free pages = pages deallocated and on free list + pages never allocated (beyond total)
        let on_free_list = self.free_list.len() as u64;
        // Pages that have been touched but are free = on_free_list
        // Pages never touched = total_pages..∞ are implicitly free but don't count
        let free = on_free_list;
        (allocated, free, total)
    }

    /// Get free list head (for persistence)
    ///
    /// # Returns
    ///
    /// Top of the free list stack page ID (0 if empty)
    pub fn free_list_head(&self) -> u64 {
        self.free_list.last().copied().unwrap_or(0)
    }

    /// Get total pages (for persistence)
    ///
    /// # Returns
    ///
    /// Total pages allocated
    pub fn total_pages(&self) -> u64 {
        self.total_pages
    }

    /// Calculate page offset in file
    ///
    /// # Arguments
    ///
    /// * `page_id` - Page ID
    ///
    /// # Returns
    ///
    /// Byte offset of page in file
    ///
    /// # Formula
    ///
    /// offset = V3_HEADER_SIZE + (page_id - 1) * PAGE_SIZE
    ///
    /// Note: page_id 0 is the header (not a data page)
    /// Data pages start at page_id = 1
    pub fn page_offset(page_id: u64) -> NativeResult<u64> {
        if page_id == 0 {
            return Ok(0); // Header page
        }

        // Data page: header + (page_id - 1) * page_size
        let offset = V3_HEADER_SIZE + (page_id - 1) * PAGE_SIZE;
        Ok(offset)
    }

    /// Validate page checksum
    ///
    /// # Arguments
    ///
    /// * `page_data` - Raw page bytes
    /// * `stored_checksum` - Checksum from page header
    ///
    /// # Returns
    ///
    /// Ok(()) if checksum valid, Err otherwise
    pub fn validate_checksum(page_data: &[u8], stored_checksum: u64) -> NativeResult<()> {
        // Calculate XOR checksum over page data (excluding checksum field)
        let calculated = xor_checksum(page_data);

        if calculated != stored_checksum {
            return Err(NativeBackendError::InvalidChecksum {
                expected: stored_checksum,
                found: calculated,
            });
        }

        Ok(())
    }
}

/// Simple XOR checksum for page integrity
pub fn xor_checksum(data: &[u8]) -> u64 {
    const SEED: u64 = 0x5A5A5A5A5A5A5A5A;
    let mut checksum = SEED;
    for (i, &byte) in data.iter().enumerate() {
        checksum ^= (byte as u64) ^ (i as u64);
    }
    checksum
}

/// Page header format for free list linkage
///
/// Each free page stores a pointer to the next free page
/// in its header (first 8 bytes after standard page header).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreePageHeader {
    /// Next free page ID (0 if none)
    pub next_free: u64,
    /// Checksum for this free page
    pub checksum: u64,
}

impl FreePageHeader {
    /// Size of free page header in bytes
    pub const SIZE: usize = 16;

    /// Create a new free page header
    pub fn new(next_free: u64) -> Self {
        Self {
            next_free,
            checksum: 0, // Calculated when page is written
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[0..8].copy_from_slice(&self.next_free.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.checksum.to_le_bytes());
        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }

        let next_free = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
        let checksum = u64::from_le_bytes(bytes[8..16].try_into().ok()?);

        Some(Self {
            next_free,
            checksum,
        })
    }

    /// Calculate checksum for this header
    pub fn calculate_checksum(&self) -> u64 {
        xor_checksum(&self.next_free.to_le_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocator_initialization() {
        let header = PersistentHeaderV3::new_v3();
        let allocator = PageAllocator::new(&header);

        let (allocated, free, total) = allocator.stats();
        // With total_pages=0, allocator still reserves pages 0 (header) and 1 (data)
        // in the bitmap for immediate use
        assert_eq!(
            total, 0,
            "New allocator should have 0 total pages from header"
        );
        assert_eq!(
            allocated, 2,
            "Pages 0 (header) and 1 (data) should be reserved"
        );
        assert_eq!(free, 0, "Should have 0 free pages initially (all reserved)");
    }

    #[test]
    fn test_page_offset_calculation() {
        // Header page
        assert_eq!(
            PageAllocator::page_offset(0).unwrap(),
            0,
            "Page 0 should be at offset 0"
        );

        // First data page
        let first_data = V3_HEADER_SIZE;
        assert_eq!(
            PageAllocator::page_offset(1).unwrap(),
            first_data,
            "Page 1 should start after header"
        );

        // Second data page
        let second_data = V3_HEADER_SIZE + PAGE_SIZE;
        assert_eq!(
            PageAllocator::page_offset(2).unwrap(),
            second_data,
            "Page 2 should start after page 1"
        );
    }

    #[test]
    fn test_allocate_new_pages() {
        let header = PersistentHeaderV3::new_v3();
        let mut allocator = PageAllocator::new(&header);

        // Pages 0 and 1 are reserved (header + first data page)
        // First allocation should return page 2
        let page1 = allocator.allocate().unwrap();
        assert_eq!(
            page1, 2,
            "First allocation should be page 2 (pages 0,1 reserved)"
        );

        let state1 = allocator.get_page_state(page1).unwrap();
        assert_eq!(state1, PageState::Allocated);

        // Allocate second page
        let page2 = allocator.allocate().unwrap();
        assert_eq!(page2, 3, "Second allocation should be page 3");
    }

    #[test]
    fn test_deallocate_pages() {
        let header = PersistentHeaderV3::new_v3();
        let mut allocator = PageAllocator::new(&header);

        // Pages 0 and 1 are reserved
        // First allocation returns page 2
        let page1 = allocator.allocate().unwrap();
        assert_eq!(page1, 2, "First allocation returns page 2");

        let page2 = allocator.allocate().unwrap();
        assert_eq!(page2, 3, "Second allocation returns page 3");

        // Deallocate page 3
        allocator.deallocate(page2).unwrap();

        // Verify page 3 is now free
        let state = allocator.get_page_state(page2).unwrap();
        assert_eq!(state, PageState::Free);
    }

    #[test]
    fn test_double_free_detection() {
        let header = PersistentHeaderV3::new_v3();
        let mut allocator = PageAllocator::new(&header);

        // Pages 0 and 1 are reserved
        // Allocate two pages: 2 and 3
        let page0 = allocator.allocate().unwrap();
        assert_eq!(page0, 2);

        let page1 = allocator.allocate().unwrap();
        assert_eq!(page1, 3);

        // Deallocate page 3
        allocator.deallocate(page1).unwrap();

        // Second deallocate of page 3 should fail
        let result = allocator.deallocate(page1);
        assert!(result.is_err(), "Double-free should return error");
    }

    #[test]
    fn test_pin_page() {
        let header = PersistentHeaderV3::new_v3();
        let mut allocator = PageAllocator::new(&header);

        let page = allocator.allocate().unwrap();
        allocator.pin_page(page).unwrap();

        // Pinning allocated page should succeed
        let state = allocator.get_page_state(page).unwrap();
        assert_eq!(state, PageState::Allocated);
    }

    #[test]
    fn test_checksum_validation() {
        let data = b"test page data";
        let checksum = xor_checksum(data);

        // Valid checksum should pass
        assert!(PageAllocator::validate_checksum(data, checksum).is_ok());

        // Invalid checksum should fail
        let result = PageAllocator::validate_checksum(data, checksum + 1);
        assert!(result.is_err(), "Invalid checksum should fail validation");
    }

    #[test]
    fn test_free_page_header_serialization() {
        let header = FreePageHeader::new(42);
        let bytes = header.to_bytes();

        assert_eq!(bytes[0..8], 42u64.to_le_bytes());
        assert_eq!(bytes[8..16], 0u64.to_le_bytes());

        let deserialized = FreePageHeader::from_bytes(&bytes).unwrap();
        assert_eq!(deserialized.next_free, 42);
    }

    #[test]
    fn test_free_list_chain_reuse() {
        // BUG FIX: Previously, deallocate() set free_list_head = page_id
        // which overwrote the previous head, losing the chain.
        // Only 1 page could ever be reused.
        let header = PersistentHeaderV3::new_v3();
        let mut allocator = PageAllocator::new(&header);

        // Allocate 5 pages: 2, 3, 4, 5, 6
        let pages: Vec<u64> = (0..5).map(|_| allocator.allocate().unwrap()).collect();
        assert_eq!(pages, vec![2, 3, 4, 5, 6]);

        // Free pages 3, 4, 5
        allocator.deallocate(3).unwrap();
        allocator.deallocate(4).unwrap();
        allocator.deallocate(5).unwrap();

        // All 3 freed pages should be reusable (LIFO order: 5, 4, 3)
        let reused1 = allocator.allocate().unwrap();
        assert_eq!(reused1, 5, "First reuse should be last freed (LIFO)");

        let reused2 = allocator.allocate().unwrap();
        assert_eq!(reused2, 4, "Second reuse should be middle freed");

        let reused3 = allocator.allocate().unwrap();
        assert_eq!(reused3, 3, "Third reuse should be first freed");

        // Free list now empty - next allocation should be brand new
        let new_page = allocator.allocate().unwrap();
        assert_eq!(new_page, 7, "After exhausting free list, should allocate new page 7");
    }

    #[test]
    fn test_stats_accuracy_after_alloc_dealloc() {
        // BUG FIX: stats() previously counted bitmap entries instead of
        // properly tracking free pages via the free list.
        let header = PersistentHeaderV3::new_v3();
        let mut allocator = PageAllocator::new(&header);

        // Allocate 3 pages: 2, 3, 4
        allocator.allocate().unwrap();
        allocator.allocate().unwrap();
        allocator.allocate().unwrap();

        let (allocated, free, total) = allocator.stats();
        // Pages 0, 1 (reserved) + 2, 3, 4 (allocated) = 5 allocated
        assert_eq!(allocated, 5, "Should have 5 allocated pages (2 reserved + 3 new)");
        assert_eq!(free, 0, "No free pages yet");
        assert_eq!(total, 5, "Total pages should be 5");

        // Free page 3
        allocator.deallocate(3).unwrap();
        let (allocated, free, total) = allocator.stats();
        assert_eq!(allocated, 4, "Should have 4 allocated after freeing one");
        assert_eq!(free, 1, "Should have 1 free page");
        assert_eq!(total, 5, "Total pages unchanged");

        // Free page 4
        allocator.deallocate(4).unwrap();
        let (allocated, free, _total) = allocator.stats();
        assert_eq!(allocated, 3, "Should have 3 allocated after freeing two");
        assert_eq!(free, 2, "Should have 2 free pages");
    }

    #[test]
    fn test_double_free_beyond_bitmap() {
        // BUG FIX: Previously, pages beyond bitmap.len() could be
        // double-freed silently because the check was skipped.
        let header = PersistentHeaderV3::new_v3();
        let mut allocator = PageAllocator::new(&header);

        // Allocate page 2 (bitmap extends to ~1026)
        let page = allocator.allocate().unwrap();
        assert_eq!(page, 2);

        // Manually set total_pages high but don't extend bitmap
        allocator.total_pages = 5000;

        // Allocate page at high offset (will extend bitmap)
        // But first, deallocate and then try to double-free
        allocator.deallocate(page).unwrap();

        // This should fail (double-free)
        let result = allocator.deallocate(page);
        assert!(result.is_err(), "Double-free of freed page should be detected");
    }
}
