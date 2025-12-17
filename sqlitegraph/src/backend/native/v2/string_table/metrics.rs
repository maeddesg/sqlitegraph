use super::StringTable;
use crate::backend::native::{NativeBackendError, NativeResult};

/// Summary information about how effective the table is at deduplication.
#[derive(Debug, Clone)]
pub struct CompressionMetrics {
    pub total_unique_strings: usize,
    pub total_string_bytes: usize,
    pub total_storage_bytes: usize,
    pub compression_ratio: f64,
}

impl StringTable {
    pub fn compression_metrics(&self) -> CompressionMetrics {
        let total_string_bytes: usize = self.strings.iter().map(|s| s.as_bytes().len()).sum();
        let total_storage_bytes = self.serialized_size();
        CompressionMetrics {
            total_unique_strings: self.strings.len(),
            total_string_bytes,
            total_storage_bytes,
            compression_ratio: if total_string_bytes > 0 {
                total_storage_bytes as f64 / total_string_bytes as f64
            } else {
                0.0
            },
        }
    }

    pub fn validate(&self) -> NativeResult<()> {
        for i in 1..self.offsets.len() {
            if self.offsets[i] <= self.offsets[i - 1] {
                return Err(NativeBackendError::CorruptStringTable {
                    reason: format!(
                        "Non-monotonic offset at index {}: {} <= {}",
                        i,
                        self.offsets[i],
                        self.offsets[i - 1]
                    ),
                });
            }
        }

        for string in &self.strings {
            std::str::from_utf8(string.as_bytes()).map_err(|e| {
                NativeBackendError::CorruptStringTable {
                    reason: format!("Invalid UTF-8 in string table: {}", e),
                }
            })?;
        }

        let mut calculated_offset = 0u32;
        for (i, string) in self.strings.iter().enumerate() {
            if self.offsets[i] != calculated_offset {
                return Err(NativeBackendError::CorruptStringTable {
                    reason: format!(
                        "Offset mismatch at index {}: expected {}, found {}",
                        i, calculated_offset, self.offsets[i]
                    ),
                });
            }
            calculated_offset += string.len() as u32;
        }

        Ok(())
    }
}
