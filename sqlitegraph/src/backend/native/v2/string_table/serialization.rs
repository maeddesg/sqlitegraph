use super::table::StringTable;
use crate::backend::native::{NativeBackendError, NativeResult};

impl StringTable {
    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&(self.strings.len() as u32).to_be_bytes());

        for (string, &offset) in self.strings.iter().zip(self.offsets.iter()) {
            buffer.extend_from_slice(&offset.to_be_bytes());
            let bytes = string.as_bytes();
            let len = bytes.len().min(u16::MAX as usize) as u16;
            buffer.extend_from_slice(&len.to_be_bytes());
            buffer.extend_from_slice(&bytes[..len as usize]);
        }

        buffer
    }

    pub fn deserialize(bytes: &[u8]) -> NativeResult<Self> {
        if bytes.len() < 4 {
            return Err(NativeBackendError::BufferTooSmall {
                size: bytes.len(),
                min_size: 4,
            });
        }

        let mut offset = 0;
        let string_count = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        let mut strings = Vec::with_capacity(string_count);
        let mut offsets = Vec::with_capacity(string_count);

        for _ in 0..string_count {
            if offset + 6 > bytes.len() {
                return Err(NativeBackendError::BufferTooSmall {
                    size: bytes.len(),
                    min_size: offset + 6,
                });
            }

            let string_offset = u32::from_be_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            offset += 4;

            let string_len = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
            offset += 2;

            if offset + string_len > bytes.len() {
                return Err(NativeBackendError::BufferTooSmall {
                    size: bytes.len(),
                    min_size: offset + string_len,
                });
            }

            let data = &bytes[offset..offset + string_len];
            let string =
                std::str::from_utf8(data).map_err(|e| NativeBackendError::CorruptStringTable {
                    reason: e.to_string(),
                })?;

            strings.push(string.to_string());
            offsets.push(string_offset);
            offset += string_len;
        }

        let mut table = StringTable {
            strings,
            offsets,
            common_edge_types: std::collections::HashMap::new(),
        };
        table.rebuild_common_type_cache();
        Ok(table)
    }

    pub fn serialized_size(&self) -> usize {
        4 + self
            .strings
            .iter()
            .map(|string| 4 + 2 + string.as_bytes().len())
            .sum::<usize>()
    }
}
