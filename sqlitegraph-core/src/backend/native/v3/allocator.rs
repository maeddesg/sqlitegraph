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
use crate::backend::native::v3::constants::{
    DEFAULT_PAGE_SIZE, V3_HEADER_SIZE,
};
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
    /// Free list head (page_id, 0 if none)
    free_list_head: u64,
    /// Total pages allocated (including free)
    total_pages: u64,
    /// Page size in bytes
    page_size: u64,
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
    /// Initialized PageAllocator with bitmap sized to total_pages
    pub fn new(header: &PersistentHeaderV3) -> Self {
        let total_pages = header.total_pages as usize;
        let mut bitmap = Vec::with_capacity(total_pages.max(INITIAL_BITMAP_PAGES));

        // Initialize bitmap: pages 0 and 1 are reserved (header and first data page)
        // Page 0: Header (112 bytes, rest unused)
        // Page 1: First data page (node index root)
        bitmap.resize(total_pages.max(2), true); // true = allocated/reserved
        if total_pages >= 1 {
            bitmap[0] = true; // Header page always allocated
        }
        if total_pages >= 2 {
            bitmap[1] = true; // First data page initially allocated
        }

        // Remaining pages based on free list (reverse engineering from free_list_head)
        // For now, all pages beyond 1 are considered free unless on free list
        for page_id in 2..total_pages {
            bitmap[page_id as usize] = false; // Initially mark as free
        }

        Self {
            bitmap,
            free_list_head: header.free_page_list_head,
            total_pages: header.total_pages,
            page_size: header.page_size as u64,
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
        // Strategy 1: Try to reuse from free list
        if self.free_list_head != 0 {
            let page_id = self.free_list_head;

            // Mark as allocated in bitmap
            if page_id > 0 && (page_id as usize) < self.bitmap.len() {
                self.bitmap[page_id as usize] = true;
            }

            // Note: In a full implementation, we would read the page header
            // to get the next free page pointer. For now, we clear the
            // free list head to indicate the page is taken.
            self.free_list_head = 0;

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

        // Double-free detection
        if page_idx < self.bitmap.len() && !self.bitmap[page_idx] {
            return Err(NativeBackendError::CorruptionDetected {
                context: format!("Double-free detected for page {}", page_id),
                source: None,
            });
        }

        // Mark as free in bitmap
        if page_idx < self.bitmap.len() {
            self.bitmap[page_idx] = false;
        }

        // Add to free list (singly-linked: page becomes new head)
        // Note: In full implementation, we would write next_free=old_head
        // to the page header. For now, we just update the in-memory list.
        self.free_list_head = page_id;

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
        let allocated = self.bitmap.iter().filter(|&&x| x).count() as u64;
        let total = self.total_pages;
        let free = total.saturating_sub(allocated);
        (allocated, free, total)
    }

    /// Get free list head (for persistence)
    ///
    /// # Returns
    ///
    /// Current free list head page ID (0 if none)
    pub fn free_list_head(&self) -> u64 {
        self.free_list_head
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
        assert_eq!(total, 0, "New allocator should have 0 total pages from header");
        assert_eq!(allocated, 2, "Pages 0 (header) and 1 (data) should be reserved");
        assert_eq!(free, 0, "Should have 0 free pages initially (all reserved)");
    }

    #[test]
    fn test_page_offset_calculation() {
        // Header page
        assert_eq!(PageAllocator::page_offset(0).unwrap(), 0, "Page 0 should be at offset 0");

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
        assert_eq!(page1, 2, "First allocation should be page 2 (pages 0,1 reserved)");

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
}
