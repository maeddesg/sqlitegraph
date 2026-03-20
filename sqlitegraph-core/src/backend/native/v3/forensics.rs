//! Forensic instrumentation for V3 backend performance analysis
//!
//! This module provides atomic counters for tracking internal operations
//! to identify performance amplification issues.
//!
//! # Usage
//!
//! Enable with the `v3-forensics` feature flag:
//! ```toml
//! [features]
//! v3-forensics = []
//! ```
//!
//! # Counters Tracked
//!
//! ## Write Path
//! - `logical_insert_node_calls`: Number of insert_node API calls
//! - `logical_insert_edge_calls`: Number of insert_edge API calls
//! - `btree_insert_calls`: Number of B+Tree insert operations
//! - `btree_split_count`: Number of B+Tree page splits
//! - `page_allocate_count`: Number of page allocations
//! - `page_write_count`: Total number of page write operations
//! - `wal_append_count`: Number of WAL records appended
//! - `wal_flush_count`: Number of WAL flush operations
//! - `node_encode_count`: Number of node encodings
//! - `edge_encode_count`: Number of edge encodings
//! - `sync_data_count`: Number of sync_data() calls
//! - `sync_all_count`: Number of sync_all() calls
//!
//! ## Read Path
//! - `logical_get_node_calls`: Number of get_node API calls
//! - `logical_neighbors_calls`: Number of neighbors API calls
//! - `btree_lookup_calls`: Number of B+Tree lookup operations
//! - `page_read_count`: Total number of page read operations
//! - `node_decode_count`: Number of node decodings
//! - `edge_decode_count`: Number of edge decodings
//!
//! # Phase 3: Page Ownership Tracking
//!
//! This module now includes a page ownership registry that tracks:
//! - Which subsystem allocated each page first
//! - All writes to each page (page_id, page_type, subsystem, sequence)
//! - Page type mismatches (e.g., B+Tree writing to a node page)
//! - Conflicting writes from different subsystems to the same page
//!
//! The page types tracked are:
//! - `NODE`: NodePage (node data storage)
//! - `BTREE`: IndexPage (B+Tree index page)
//! - `EDGE`: Edge cluster page
//! - `HEADER`: Persistent header
//! - `UNKNOWN`: Unidentified page type

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// Page type enumeration for ownership tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageType {
    Node = 1,
    BTree = 2,
    Edge = 3,
    Header = 4,
    Unknown = 5,
}

impl PageType {
    pub fn name(&self) -> &'static str {
        match self {
            PageType::Node => "NODE",
            PageType::BTree => "BTREE",
            PageType::Edge => "EDGE",
            PageType::Header => "HEADER",
            PageType::Unknown => "UNKNOWN",
        }
    }

    /// Detect page type from raw page bytes by checking header patterns
    pub fn detect_from_bytes(bytes: &[u8]) -> PageType {
        if bytes.len() < 32 {
            return PageType::Unknown;
        }

        // Read page_id from offset 0-8
        let page_id = u64::from_be_bytes(bytes[0..8].try_into().unwrap_or([0u8; 8]));

        // Page 0 is always header
        if page_id == 0 {
            return PageType::Header;
        }

        // Check is_leaf flag at offset 8 (IndexPage marker)
        // B+Tree pages have is_leaf at offset 8 (0 or 1)
        let is_leaf_or_reserved = bytes[8];

        // NodePage has next_page_id at offset 8-15
        // If bytes[8] is 0, it could be either:
        // - B+Tree internal page (is_leaf = 0)
        // - NodePage with next_page_id = 0
        //
        // Distinguish by checking offset 9:
        // - B+Tree has is_root at offset 9
        // - NodePage has high byte of next_page_id
        let is_root_flag = bytes[9];

        // B+Tree pages have is_root as 0 or 1 at offset 9
        // NodePage next_page_id high byte is typically > 1 for non-zero page IDs
        if is_leaf_or_reserved <= 1 && is_root_flag <= 1 {
            // Likely B+Tree page
            return PageType::BTree;
        }

        // Check for NodePage pattern
        // NodePage has: page_id(8) + next_page_id(8) + node_count(2) + used_bytes(2)
        // The used_bytes at offset 18-19 should be reasonable (< 4096)
        let used_bytes = u16::from_be_bytes([bytes[18], bytes[19]]);

        // If used_bytes is reasonable (< 4000), likely a NodePage
        if used_bytes < 4000 {
            return PageType::Node;
        }

        // Check for Edge cluster pattern (less certain)
        // Edge pages have similar structure to NodePage but different layout
        PageType::Unknown
    }
}

/// Subsystem enumeration for ownership tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Subsystem {
    NodeStore = 1,
    BTreeManager = 2,
    EdgeStore = 3,
    Allocator = 4,
    Unknown = 5,
}

impl Subsystem {
    pub fn name(&self) -> &'static str {
        match self {
            Subsystem::NodeStore => "NodeStore",
            Subsystem::BTreeManager => "BTreeManager",
            Subsystem::EdgeStore => "EdgeStore",
            Subsystem::Allocator => "Allocator",
            Subsystem::Unknown => "Unknown",
        }
    }
}

/// Page ownership record
#[derive(Debug, Clone)]
pub struct PageOwnershipRecord {
    /// Page ID
    pub page_id: u64,
    /// Subsystem that first allocated this page
    pub first_owner: Subsystem,
    /// Page type when first allocated
    pub first_page_type: PageType,
    /// Write sequence number for this page
    pub write_sequence: u64,
    /// All writes to this page
    pub writes: Vec<PageWriteRecord>,
    /// Whether a conflict was detected
    pub has_conflict: bool,
    /// First conflict detected (if any)
    pub first_conflict: Option<PageConflict>,
}

/// Individual page write record
#[derive(Debug, Clone)]
pub struct PageWriteRecord {
    /// Sequence number (global order)
    pub sequence: u64,
    /// Subsystem performing the write
    pub subsystem: Subsystem,
    /// Page type being written
    pub page_type: PageType,
    /// File offset where written
    pub file_offset: u64,
    /// Function that performed the write
    pub function: String,
    /// Whether this was a conflict (different subsystem/type than first owner)
    pub is_conflict: bool,
}

/// Page conflict record
#[derive(Debug, Clone)]
pub struct PageConflict {
    /// First write
    pub first_write: PageWriteRecord,
    /// Conflicting write
    pub conflicting_write: PageWriteRecord,
    /// Type of conflict
    pub conflict_type: ConflictType,
}

/// Type of conflict detected
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictType {
    /// Different subsystems writing to same page
    SubsystemMismatch,
    /// Different page types on same page
    PageTypeMismatch,
    /// Both subsystem and type mismatch
    BothMismatch,
}

impl PageOwnershipRecord {
    pub fn new(page_id: u64, subsystem: Subsystem, page_type: PageType) -> Self {
        Self {
            page_id,
            first_owner: subsystem,
            first_page_type: page_type,
            write_sequence: 0,
            writes: Vec::new(),
            has_conflict: false,
            first_conflict: None,
        }
    }

    pub fn record_write(
        &mut self,
        subsystem: Subsystem,
        page_type: PageType,
        file_offset: u64,
        function: String,
        sequence: u64,
    ) -> bool {
        let is_conflict = subsystem != self.first_owner || page_type != self.first_page_type;

        let write_record = PageWriteRecord {
            sequence,
            subsystem,
            page_type,
            file_offset,
            function,
            is_conflict,
        };

        if is_conflict && !self.has_conflict {
            self.has_conflict = true;
            self.first_conflict = Some(PageConflict {
                first_write: self.writes.first().cloned().unwrap_or(write_record.clone()),
                conflicting_write: write_record.clone(),
                conflict_type: if subsystem != self.first_owner && page_type != self.first_page_type
                {
                    ConflictType::BothMismatch
                } else if subsystem != self.first_owner {
                    ConflictType::SubsystemMismatch
                } else {
                    ConflictType::PageTypeMismatch
                },
            });
        }

        self.writes.push(write_record);
        self.write_sequence = sequence;
        is_conflict
    }
}

/// Global page ownership registry
pub static PAGE_OWNERSHIP: OnceLock<Mutex<PageOwnershipRegistry>> = OnceLock::new();

/// Get the page ownership registry
pub fn get_page_registry() -> &'static Mutex<PageOwnershipRegistry> {
    PAGE_OWNERSHIP.get_or_init(|| Mutex::new(PageOwnershipRegistry::new()))
}

/// Page ownership registry
#[derive(Debug)]
pub struct PageOwnershipRegistry {
    /// Map from page_id to ownership record
    pages: HashMap<u64, PageOwnershipRecord>,
    /// Global write sequence counter
    global_sequence: u64,
    /// Total conflicts detected
    total_conflicts: u64,
    /// First conflicting page ID (if any)
    first_conflict_page: Option<u64>,
}

impl PageOwnershipRegistry {
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            global_sequence: 0,
            total_conflicts: 0,
            first_conflict_page: None,
        }
    }

    /// Register a page allocation (first claim)
    pub fn register_allocation(&mut self, page_id: u64, subsystem: Subsystem, page_type: PageType) {
        if !self.pages.contains_key(&page_id) {
            let record = PageOwnershipRecord::new(page_id, subsystem, page_type);
            self.pages.insert(page_id, record);
        }
    }

    /// Register a page write
    pub fn register_write(
        &mut self,
        page_id: u64,
        subsystem: Subsystem,
        page_type: PageType,
        file_offset: u64,
        function: String,
    ) -> bool {
        self.global_sequence += 1;

        // Auto-register if this is the first time we see this page
        if !self.pages.contains_key(&page_id) {
            self.register_allocation(page_id, subsystem, page_type);
        }

        let record = self.pages.get_mut(&page_id).unwrap();
        let is_conflict = record.record_write(
            subsystem,
            page_type,
            file_offset,
            function,
            self.global_sequence,
        );

        if is_conflict && self.first_conflict_page.is_none() {
            self.first_conflict_page = Some(page_id);
        }

        if is_conflict {
            self.total_conflicts += 1;
        }

        is_conflict
    }

    /// Get ownership record for a page
    pub fn get(&self, page_id: u64) -> Option<&PageOwnershipRecord> {
        self.pages.get(&page_id)
    }

    /// Get all pages with conflicts
    pub fn get_conflicts(&self) -> Vec<&PageOwnershipRecord> {
        self.pages.values().filter(|r| r.has_conflict).collect()
    }

    /// Get total conflict count
    pub fn total_conflicts(&self) -> u64 {
        self.total_conflicts
    }

    /// Get first conflicting page ID
    pub fn first_conflict_page(&self) -> Option<u64> {
        self.first_conflict_page
    }

    /// Print conflict report
    pub fn print_conflict_report(&self) {
        println!("\n═══════════════════════════════════════════════════════════");
        println!("              PAGE OWNERSHIP CONFLICT REPORT                 ");
        println!("═══════════════════════════════════════════════════════════\n");

        println!("Total conflicts detected: {}", self.total_conflicts);
        println!("First conflicting page: {:?}\n", self.first_conflict_page);

        let conflicts = self.get_conflicts();
        if conflicts.is_empty() {
            println!("No conflicts detected - all pages have consistent ownership.\n");
        } else {
            println!("Conflicting pages (showing first 10):\n");
            for (idx, record) in conflicts.iter().take(10).enumerate() {
                println!("{}. Page {}:", idx + 1, record.page_id);
                println!(
                    "   First owner: {} ({})",
                    record.first_owner.name(),
                    record.first_page_type.name()
                );

                if let Some(conflict) = &record.first_conflict {
                    println!("   FIRST CONFLICT:");
                    println!(
                        "     Original: {} wrote {} at offset {}",
                        conflict.first_write.subsystem.name(),
                        conflict.first_write.page_type.name(),
                        conflict.first_write.file_offset
                    );
                    println!(
                        "     Conflict: {} wrote {} at offset {}",
                        conflict.conflicting_write.subsystem.name(),
                        conflict.conflicting_write.page_type.name(),
                        conflict.conflicting_write.file_offset
                    );
                    println!("     Type: {:?}\n", conflict.conflict_type);
                }

                println!("   Total writes to this page: {}", record.writes.len());
                println!("   Write history:");
                for write in record.writes.iter().take(5) {
                    println!(
                        "     [seq={}] {} wrote {} at {} ({})",
                        write.sequence,
                        write.subsystem.name(),
                        write.page_type.name(),
                        write.file_offset,
                        write.function
                    );
                }
                if record.writes.len() > 5 {
                    println!("     ... and {} more", record.writes.len() - 5);
                }
                println!();
            }

            if conflicts.len() > 10 {
                println!("... and {} more conflicting pages", conflicts.len() - 10);
            }
        }

        println!("═══════════════════════════════════════════════════════════\n");
    }

    /// Print page ownership map
    pub fn print_ownership_map(&self) {
        println!("\n═══════════════════════════════════════════════════════════");
        println!("                 PAGE OWNERSHIP MAP                         ");
        println!("═══════════════════════════════════════════════════════════\n");

        println!("Total pages tracked: {}\n", self.pages.len());

        // Group by owner
        let mut by_owner: HashMap<(&str, &str), Vec<u64>> = HashMap::new();
        for (page_id, record) in &self.pages {
            let key = (record.first_owner.name(), record.first_page_type.name());
            by_owner.entry(key).or_default().push(*page_id);
        }

        for ((owner, page_type), page_ids) in by_owner.iter() {
            println!("{} ({}): {} pages", owner, page_type, page_ids.len());
            if page_ids.len() <= 10 {
                println!("  Page IDs: {:?}", page_ids);
            } else {
                println!(
                    "  Page IDs: {:?} ... and {} more",
                    &page_ids[..10],
                    page_ids.len() - 10
                );
            }
        }

        println!("\n═══════════════════════════════════════════════════════════\n");
    }

    /// Reset the registry
    pub fn reset(&mut self) {
        self.pages.clear();
        self.global_sequence = 0;
        self.total_conflicts = 0;
        self.first_conflict_page = None;
    }
}

impl Default for PageOwnershipRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global forensic counters
pub static FORENSIC_COUNTERS: ForensicCounters = ForensicCounters {
    // Write path
    logical_insert_node_calls: AtomicU64::new(0),
    logical_insert_edge_calls: AtomicU64::new(0),
    btree_insert_calls: AtomicU64::new(0),
    btree_split_count: AtomicU64::new(0),
    page_allocate_count: AtomicU64::new(0),
    page_write_count: AtomicU64::new(0),
    wal_append_count: AtomicU64::new(0),
    wal_flush_count: AtomicU64::new(0),
    checkpoint_count: AtomicU64::new(0),
    node_encode_count: AtomicU64::new(0),
    edge_encode_count: AtomicU64::new(0),
    sync_data_count: AtomicU64::new(0),
    sync_all_count: AtomicU64::new(0),

    // Read path
    logical_get_node_calls: AtomicU64::new(0),
    logical_neighbors_calls: AtomicU64::new(0),
    btree_lookup_calls: AtomicU64::new(0),
    page_read_count: AtomicU64::new(0),
    node_decode_count: AtomicU64::new(0),
    edge_decode_count: AtomicU64::new(0),

    // Cache performance
    btree_cache_hit_count: AtomicU64::new(0),
    btree_cache_miss_count: AtomicU64::new(0),
    node_cache_hit_count: AtomicU64::new(0),
    node_cache_miss_count: AtomicU64::new(0),

    // Lock contention
    btree_read_lock_count: AtomicU64::new(0),
    btree_write_lock_count: AtomicU64::new(0),

    // Phase 2: Enhanced cache residency tracking
    dirty_page_hit_count: AtomicU64::new(0),
    node_page_cache_hit_count: AtomicU64::new(0),
    node_page_cache_miss_count: AtomicU64::new(0),
    redundant_page_reload_count: AtomicU64::new(0),

    // Phase 2: Edge path visibility
    edge_cache_hit_count: AtomicU64::new(0),
    edge_cache_miss_count: AtomicU64::new(0),
    edge_page_read_count: AtomicU64::new(0),

    // Phase 2: Page-ID tracing (for operation-scoped analysis)
    last_btree_pages_read: AtomicU64::new(0), // Bitmask of pages read in last op
    last_node_pages_read: AtomicU64::new(0),  // Bitmask of pages read in last op

    // Phase 3: Node unpack cost breakdown
    node_page_unpack_count: AtomicU64::new(0), // Number of NodePage::unpack() calls
    node_linear_scan_steps: AtomicU64::new(0), // Total nodes scanned during unpack (O(n) search)
    btree_traversal_depth_total: AtomicU64::new(0), // Total B+Tree levels traversed
};

/// Forensic counter structure
pub struct ForensicCounters {
    // Write path counters
    pub logical_insert_node_calls: AtomicU64,
    pub logical_insert_edge_calls: AtomicU64,
    pub btree_insert_calls: AtomicU64,
    pub btree_split_count: AtomicU64,
    pub page_allocate_count: AtomicU64,
    pub page_write_count: AtomicU64,
    pub wal_append_count: AtomicU64,
    pub wal_flush_count: AtomicU64,
    pub checkpoint_count: AtomicU64,
    pub node_encode_count: AtomicU64,
    pub edge_encode_count: AtomicU64,
    pub sync_data_count: AtomicU64,
    pub sync_all_count: AtomicU64,

    // Read path counters
    pub logical_get_node_calls: AtomicU64,
    pub logical_neighbors_calls: AtomicU64,
    pub btree_lookup_calls: AtomicU64,
    pub page_read_count: AtomicU64,
    pub node_decode_count: AtomicU64,
    pub edge_decode_count: AtomicU64,

    // Cache performance counters
    pub btree_cache_hit_count: AtomicU64,
    pub btree_cache_miss_count: AtomicU64,
    pub node_cache_hit_count: AtomicU64,
    pub node_cache_miss_count: AtomicU64,

    // Lock contention counters
    pub btree_read_lock_count: AtomicU64,
    pub btree_write_lock_count: AtomicU64,

    // Phase 2: Enhanced cache residency tracking
    pub dirty_page_hit_count: AtomicU64, // Hits on dirty_pages (no I/O)
    pub node_page_cache_hit_count: AtomicU64, // Node page cache hits (no I/O)
    pub node_page_cache_miss_count: AtomicU64, // Node page cache misses (disk I/O)
    pub redundant_page_reload_count: AtomicU64, // Page re-read in same logical op

    // Phase 2: Edge path visibility
    pub edge_cache_hit_count: AtomicU64, // Edge in-memory cache hits
    pub edge_cache_miss_count: AtomicU64, // Edge in-memory cache misses
    pub edge_page_read_count: AtomicU64, // Edge disk page reads

    // Phase 2: Page-ID tracing (for operation-scoped analysis)
    pub last_btree_pages_read: AtomicU64, // Bitmask of pages read in last op
    pub last_node_pages_read: AtomicU64,  // Bitmask of pages read in last op

    // Phase 3: Node unpack cost breakdown
    pub node_page_unpack_count: AtomicU64, // Number of NodePage::unpack() calls
    pub node_linear_scan_steps: AtomicU64, // Total nodes scanned during unpack (O(n) search)
    pub btree_traversal_depth_total: AtomicU64, // Total B+Tree levels traversed
}

impl ForensicCounters {
    /// Reset all counters to zero
    pub fn reset(&self) {
        // Write path
        self.logical_insert_node_calls.store(0, Ordering::Relaxed);
        self.logical_insert_edge_calls.store(0, Ordering::Relaxed);
        self.btree_insert_calls.store(0, Ordering::Relaxed);
        self.btree_split_count.store(0, Ordering::Relaxed);
        self.page_allocate_count.store(0, Ordering::Relaxed);
        self.page_write_count.store(0, Ordering::Relaxed);
        self.wal_append_count.store(0, Ordering::Relaxed);
        self.wal_flush_count.store(0, Ordering::Relaxed);
        self.checkpoint_count.store(0, Ordering::Relaxed);
        self.node_encode_count.store(0, Ordering::Relaxed);
        self.edge_encode_count.store(0, Ordering::Relaxed);
        self.sync_data_count.store(0, Ordering::Relaxed);
        self.sync_all_count.store(0, Ordering::Relaxed);

        // Read path
        self.logical_get_node_calls.store(0, Ordering::Relaxed);
        self.logical_neighbors_calls.store(0, Ordering::Relaxed);
        self.btree_lookup_calls.store(0, Ordering::Relaxed);
        self.page_read_count.store(0, Ordering::Relaxed);
        self.node_decode_count.store(0, Ordering::Relaxed);
        self.edge_decode_count.store(0, Ordering::Relaxed);

        // Cache performance
        self.btree_cache_hit_count.store(0, Ordering::Relaxed);
        self.btree_cache_miss_count.store(0, Ordering::Relaxed);
        self.node_cache_hit_count.store(0, Ordering::Relaxed);
        self.node_cache_miss_count.store(0, Ordering::Relaxed);

        // Lock contention
        self.btree_read_lock_count.store(0, Ordering::Relaxed);
        self.btree_write_lock_count.store(0, Ordering::Relaxed);

        // Phase 2: Enhanced tracking
        self.dirty_page_hit_count.store(0, Ordering::Relaxed);
        self.node_page_cache_hit_count.store(0, Ordering::Relaxed);
        self.node_page_cache_miss_count.store(0, Ordering::Relaxed);
        self.redundant_page_reload_count.store(0, Ordering::Relaxed);
        self.edge_cache_hit_count.store(0, Ordering::Relaxed);
        self.edge_cache_miss_count.store(0, Ordering::Relaxed);
        self.edge_page_read_count.store(0, Ordering::Relaxed);
        self.last_btree_pages_read.store(0, Ordering::Relaxed);
        self.last_node_pages_read.store(0, Ordering::Relaxed);

        // Phase 3: Node unpack cost breakdown
        self.node_page_unpack_count.store(0, Ordering::Relaxed);
        self.node_linear_scan_steps.store(0, Ordering::Relaxed);
        self.btree_traversal_depth_total.store(0, Ordering::Relaxed);
    }

    /// Print a formatted report of all counters
    pub fn print_report(&self) {
        println!("\n═══════════════════════════════════════════════════════════");
        println!("                    V3 FORENSIC COUNTER REPORT                 ");
        println!("═══════════════════════════════════════════════════════════\n");

        println!("WRITE PATH:");
        println!(
            "  Logical insert_node calls:     {}",
            self.logical_insert_node_calls.load(Ordering::Relaxed)
        );
        println!(
            "  Logical insert_edge calls:     {}",
            self.logical_insert_edge_calls.load(Ordering::Relaxed)
        );
        println!(
            "  B+Tree insert calls:           {}",
            self.btree_insert_calls.load(Ordering::Relaxed)
        );
        println!(
            "  B+Tree split count:            {}",
            self.btree_split_count.load(Ordering::Relaxed)
        );
        println!(
            "  Page allocations:              {}",
            self.page_allocate_count.load(Ordering::Relaxed)
        );
        println!(
            "  Page writes (total):           {}",
            self.page_write_count.load(Ordering::Relaxed)
        );
        println!(
            "  WAL appends:                   {}",
            self.wal_append_count.load(Ordering::Relaxed)
        );
        println!(
            "  WAL flushes:                   {}",
            self.wal_flush_count.load(Ordering::Relaxed)
        );
        println!(
            "  Checkpoint count:              {}",
            self.checkpoint_count.load(Ordering::Relaxed)
        );
        println!(
            "  Node encodes:                  {}",
            self.node_encode_count.load(Ordering::Relaxed)
        );
        println!(
            "  Edge encodes:                  {}",
            self.edge_encode_count.load(Ordering::Relaxed)
        );
        println!(
            "  sync_data() calls:             {}",
            self.sync_data_count.load(Ordering::Relaxed)
        );
        println!(
            "  sync_all() calls:              {}",
            self.sync_all_count.load(Ordering::Relaxed)
        );

        println!("\nREAD PATH:");
        println!(
            "  Logical get_node calls:        {}",
            self.logical_get_node_calls.load(Ordering::Relaxed)
        );
        println!(
            "  Logical neighbors calls:       {}",
            self.logical_neighbors_calls.load(Ordering::Relaxed)
        );
        println!(
            "  B+Tree lookup calls:           {}",
            self.btree_lookup_calls.load(Ordering::Relaxed)
        );
        println!(
            "  Page reads (total):            {}",
            self.page_read_count.load(Ordering::Relaxed)
        );
        println!(
            "  Node decodes:                  {}",
            self.node_decode_count.load(Ordering::Relaxed)
        );
        println!(
            "  Edge decodes:                  {}",
            self.edge_decode_count.load(Ordering::Relaxed)
        );

        println!("\nCACHE PERFORMANCE:");
        let btree_hits = self.btree_cache_hit_count.load(Ordering::Relaxed);
        let btree_misses = self.btree_cache_miss_count.load(Ordering::Relaxed);
        let btree_total = btree_hits + btree_misses;
        let btree_hit_rate = if btree_total > 0 {
            (btree_hits as f64 / btree_total as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "  B+Tree cache hits:             {} (hit rate: {:.1}%)",
            btree_hits, btree_hit_rate
        );
        println!("  B+Tree cache misses:           {}", btree_misses);

        let node_hits = self.node_cache_hit_count.load(Ordering::Relaxed);
        let node_misses = self.node_cache_miss_count.load(Ordering::Relaxed);
        let node_total = node_hits + node_misses;
        let node_hit_rate = if node_total > 0 {
            (node_hits as f64 / node_total as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "  Node cache hits:               {} (hit rate: {:.1}%)",
            node_hits, node_hit_rate
        );
        println!("  Node cache misses:             {}", node_misses);

        println!("\nLOCK USAGE:");
        println!(
            "  B+Tree read lock count:        {}",
            self.btree_read_lock_count.load(Ordering::Relaxed)
        );
        println!(
            "  B+Tree write lock count:       {}",
            self.btree_write_lock_count.load(Ordering::Relaxed)
        );

        println!("\nPHASE 2: ENHANCED CACHE RESIDENCY:");
        println!(
            "  Dirty page hits:               {}",
            self.dirty_page_hit_count.load(Ordering::Relaxed)
        );
        println!(
            "  Node page cache hits:          {}",
            self.node_page_cache_hit_count.load(Ordering::Relaxed)
        );
        println!(
            "  Node page cache misses:        {}",
            self.node_page_cache_miss_count.load(Ordering::Relaxed)
        );
        println!(
            "  Redundant page reloads:        {}",
            self.redundant_page_reload_count.load(Ordering::Relaxed)
        );

        println!("\nPHASE 2: EDGE PATH VISIBILITY:");
        println!(
            "  Edge cache hits:               {}",
            self.edge_cache_hit_count.load(Ordering::Relaxed)
        );
        println!(
            "  Edge cache misses:             {}",
            self.edge_cache_miss_count.load(Ordering::Relaxed)
        );
        println!(
            "  Edge page reads:               {}",
            self.edge_page_read_count.load(Ordering::Relaxed)
        );

        println!("\nPHASE 3: NODE UNPACK COST BREAKDOWN:");
        println!(
            "  NodePage::unpack() calls:      {}",
            self.node_page_unpack_count.load(Ordering::Relaxed)
        );
        println!(
            "  Linear scan steps (total):     {}",
            self.node_linear_scan_steps.load(Ordering::Relaxed)
        );
        let avg_scan = if self.node_page_unpack_count.load(Ordering::Relaxed) > 0 {
            self.node_linear_scan_steps.load(Ordering::Relaxed) as f64
                / self.node_page_unpack_count.load(Ordering::Relaxed) as f64
        } else {
            0.0
        };
        println!("  Avg nodes scanned per unpack:  {:.1}", avg_scan);
        println!(
            "  B+Tree traversal depth total:  {}",
            self.btree_traversal_depth_total.load(Ordering::Relaxed)
        );
        let avg_depth = if self.btree_lookup_calls.load(Ordering::Relaxed) > 0 {
            self.btree_traversal_depth_total.load(Ordering::Relaxed) as f64
                / self.btree_lookup_calls.load(Ordering::Relaxed) as f64
        } else {
            0.0
        };
        println!("  Avg B+Tree depth per lookup:   {:.1}", avg_depth);

        println!("\n═══════════════════════════════════════════════════════════\n");
    }

    /// Get a snapshot of all counter values as a struct
    pub fn snapshot(&self) -> ForensicSnapshot {
        ForensicSnapshot {
            // Write path
            logical_insert_node_calls: self.logical_insert_node_calls.load(Ordering::Relaxed),
            logical_insert_edge_calls: self.logical_insert_edge_calls.load(Ordering::Relaxed),
            btree_insert_calls: self.btree_insert_calls.load(Ordering::Relaxed),
            btree_split_count: self.btree_split_count.load(Ordering::Relaxed),
            page_allocate_count: self.page_allocate_count.load(Ordering::Relaxed),
            page_write_count: self.page_write_count.load(Ordering::Relaxed),
            wal_append_count: self.wal_append_count.load(Ordering::Relaxed),
            wal_flush_count: self.wal_flush_count.load(Ordering::Relaxed),
            checkpoint_count: self.checkpoint_count.load(Ordering::Relaxed),
            node_encode_count: self.node_encode_count.load(Ordering::Relaxed),
            edge_encode_count: self.edge_encode_count.load(Ordering::Relaxed),
            sync_data_count: self.sync_data_count.load(Ordering::Relaxed),
            sync_all_count: self.sync_all_count.load(Ordering::Relaxed),

            // Read path
            logical_get_node_calls: self.logical_get_node_calls.load(Ordering::Relaxed),
            logical_neighbors_calls: self.logical_neighbors_calls.load(Ordering::Relaxed),
            btree_lookup_calls: self.btree_lookup_calls.load(Ordering::Relaxed),
            page_read_count: self.page_read_count.load(Ordering::Relaxed),
            node_decode_count: self.node_decode_count.load(Ordering::Relaxed),
            edge_decode_count: self.edge_decode_count.load(Ordering::Relaxed),

            // Cache performance
            btree_cache_hit_count: self.btree_cache_hit_count.load(Ordering::Relaxed),
            btree_cache_miss_count: self.btree_cache_miss_count.load(Ordering::Relaxed),
            node_cache_hit_count: self.node_cache_hit_count.load(Ordering::Relaxed),
            node_cache_miss_count: self.node_cache_miss_count.load(Ordering::Relaxed),

            // Lock contention
            btree_read_lock_count: self.btree_read_lock_count.load(Ordering::Relaxed),
            btree_write_lock_count: self.btree_write_lock_count.load(Ordering::Relaxed),

            // Phase 2: Enhanced tracking
            dirty_page_hit_count: self.dirty_page_hit_count.load(Ordering::Relaxed),
            node_page_cache_hit_count: self.node_page_cache_hit_count.load(Ordering::Relaxed),
            node_page_cache_miss_count: self.node_page_cache_miss_count.load(Ordering::Relaxed),
            redundant_page_reload_count: self.redundant_page_reload_count.load(Ordering::Relaxed),
            edge_cache_hit_count: self.edge_cache_hit_count.load(Ordering::Relaxed),
            edge_cache_miss_count: self.edge_cache_miss_count.load(Ordering::Relaxed),
            edge_page_read_count: self.edge_page_read_count.load(Ordering::Relaxed),
            last_btree_pages_read: self.last_btree_pages_read.load(Ordering::Relaxed),
            last_node_pages_read: self.last_node_pages_read.load(Ordering::Relaxed),

            // Phase 3: Node unpack cost breakdown
            node_page_unpack_count: self.node_page_unpack_count.load(Ordering::Relaxed),
            node_linear_scan_steps: self.node_linear_scan_steps.load(Ordering::Relaxed),
            btree_traversal_depth_total: self.btree_traversal_depth_total.load(Ordering::Relaxed),
        }
    }
}

/// A snapshot of counter values at a point in time
#[derive(Debug, Clone, Copy)]
pub struct ForensicSnapshot {
    // Write path
    pub logical_insert_node_calls: u64,
    pub logical_insert_edge_calls: u64,
    pub btree_insert_calls: u64,
    pub btree_split_count: u64,
    pub page_allocate_count: u64,
    pub page_write_count: u64,
    pub wal_append_count: u64,
    pub wal_flush_count: u64,
    pub checkpoint_count: u64,
    pub node_encode_count: u64,
    pub edge_encode_count: u64,
    pub sync_data_count: u64,
    pub sync_all_count: u64,

    // Read path
    pub logical_get_node_calls: u64,
    pub logical_neighbors_calls: u64,
    pub btree_lookup_calls: u64,
    pub page_read_count: u64,
    pub node_decode_count: u64,
    pub edge_decode_count: u64,

    // Cache performance
    pub btree_cache_hit_count: u64,
    pub btree_cache_miss_count: u64,
    pub node_cache_hit_count: u64,
    pub node_cache_miss_count: u64,

    // Lock contention
    pub btree_read_lock_count: u64,
    pub btree_write_lock_count: u64,

    // Phase 2: Enhanced tracking
    pub dirty_page_hit_count: u64,
    pub node_page_cache_hit_count: u64,
    pub node_page_cache_miss_count: u64,
    pub redundant_page_reload_count: u64,
    pub edge_cache_hit_count: u64,
    pub edge_cache_miss_count: u64,
    pub edge_page_read_count: u64,
    pub last_btree_pages_read: u64,
    pub last_node_pages_read: u64,

    // Phase 3: Node unpack cost breakdown
    pub node_page_unpack_count: u64,
    pub node_linear_scan_steps: u64,
    pub btree_traversal_depth_total: u64,
}

impl ForensicSnapshot {
    /// Calculate the difference between two snapshots
    pub fn diff(&self, after: &ForensicSnapshot) -> ForensicDelta {
        ForensicDelta {
            logical_insert_node_calls: after
                .logical_insert_node_calls
                .wrapping_sub(self.logical_insert_node_calls),
            logical_insert_edge_calls: after
                .logical_insert_edge_calls
                .wrapping_sub(self.logical_insert_edge_calls),
            btree_insert_calls: after
                .btree_insert_calls
                .wrapping_sub(self.btree_insert_calls),
            btree_split_count: after.btree_split_count.wrapping_sub(self.btree_split_count),
            page_allocate_count: after
                .page_allocate_count
                .wrapping_sub(self.page_allocate_count),
            page_write_count: after.page_write_count.wrapping_sub(self.page_write_count),
            wal_append_count: after.wal_append_count.wrapping_sub(self.wal_append_count),
            wal_flush_count: after.wal_flush_count.wrapping_sub(self.wal_flush_count),
            checkpoint_count: after.checkpoint_count.wrapping_sub(self.checkpoint_count),
            node_encode_count: after.node_encode_count.wrapping_sub(self.node_encode_count),
            edge_encode_count: after.edge_encode_count.wrapping_sub(self.edge_encode_count),
            sync_data_count: after.sync_data_count.wrapping_sub(self.sync_data_count),
            sync_all_count: after.sync_all_count.wrapping_sub(self.sync_all_count),

            logical_get_node_calls: after
                .logical_get_node_calls
                .wrapping_sub(self.logical_get_node_calls),
            logical_neighbors_calls: after
                .logical_neighbors_calls
                .wrapping_sub(self.logical_neighbors_calls),
            btree_lookup_calls: after
                .btree_lookup_calls
                .wrapping_sub(self.btree_lookup_calls),
            page_read_count: after.page_read_count.wrapping_sub(self.page_read_count),
            node_decode_count: after.node_decode_count.wrapping_sub(self.node_decode_count),
            edge_decode_count: after.edge_decode_count.wrapping_sub(self.edge_decode_count),

            btree_cache_hit_count: after
                .btree_cache_hit_count
                .wrapping_sub(self.btree_cache_hit_count),
            btree_cache_miss_count: after
                .btree_cache_miss_count
                .wrapping_sub(self.btree_cache_miss_count),
            node_cache_hit_count: after
                .node_cache_hit_count
                .wrapping_sub(self.node_cache_hit_count),
            node_cache_miss_count: after
                .node_cache_miss_count
                .wrapping_sub(self.node_cache_miss_count),

            btree_read_lock_count: after
                .btree_read_lock_count
                .wrapping_sub(self.btree_read_lock_count),
            btree_write_lock_count: after
                .btree_write_lock_count
                .wrapping_sub(self.btree_write_lock_count),

            // Phase 2: Enhanced tracking
            dirty_page_hit_count: after
                .dirty_page_hit_count
                .wrapping_sub(self.dirty_page_hit_count),
            node_page_cache_hit_count: after
                .node_page_cache_hit_count
                .wrapping_sub(self.node_page_cache_hit_count),
            node_page_cache_miss_count: after
                .node_page_cache_miss_count
                .wrapping_sub(self.node_page_cache_miss_count),
            redundant_page_reload_count: after
                .redundant_page_reload_count
                .wrapping_sub(self.redundant_page_reload_count),
            edge_cache_hit_count: after
                .edge_cache_hit_count
                .wrapping_sub(self.edge_cache_hit_count),
            edge_cache_miss_count: after
                .edge_cache_miss_count
                .wrapping_sub(self.edge_cache_miss_count),
            edge_page_read_count: after
                .edge_page_read_count
                .wrapping_sub(self.edge_page_read_count),
            last_btree_pages_read: after
                .last_btree_pages_read
                .wrapping_sub(self.last_btree_pages_read),
            last_node_pages_read: after
                .last_node_pages_read
                .wrapping_sub(self.last_node_pages_read),

            // Phase 3: Node unpack cost breakdown
            node_page_unpack_count: after
                .node_page_unpack_count
                .wrapping_sub(self.node_page_unpack_count),
            node_linear_scan_steps: after
                .node_linear_scan_steps
                .wrapping_sub(self.node_linear_scan_steps),
            btree_traversal_depth_total: after
                .btree_traversal_depth_total
                .wrapping_sub(self.btree_traversal_depth_total),
        }
    }
}

/// The difference between two counter snapshots
#[derive(Debug, Clone, Copy)]
pub struct ForensicDelta {
    // Write path
    pub logical_insert_node_calls: u64,
    pub logical_insert_edge_calls: u64,
    pub btree_insert_calls: u64,
    pub btree_split_count: u64,
    pub page_allocate_count: u64,
    pub page_write_count: u64,
    pub wal_append_count: u64,
    pub wal_flush_count: u64,
    pub checkpoint_count: u64,
    pub node_encode_count: u64,
    pub edge_encode_count: u64,
    pub sync_data_count: u64,
    pub sync_all_count: u64,

    // Read path
    pub logical_get_node_calls: u64,
    pub logical_neighbors_calls: u64,
    pub btree_lookup_calls: u64,
    pub page_read_count: u64,
    pub node_decode_count: u64,
    pub edge_decode_count: u64,

    // Cache performance
    pub btree_cache_hit_count: u64,
    pub btree_cache_miss_count: u64,
    pub node_cache_hit_count: u64,
    pub node_cache_miss_count: u64,

    // Lock contention
    pub btree_read_lock_count: u64,
    pub btree_write_lock_count: u64,

    // Phase 2: Enhanced tracking
    pub dirty_page_hit_count: u64,
    pub node_page_cache_hit_count: u64,
    pub node_page_cache_miss_count: u64,
    pub redundant_page_reload_count: u64,
    pub edge_cache_hit_count: u64,
    pub edge_cache_miss_count: u64,
    pub edge_page_read_count: u64,
    pub last_btree_pages_read: u64,
    pub last_node_pages_read: u64,

    // Phase 3: Node unpack cost breakdown
    pub node_page_unpack_count: u64,
    pub node_linear_scan_steps: u64,
    pub btree_traversal_depth_total: u64,
}

impl ForensicDelta {
    /// Print the delta as a formatted report
    pub fn print_report(&self) {
        println!("\n───────────────────────────────────────────────────────────────");
        println!("                    FORENSIC DELTA REPORT                       ");
        println!("───────────────────────────────────────────────────────────────\n");

        println!("WRITE PATH (per operation):");
        println!(
            "  B+Tree insert calls:           {}",
            self.btree_insert_calls
        );
        println!(
            "  B+Tree split count:            {}",
            self.btree_split_count
        );
        println!(
            "  Page allocations:              {}",
            self.page_allocate_count
        );
        println!("  Page writes (total):           {}", self.page_write_count);
        println!("  WAL appends:                   {}", self.wal_append_count);
        println!("  WAL flushes:                   {}", self.wal_flush_count);
        println!("  Checkpoint count:              {}", self.checkpoint_count);
        println!(
            "  Node encodes:                  {}",
            self.node_encode_count
        );
        println!(
            "  Edge encodes:                  {}",
            self.edge_encode_count
        );
        println!("  sync_data() calls:             {}", self.sync_data_count);
        println!("  sync_all() calls:              {}", self.sync_all_count);

        println!("\nREAD PATH (per operation):");
        println!(
            "  B+Tree lookup calls:           {}",
            self.btree_lookup_calls
        );
        println!("  Page reads (total):            {}", self.page_read_count);
        println!(
            "  Node decodes:                  {}",
            self.node_decode_count
        );
        println!(
            "  Edge decodes:                  {}",
            self.edge_decode_count
        );

        println!("\nCACHE PERFORMANCE (per operation):");
        let btree_total = self.btree_cache_hit_count + self.btree_cache_miss_count;
        let btree_hit_rate = if btree_total > 0 {
            (self.btree_cache_hit_count as f64 / btree_total as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "  B+Tree cache hits/misses:       {}/{} ({:.1}%)",
            self.btree_cache_hit_count, btree_total, btree_hit_rate
        );

        let node_total = self.node_cache_hit_count + self.node_cache_miss_count;
        let node_hit_rate = if node_total > 0 {
            (self.node_cache_hit_count as f64 / node_total as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "  Node cache hits/misses:        {}/{} ({:.1}%)",
            self.node_cache_hit_count, node_total, node_hit_rate
        );

        println!("\nPHASE 2: CACHE RESIDENCY (per operation):");
        println!(
            "  Dirty page hits:               {}",
            self.dirty_page_hit_count
        );
        println!(
            "  Node page cache hits:          {}",
            self.node_page_cache_hit_count
        );
        println!(
            "  Node page cache misses:        {}",
            self.node_page_cache_miss_count
        );
        println!(
            "  Redundant page reloads:        {}",
            self.redundant_page_reload_count
        );

        println!("\nPHASE 2: EDGE PATH (per operation):");
        println!(
            "  Edge cache hits/misses:        {}/{}",
            self.edge_cache_hit_count,
            self.edge_cache_hit_count + self.edge_cache_miss_count
        );
        println!(
            "  Edge page reads:               {}",
            self.edge_page_read_count
        );

        println!("\nPHASE 3: UNPACK COST BREAKDOWN (per operation):");
        println!(
            "  NodePage::unpack() calls:      {}",
            self.node_page_unpack_count
        );
        println!(
            "  Linear scan steps:             {}",
            self.node_linear_scan_steps
        );
        let avg_scan = if self.node_page_unpack_count > 0 {
            self.node_linear_scan_steps as f64 / self.node_page_unpack_count as f64
        } else {
            0.0
        };
        println!("  Avg nodes scanned per unpack:  {:.1}", avg_scan);
        println!(
            "  B+Tree traversal depth:        {}",
            self.btree_traversal_depth_total
        );
        let avg_depth = if self.btree_lookup_calls > 0 {
            self.btree_traversal_depth_total as f64 / self.btree_lookup_calls as f64
        } else {
            0.0
        };
        println!("  Avg B+Tree depth per lookup:   {:.1}", avg_depth);

        println!("\n───────────────────────────────────────────────────────────────\n");
    }
}

/// Helper macros for tracking page ownership
///
/// These macros make it easy to track page allocations and writes from
/// anywhere in the V3 codebase. They are feature-gated to v3-forensics
/// to have zero overhead in production builds.
///
/// # Usage
///
/// ```rust,ignore
/// // Track a page allocation
/// track_page_alloc!(page_id, Subsystem::NodeStore, PageType::Node);
///
/// // Track a page write
/// track_page_write!(page_id, Subsystem::BTreeManager, PageType::BTree, file_offset, "write_page");
/// ```
///
/// # Examples
///
/// In NodeStore:
/// ```rust,ignore
/// track_page_alloc!(new_page_id, Subsystem::NodeStore, PageType::Node);
/// track_page_write!(page_id, Subsystem::NodeStore, PageType::Node, offset, "write_node_page");
/// ```
///
/// In BTreeManager:
/// ```rust,ignore
/// track_page_alloc!(new_page_id, Subsystem::BTreeManager, PageType::BTree);
/// track_page_write!(page_id, Subsystem::BTreeManager, PageType::BTree, offset, "write_page");
/// ```
///
/// In EdgeStore:
/// ```rust,ignore
/// track_page_alloc!(new_page_id, Subsystem::EdgeStore, PageType::Edge);
/// track_page_write!(page_id, Subsystem::EdgeStore, PageType::Edge, offset, "write_page_to_disk");
/// ```

#[macro_export]
macro_rules! track_page_alloc {
    ($page_id:expr, $subsystem:expr, $page_type:expr) => {
        #[cfg(feature = "v3-forensics")]
        {
            let mut registry = $crate::backend::native::v3::forensics::get_page_registry().lock();
            registry.register_allocation($page_id, $subsystem, $page_type);
        }
    };
}

#[macro_export]
macro_rules! track_page_write {
    ($page_id:expr, $subsystem:expr, $page_type:expr, $offset:expr, $function:expr) => {
        #[cfg(feature = "v3-forensics")]
        {
            let mut registry = $crate::backend::native::v3::forensics::get_page_registry().lock();
            let has_conflict = registry.register_write(
                $page_id,
                $subsystem,
                $page_type,
                $offset,
                $function.to_string(),
            );
            if has_conflict {
                eprintln!("⚠️  PAGE OWNERSHIP CONFLICT: page_id={}, subsystem={:?}, page_type={:?}, offset={}",
                    $page_id, $subsystem, $page_type, $offset);
            }
        }
    };
}

#[macro_export]
macro_rules! track_page_write_auto {
    ($page_id:expr, $subsystem:expr, $bytes:expr, $offset:expr, $function:expr) => {
        #[cfg(feature = "v3-forensics")]
        {
            let page_type =
                $crate::backend::native::v3::forensics::PageType::detect_from_bytes($bytes);
            $crate::track_page_write!($page_id, $subsystem, page_type, $offset, $function);
        }

        #[cfg(not(feature = "v3-forensics"))]
        {};
    };
}

/// Print the page ownership conflict report
pub fn print_page_ownership_report() {
    let registry = get_page_registry().lock();
    registry.print_conflict_report();
}

/// Print the page ownership map
pub fn print_page_ownership_map() {
    let registry = get_page_registry().lock();
    registry.print_ownership_map();
}

/// Check if there are any page ownership conflicts
pub fn has_page_conflicts() -> bool {
    let registry = get_page_registry().lock();
    registry.total_conflicts() > 0
}

/// Get the first conflicting page ID
pub fn get_first_conflict_page() -> Option<u64> {
    let registry = get_page_registry().lock();
    registry.first_conflict_page()
}

/// Reset page ownership tracking
pub fn reset_page_ownership() {
    let mut registry = get_page_registry().lock();
    registry.reset();
}

/// Run a detailed page ownership scan of a database file
///
/// This function reads all pages from a database file and attempts to
/// identify page types, then checks for inconsistencies in the registry.
pub fn scan_database_pages(db_path: &std::path::Path) -> Result<PageScanReport, std::io::Error> {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    let mut file = File::open(db_path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len();

    const PAGE_SIZE: u64 = 4096;
    const V3_HEADER_SIZE: u64 = 112;

    let mut report = PageScanReport {
        total_pages: 0,
        node_pages: 0,
        btree_pages: 0,
        edge_pages: 0,
        unknown_pages: 0,
        corrupted_pages: Vec::new(),
    };

    let mut page_buffer = vec![0u8; PAGE_SIZE as usize];

    // Skip header, scan data pages
    let mut offset = V3_HEADER_SIZE;
    let mut page_id = 1u64;

    while offset + PAGE_SIZE <= file_size {
        file.seek(std::io::SeekFrom::Start(offset))?;
        file.read_exact(&mut page_buffer)?;

        let page_type = PageType::detect_from_bytes(&page_buffer);

        match page_type {
            PageType::Node => report.node_pages += 1,
            PageType::BTree => report.btree_pages += 1,
            PageType::Edge => report.edge_pages += 1,
            PageType::Header => {} // Should not see header in data pages
            PageType::Unknown => {
                report.unknown_pages += 1;
                // Check for corruption indicators
                if page_buffer.len() >= 32 {
                    let used_bytes = u16::from_be_bytes([page_buffer[18], page_buffer[19]]);
                    if used_bytes > 4000 {
                        report.corrupted_pages.push(PageCorruption {
                            page_id,
                            offset,
                            detected_type: page_type,
                            used_bytes,
                            first_bytes: page_buffer[..32].to_vec(),
                        });
                    }
                }
            }
        }

        report.total_pages += 1;
        offset += PAGE_SIZE;
        page_id += 1;
    }

    Ok(report)
}

/// Result of scanning database pages
#[derive(Debug)]
pub struct PageScanReport {
    pub total_pages: usize,
    pub node_pages: usize,
    pub btree_pages: usize,
    pub edge_pages: usize,
    pub unknown_pages: usize,
    pub corrupted_pages: Vec<PageCorruption>,
}

impl PageScanReport {
    pub fn print(&self) {
        println!("\n═══════════════════════════════════════════════════════════");
        println!("                 DATABASE PAGE SCAN REPORT                 ");
        println!("═══════════════════════════════════════════════════════════\n");

        println!("Total pages scanned: {}", self.total_pages);
        println!("  Node pages: {}", self.node_pages);
        println!("  B+Tree pages: {}", self.btree_pages);
        println!("  Edge pages: {}", self.edge_pages);
        println!("  Unknown pages: {}", self.unknown_pages);

        if !self.corrupted_pages.is_empty() {
            println!(
                "\n⚠️  CORRUPTED PAGES DETECTED: {}",
                self.corrupted_pages.len()
            );
            for corruption in self.corrupted_pages.iter().take(10) {
                println!(
                    "  Page {} at offset {}:",
                    corruption.page_id, corruption.offset
                );
                println!("    Detected type: {:?}", corruption.detected_type);
                println!(
                    "    used_bytes: {} (0x{:04x})",
                    corruption.used_bytes, corruption.used_bytes
                );
                println!("    First 32 bytes: {:?}", &corruption.first_bytes[..]);
            }
            if self.corrupted_pages.len() > 10 {
                println!("  ... and {} more", self.corrupted_pages.len() - 10);
            }
        } else {
            println!("\n✓ No corruption detected in page headers");
        }

        println!("\n═══════════════════════════════════════════════════════════\n");
    }
}

/// Page corruption record
#[derive(Debug)]
pub struct PageCorruption {
    pub page_id: u64,
    pub offset: u64,
    pub detected_type: PageType,
    pub used_bytes: u16,
    pub first_bytes: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_type_detection_node_page() {
        // NodePage header: page_id(8) + next_page_id(8) + node_count(2) + used_bytes(2) + base_id(8) + checksum(4)
        let mut node_page_bytes = vec![0u8; 4096];

        // page_id = 1
        node_page_bytes[0..8].copy_from_slice(&1u64.to_be_bytes());
        // next_page_id = 0
        node_page_bytes[8..16].copy_from_slice(&0u64.to_be_bytes());
        // node_count = 10
        node_page_bytes[16..18].copy_from_slice(&10u16.to_be_bytes());
        // used_bytes = 512 (reasonable value)
        node_page_bytes[18..20].copy_from_slice(&512u16.to_be_bytes());

        let detected = PageType::detect_from_bytes(&node_page_bytes);
        assert_eq!(detected, PageType::Node);
    }

    #[test]
    fn test_page_type_detection_btree_leaf() {
        // IndexPage header: page_id(8) + is_leaf(1) + is_root(1) + count(2) + checksum(4) + padding(16)
        let mut btree_bytes = vec![0u8; 4096];

        // page_id = 2
        btree_bytes[0..8].copy_from_slice(&2u64.to_be_bytes());
        // is_leaf = 1
        btree_bytes[8] = 1;
        // is_root = 0
        btree_bytes[9] = 0;

        let detected = PageType::detect_from_bytes(&btree_bytes);
        assert_eq!(detected, PageType::BTree);
    }

    #[test]
    fn test_page_ownership_registry() {
        let mut registry = PageOwnershipRegistry::new();

        // Register initial allocation
        registry.register_allocation(1, Subsystem::NodeStore, PageType::Node);

        // Register write from same subsystem
        let conflict1 = registry.register_write(
            1,
            Subsystem::NodeStore,
            PageType::Node,
            4208,
            "write_node_page".to_string(),
        );
        assert!(!conflict1);

        // Register write from different subsystem (conflict!)
        let conflict2 = registry.register_write(
            1,
            Subsystem::BTreeManager,
            PageType::BTree,
            4208,
            "write_page".to_string(),
        );
        assert!(conflict2);

        // Check registry state
        assert_eq!(registry.total_conflicts(), 1);
        assert_eq!(registry.first_conflict_page(), Some(1));

        let record = registry.get(1).unwrap();
        assert!(record.has_conflict);
        assert_eq!(record.writes.len(), 2);
    }
}
