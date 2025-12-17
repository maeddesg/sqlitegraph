use crate::backend::native::{NativeBackendError, NativeResult};
use std::collections::HashMap;

const COMMON_EDGE_TYPES: [&str; 16] = [
    "calls",
    "imports",
    "defines",
    "uses",
    "contains",
    "implements",
    "extends",
    "references",
    "declares",
    "exports",
    "inherits",
    "overrides",
    "depends_on",
    "relates_to",
    "includes",
    "aliases",
];

/// Shared string table that deduplicates edge type labels across the file.
#[derive(Debug, Clone)]
pub struct StringTable {
    pub(super) strings: Vec<String>,
    pub(super) offsets: Vec<u32>,
    pub(super) common_edge_types: HashMap<String, u16>,
}

impl StringTable {
    pub fn new() -> Self {
        let mut table = Self {
            strings: Vec::new(),
            offsets: Vec::new(),
            common_edge_types: HashMap::new(),
        };
        table.prepopulate_common_types();
        table
    }

    fn prepopulate_common_types(&mut self) {
        for edge_type in COMMON_EDGE_TYPES {
            let offset = self.add_string_internal(edge_type.to_string());
            self.common_edge_types
                .insert(edge_type.to_string(), offset as u16);
        }
    }

    pub fn get_or_add_offset(&mut self, string: &str) -> NativeResult<u16> {
        if let Some(&offset) = self.common_edge_types.get(string) {
            return Ok(offset);
        }

        if let Some((index, _)) = self
            .strings
            .iter()
            .enumerate()
            .find(|(_, existing)| existing == &string)
        {
            return Ok(self.offsets[index] as u16);
        }

        let offset = self.add_string_internal(string.to_string());
        if offset > u16::MAX as u32 {
            return Err(NativeBackendError::RecordTooLarge {
                size: offset,
                max_size: u16::MAX as u32,
            });
        }
        Ok(offset as u16)
    }

    pub fn get_string(&self, offset: u16) -> NativeResult<&str> {
        let offset_u32 = offset as u32;
        if let Some(index) = self.offsets.iter().position(|&val| val == offset_u32) {
            return Ok(&self.strings[index]);
        }
        Err(NativeBackendError::InvalidStringOffset { offset: offset_u32 })
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    fn add_string_internal(&mut self, string: String) -> u32 {
        let offset = if let Some(last) = self.offsets.last() {
            last + self.strings.last().map(|s| s.len() as u32).unwrap_or(0)
        } else {
            0
        };
        self.strings.push(string);
        self.offsets.push(offset);
        offset
    }

    pub fn rebuild_common_type_cache(&mut self) {
        self.common_edge_types.clear();
        for edge_type in COMMON_EDGE_TYPES {
            if let Some((index, _)) = self
                .strings
                .iter()
                .enumerate()
                .find(|(_, existing)| existing == &edge_type)
            {
                self.common_edge_types
                    .insert(edge_type.to_string(), self.offsets[index] as u16);
            }
        }
    }
}
