use crate::backend::native::types::FileOffset;

/// Free block stored in offset order.
#[derive(Debug, Clone)]
pub struct FreeBlock {
    pub offset: FileOffset,
    pub size: u32,
    pub recently_freed: bool,
}

impl FreeBlock {
    pub fn new(offset: FileOffset, size: u32) -> Self {
        Self {
            offset,
            size,
            recently_freed: true,
        }
    }

    pub fn can_accommodate(&self, requested_size: u32) -> bool {
        self.size >= requested_size
    }

    pub fn split_if_needed(&mut self, requested_size: u32) -> Option<FreeBlock> {
        if self.size > requested_size + super::manager::MIN_BLOCK_SIZE {
            let remaining_size = self.size - requested_size;
            let remaining_offset = self.offset + requested_size as u64;
            self.size = requested_size;
            return Some(FreeBlock::new(remaining_offset, remaining_size));
        }
        None
    }

    pub fn can_merge_with(&self, other: &FreeBlock) -> bool {
        self.offset + self.size as u64 == other.offset
            || other.offset + other.size as u64 == self.offset
    }

    pub fn merge_with(&mut self, other: &FreeBlock) {
        if self.offset + self.size as u64 == other.offset {
            self.size += other.size;
        } else if other.offset + other.size as u64 == self.offset {
            self.offset = other.offset;
            self.size += other.size;
        }
    }
}
