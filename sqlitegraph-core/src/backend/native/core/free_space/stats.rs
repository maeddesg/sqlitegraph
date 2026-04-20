use super::block::FreeBlock;

#[derive(Debug, Clone, Default)]
pub struct AllocationStats {
    pub total_allocations: u64,
    pub total_deallocations: u64,
    pub total_allocated_bytes: u64,
    pub total_deallocated_bytes: u64,
    pub block_splits: u64,
    pub block_merges: u64,
    pub fragmentation_ratio: f64,
}

impl AllocationStats {
    pub fn efficiency(&self) -> f64 {
        if self.total_allocated_bytes > 0 {
            (self.total_allocated_bytes - self.total_deallocated_bytes) as f64
                / self.total_allocated_bytes as f64
        } else {
            1.0
        }
    }

    pub fn net_allocated_bytes(&self) -> i64 {
        self.total_allocated_bytes as i64 - self.total_deallocated_bytes as i64
    }
}

#[derive(Debug, Clone)]
pub struct CompactionReport {
    pub initial_fragments: usize,
    pub final_fragments: usize,
    pub initial_fragmentation_ratio: f64,
    pub final_fragmentation_ratio: f64,
    pub small_fragments_removed: usize,
    pub blocks_merged: u64,
}

impl CompactionReport {
    pub fn was_effective(&self) -> bool {
        self.final_fragments < self.initial_fragments
            || self.final_fragmentation_ratio < self.initial_fragmentation_ratio
    }

    pub fn improvement_summary(&self) -> String {
        format!(
            "Compaction: {}→{} fragments ({:.1}% reduction), {:.1}→{:.1}% fragmentation",
            self.initial_fragments,
            self.final_fragments,
            (1.0 - self.final_fragments as f64 / self.initial_fragments as f64) * 100.0,
            self.initial_fragmentation_ratio * 100.0,
            self.final_fragmentation_ratio * 100.0
        )
    }
}

#[derive(Debug, Clone)]
pub struct FreeSpaceAnalysis {
    pub total_blocks: usize,
    pub total_free_bytes: u64,
    pub largest_block: u32,
    pub average_block_size: u64,
    pub fragmentation_ratio: f64,
    pub blocks_by_size: Vec<(u32, u64)>,
    pub size_histogram: Vec<(u32, usize)>,
}

impl FreeSpaceAnalysis {
    pub fn describe(&self) -> String {
        format!(
            "{} blocks / {} bytes free, largest {} bytes, fragmentation {:.1}%",
            self.total_blocks,
            self.total_free_bytes,
            self.largest_block,
            self.fragmentation_ratio * 100.0
        )
    }

    pub fn block_examples(&self) -> Vec<FreeBlock> {
        self.blocks_by_size
            .iter()
            .map(|(size, offset)| FreeBlock::new(*offset, *size))
            .collect()
    }
}
