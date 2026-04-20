//! File Coordinator for V3 Backend
//!
//! Coordinates all main DB file I/O through a single file handle to prevent
//! race conditions between NodeStore, BTreeManager, and V3EdgeStore.
//!
//! **Root Cause Fixed:** Multiple components opening independent file handles
//! caused inconsistent file size metadata and data corruption during concurrent
//! page writes.
//!
//! **Solution:** Single shared file handle with mutex-protected write operations.

use crate::backend::native::v3::constants::{DEFAULT_PAGE_SIZE, V3_HEADER_SIZE};
use crate::backend::native::{NativeBackendError, NativeResult};
use parking_lot::Mutex;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;

/// Coordinated file handle for all V3 main DB file I/O
///
/// All writes to the main DB file MUST go through this coordinator to ensure
/// file size metadata consistency and prevent race conditions.
pub struct FileCoordinator {
    /// The underlying file handle (kept open for the lifetime of the coordinator)
    file: Mutex<CoordinatedFile>,
    /// Path to the database file (for reopen on error)
    db_path: std::path::PathBuf,
}

/// Inner file handle with coordination logic
struct CoordinatedFile {
    file: std::fs::File,
    /// Cached file size to avoid repeated metadata() calls
    cached_size: u64,
}

impl FileCoordinator {
    /// Create a new file coordinator for the given database path
    ///
    /// Opens the file in read-write mode. If the file doesn't exist, it will be
    /// created when the first write occurs.
    pub fn create(db_path: &std::path::Path) -> NativeResult<Self> {
        // Open file - create if doesn't exist, but don't truncate
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true) // Create if doesn't exist, no-op if exists
            .open(db_path)
            .map_err(|e| NativeBackendError::IoError {
                context: format!(
                    "Failed to open db file for coordination: {}",
                    db_path.display()
                ),
                source: e,
            })?;

        // Get initial file size
        let cached_size = file.metadata().map(|m| m.len()).unwrap_or(0);

        Ok(Self {
            file: Mutex::new(CoordinatedFile { file, cached_size }),
            db_path: db_path.to_path_buf(),
        })
    }

    /// Write a page of data to the file at the specified offset
    ///
    /// This method:
    /// 1. Seeks to the offset (automatically extends file if needed)
    /// 2. Writes the data
    /// 3. Syncs to ensure durability
    /// 4. Updates the cached size
    ///
    /// All operations are protected by a mutex to ensure atomicity.
    pub fn write_page(&self, page_id: u64, data: &[u8]) -> NativeResult<()> {
        let mut coord = self.file.lock();

        // Calculate offset
        let offset = Self::page_offset(page_id);
        let required_len = offset + data.len() as u64;

        // Seek to offset (extends file automatically if beyond EOF)
        coord
            .file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to seek to page {} at offset {}", page_id, offset),
                source: e,
            })?;

        // Write data
        coord
            .file
            .write_all(data)
            .map_err(|e| NativeBackendError::IoError {
                context: format!(
                    "Failed to write page {} data ({} bytes)",
                    page_id,
                    data.len()
                ),
                source: e,
            })?;

        // Sync to disk - ensures both data and metadata are flushed
        coord
            .file
            .sync_all()
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to sync page {} write", page_id),
                source: e,
            })?;

        // Update cached size to actual file size (may have grown)
        let actual_size = coord.file.metadata().map(|m| m.len()).unwrap_or(0);
        coord.cached_size = actual_size;

        Ok(())
    }

    /// Read a page of data from the file at the specified offset
    ///
    /// Reads exactly `buffer.len()` bytes from the file. Returns an error if
    /// the file is shorter than expected (e.g., reading beyond EOF).
    pub fn read_page(&self, page_id: u64, buffer: &mut [u8]) -> NativeResult<()> {
        let mut coord = self.file.lock();

        // Calculate offset
        let offset = Self::page_offset(page_id);
        let required_len = offset + buffer.len() as u64;

        // CRITICAL: Check if file is large enough before reading
        // This provides a better error message than UnexpectedEof
        if coord.cached_size < required_len {
            return Err(NativeBackendError::IoError {
                context: format!(
                    "File too small to read page {}: cached_size={} < required_len={}",
                    page_id, coord.cached_size, required_len
                ),
                source: std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!(
                        "file size {} < required {}",
                        coord.cached_size, required_len
                    ),
                ),
            });
        }

        // Seek to offset
        coord
            .file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to seek to page {} at offset {}", page_id, offset),
                source: e,
            })?;

        // Read exact number of bytes - fail if file is too short
        coord
            .file
            .read_exact(buffer)
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to read page {} from disk", page_id),
                source: e,
            })?;

        Ok(())
    }

    /// Write raw data at a specific offset (for external node data)
    ///
    /// Used by V3Backend for storing large node data that doesn't fit inline.
    /// This method extends the file if needed and writes the data atomically.
    pub fn write_data_at_offset(&self, offset: u64, data: &[u8]) -> NativeResult<()> {
        let mut coord = self.file.lock();

        let required_len = offset + data.len() as u64;

        // Seek to offset (extends file automatically if needed)
        coord
            .file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| NativeBackendError::IoError {
                context: format!("Failed to seek to offset {}", offset),
                source: e,
            })?;

        // Write data
        coord
            .file
            .write_all(data)
            .map_err(|e| NativeBackendError::IoError {
                context: "Failed to write external data".to_string(),
                source: e,
            })?;

        // Sync to disk
        coord
            .file
            .sync_all()
            .map_err(|e| NativeBackendError::IoError {
                context: "Failed to sync external data".to_string(),
                source: e,
            })?;

        // Update cached size
        if required_len > coord.cached_size {
            coord.cached_size = required_len;
        }

        Ok(())
    }

    /// Get the current file size
    pub fn file_size(&self) -> u64 {
        self.file.lock().cached_size
    }

    /// Flush all pending writes to disk
    pub fn sync_all(&self) -> NativeResult<()> {
        self.file
            .lock()
            .file
            .sync_all()
            .map_err(|e| NativeBackendError::IoError {
                context: "Failed to sync file".to_string(),
                source: e,
            })
    }

    /// Calculate page offset from page ID
    fn page_offset(page_id: u64) -> u64 {
        if page_id == 0 {
            0
        } else {
            V3_HEADER_SIZE as u64 + (page_id - 1) * DEFAULT_PAGE_SIZE
        }
    }

    /// Get the database path
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_coordinator_create() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("test.graph");

        let coordinator = FileCoordinator::create(&db_path).unwrap();
        assert_eq!(coordinator.file_size(), 0);
        assert_eq!(coordinator.db_path(), db_path);
    }

    #[test]
    fn test_write_and_read_page() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("test.graph");

        let coordinator = FileCoordinator::create(&db_path).unwrap();

        // Write page 1
        let data1 = vec![1u8; 4096];
        coordinator.write_page(1, &data1).unwrap();
        assert_eq!(coordinator.file_size(), V3_HEADER_SIZE as u64 + 4096);

        // Write page 2
        let data2 = vec![2u8; 4096];
        coordinator.write_page(2, &data2).unwrap();
        assert_eq!(coordinator.file_size(), V3_HEADER_SIZE as u64 + 8192);

        // Read back
        let mut buffer = vec![0u8; 4096];
        coordinator.read_page(1, &mut buffer).unwrap();
        assert_eq!(buffer, data1);

        coordinator.read_page(2, &mut buffer).unwrap();
        assert_eq!(buffer, data2);
    }

    #[test]
    fn test_write_extends_file() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("test.graph");

        let coordinator = FileCoordinator::create(&db_path).unwrap();

        // Write page 100 (should extend file)
        let data = vec![42u8; 4096];
        coordinator.write_page(100, &data).unwrap();

        let expected_size = V3_HEADER_SIZE as u64 + (99 * DEFAULT_PAGE_SIZE) + 4096;
        assert_eq!(coordinator.file_size(), expected_size);

        // Read it back
        let mut buffer = vec![0u8; 4096];
        coordinator.read_page(100, &mut buffer).unwrap();
        assert_eq!(buffer, data);
    }
}
