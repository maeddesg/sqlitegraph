//! Extension helpers to serialize legacy edge records into compact form.

use crate::backend::native::v2::string_table::StringTable;
use crate::backend::native::{EdgeRecord, NativeBackendError, NativeResult};

/// Extension trait for lossless conversions between `EdgeRecord` and the compact format.
pub trait EdgeRecordCompactExt {
    /// Serialize into the compact binary format using JSON payloads.
    fn serialize_compact(&self) -> NativeResult<Vec<u8>>;
    /// Deserialize from the compact layout, resolving edge types via the shared table.
    fn deserialize_compact(bytes: &[u8], string_table: &StringTable) -> NativeResult<EdgeRecord>;
}

impl EdgeRecordCompactExt for EdgeRecord {
    fn serialize_compact(&self) -> NativeResult<Vec<u8>> {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&self.to_id.to_be_bytes());

        let edge_type_bytes = self.edge_type.as_bytes();
        if edge_type_bytes.len() > u16::MAX as usize {
            return Err(NativeBackendError::RecordTooLarge {
                size: edge_type_bytes.len() as u32,
                max_size: u16::MAX as u32,
            });
        }
        buffer.extend_from_slice(&(edge_type_bytes.len() as u16).to_be_bytes());
        buffer.extend_from_slice(edge_type_bytes);

        let data_bytes = serde_json::to_vec(&self.data)?;
        if data_bytes.len() > u32::MAX as usize {
            return Err(NativeBackendError::RecordTooLarge {
                size: data_bytes.len() as u32,
                max_size: u32::MAX,
            });
        }
        buffer.extend_from_slice(&(data_bytes.len() as u32).to_be_bytes());
        buffer.extend_from_slice(&data_bytes);
        Ok(buffer)
    }

    fn deserialize_compact(bytes: &[u8], _string_table: &StringTable) -> NativeResult<EdgeRecord> {
        if bytes.len() < 14 {
            return Err(NativeBackendError::BufferTooSmall {
                size: bytes.len(),
                min_size: 14,
            });
        }

        let mut cursor = 0;
        let neighbor_id = i64::from_be_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
            bytes[cursor + 4],
            bytes[cursor + 5],
            bytes[cursor + 6],
            bytes[cursor + 7],
        ]);
        cursor += 8;

        let type_len = u16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]) as usize;
        cursor += 2;
        if bytes.len() < cursor + type_len + 4 {
            return Err(NativeBackendError::BufferTooSmall {
                size: bytes.len(),
                min_size: cursor + type_len + 4,
            });
        }
        let edge_type = std::str::from_utf8(&bytes[cursor..cursor + type_len])?.to_string();
        cursor += type_len;

        let data_len = u32::from_be_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
        ]) as usize;
        cursor += 4;
        if bytes.len() < cursor + data_len {
            return Err(NativeBackendError::BufferTooSmall {
                size: bytes.len(),
                min_size: cursor + data_len,
            });
        }
        let data = if data_len == 0 {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes[cursor..cursor + data_len])?
        };

        Ok(EdgeRecord::new(0, 0, neighbor_id, edge_type, data))
    }
}
