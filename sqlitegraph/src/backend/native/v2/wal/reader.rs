//! V2 WAL reader for log reading and recovery operations.
//!
//! This module provides efficient WAL reading capabilities for recovery operations,
//! checkpointing, and log analysis. It supports sequential reading, random access
//! by LSN, and filtered reading based on record types and cluster affinity.

use crate::backend::native::v2::wal::record::V2WALSerializer;
use crate::backend::native::v2::wal::tx_range_index::TxRangeIndex;
use crate::backend::native::v2::wal::{V2WALHeader, V2WALRecord, V2WALRecordType};
use crate::backend::native::{NativeBackendError, NativeResult};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

/// WAL reader for efficient log access and recovery
pub struct V2WALReader {
    /// WAL file handle
    file: Arc<BufReader<File>>,

    /// WAL header
    header: V2WALHeader,

    /// Current file position
    current_position: u64,

    /// End of WAL data
    wal_end: u64,

    /// Transaction range index for snapshot isolation
    tx_index: TxRangeIndex,

    /// Active transaction ID during WAL parsing (for contiguity validation)
    active_tx: Option<u64>,
}

/// WAL reading iterator for sequential access
pub struct WALRecordIterator<'a> {
    reader: &'a mut V2WALReader,
    end_lsn: Option<u64>,
}

/// Filter for WAL record reading
#[derive(Debug, Clone)]
pub struct WALReadFilter {
    /// Record types to include (None = all types)
    pub record_types: Option<Vec<V2WALRecordType>>,

    /// LSN range to include (None = all LSNs)
    pub lsn_range: Option<(u64, u64)>,

    /// Cluster keys to include (None = all clusters)
    pub cluster_keys: Option<Vec<i64>>,

    /// Include only data-modifying records
    pub data_modifying_only: bool,

    /// Include only transaction control records
    pub transaction_control_only: bool,
}

impl WALReadFilter {
    /// Create a new filter that accepts all records
    pub fn all() -> Self {
        Self {
            record_types: None,
            lsn_range: None,
            cluster_keys: None,
            data_modifying_only: false,
            transaction_control_only: false,
        }
    }

    /// Create a filter for specific record types
    pub fn by_types(types: Vec<V2WALRecordType>) -> Self {
        Self {
            record_types: Some(types),
            lsn_range: None,
            cluster_keys: None,
            data_modifying_only: false,
            transaction_control_only: false,
        }
    }

    /// Create a filter for LSN range
    pub fn by_lsn_range(start_lsn: u64, end_lsn: u64) -> Self {
        Self {
            record_types: None,
            lsn_range: Some((start_lsn, end_lsn)),
            cluster_keys: None,
            data_modifying_only: false,
            transaction_control_only: false,
        }
    }

    /// Create a filter for cluster affinity
    pub fn by_cluster_keys(cluster_keys: Vec<i64>) -> Self {
        Self {
            record_types: None,
            lsn_range: None,
            cluster_keys: Some(cluster_keys),
            data_modifying_only: false,
            transaction_control_only: false,
        }
    }

    /// Create a filter for data-modifying records only
    pub fn data_modifying_only() -> Self {
        Self {
            record_types: None,
            lsn_range: None,
            cluster_keys: None,
            data_modifying_only: true,
            transaction_control_only: false,
        }
    }

    /// Create a filter for transaction control records only
    pub fn transaction_control_only() -> Self {
        Self {
            record_types: None,
            lsn_range: None,
            cluster_keys: None,
            data_modifying_only: false,
            transaction_control_only: true,
        }
    }

    /// Check if a record matches this filter
    pub fn matches(&self, record: &V2WALRecord, lsn: u64) -> bool {
        // Check record type filter
        if let Some(ref types) = self.record_types {
            if !types.contains(&record.record_type()) {
                return false;
            }
        }

        // Check LSN range filter
        if let Some((start_lsn, end_lsn)) = self.lsn_range {
            if lsn < start_lsn || lsn > end_lsn {
                return false;
            }
        }

        // Check cluster key filter
        if let Some(ref cluster_keys) = self.cluster_keys {
            if let Some(record_cluster_key) = record.cluster_key() {
                if !cluster_keys.contains(&record_cluster_key) {
                    return false;
                }
            } else {
                return false; // Record has no cluster key but filter requires it
            }
        }

        // Check data-modifying filter
        if self.data_modifying_only && !record.modifies_data() {
            return false;
        }

        // Check transaction control filter
        if self.transaction_control_only && !record.is_transaction_control() {
            return false;
        }

        true
    }
}

impl V2WALReader {
    /// Open a WAL file for reading
    pub fn open(wal_path: &Path) -> NativeResult<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(wal_path)
            .map_err(NativeBackendError::Io)?;

        let mut reader = Self {
            file: Arc::new(BufReader::new(file)),
            header: V2WALHeader::new(), // Will be read in read_header()
            current_position: std::mem::size_of::<V2WALHeader>() as u64,
            wal_end: 0,
            tx_index: TxRangeIndex::new(),
            active_tx: None,
        };

        // Read and validate header
        reader.read_header()?;

        // Determine WAL end position
        reader.determine_wal_end()?;

        // Build transaction index by scanning WAL
        reader.build_tx_index()?;

        Ok(reader)
    }

    /// Read WAL header from file
    fn read_header(&mut self) -> NativeResult<()> {
        let header_size = std::mem::size_of::<V2WALHeader>();
        let mut header_bytes = vec![0u8; header_size];

        {
            let file = Arc::get_mut(&mut self.file).ok_or(NativeBackendError::InvalidHeader {
                field: "file_access".to_string(),
                reason: "Cannot get mutable reference to WAL file".to_string(),
            })?;

            file.seek(SeekFrom::Start(0))
                .map_err(NativeBackendError::Io)?;

            file.read_exact(&mut header_bytes)
                .map_err(NativeBackendError::Io)?;
        }

        // Parse header
        self.header =
            unsafe { std::ptr::read_unaligned(header_bytes.as_ptr() as *const V2WALHeader) };

        // Validate header
        self.header.validate()?;

        Ok(())
    }

    /// Determine the end position of WAL data
    fn determine_wal_end(&mut self) -> NativeResult<()> {
        let file = Arc::get_mut(&mut self.file).ok_or(NativeBackendError::InvalidHeader {
            field: "file_access".to_string(),
            reason: "Cannot get mutable reference to WAL file".to_string(),
        })?;

        // Seek to end of file
        let file_size = file
            .seek(SeekFrom::End(0))
            .map_err(NativeBackendError::Io)?;

        self.wal_end = file_size;
        self.current_position = std::mem::size_of::<V2WALHeader>() as u64;

        Ok(())
    }

    /// Build transaction index by scanning WAL records
    ///
    /// This scans the entire WAL and builds an index mapping transactions
    /// to their LSN ranges (begin_lsn to commit_lsn). This enables efficient
    /// snapshot isolation filtering.
    ///
    /// Note: This does NOT validate WAL contiguity - it only builds the index.
    /// Contiguity validation happens during normal reads.
    fn build_tx_index(&mut self) -> NativeResult<()> {
        // Save current position
        let original_position = self.current_position;

        // Reset to start of records
        self.current_position = std::mem::size_of::<V2WALHeader>() as u64;

        // Reset active_tx state for index building
        self.active_tx = None;

        // Scan all records and build index (without contiguity validation)
        while let Some((lsn, record)) = self.read_next_record_with_validation(false)? {
            match record {
                V2WALRecord::TransactionBegin { tx_id, .. } => {
                    self.tx_index.begin_tx(tx_id, lsn);
                }
                V2WALRecord::TransactionCommit { tx_id, .. } => {
                    self.tx_index.commit_tx(tx_id, lsn);
                }
                V2WALRecord::TransactionRollback { tx_id, .. } => {
                    self.tx_index.rollback_tx(tx_id);
                }
                _ => {
                    // Data records - attribution is implicit by position
                    // No explicit action needed in tx_index
                }
            }
        }

        // Reset active_tx state after index building
        self.active_tx = None;

        // Restore original position
        self.current_position = original_position;

        Ok(())
    }

    /// Get reference to transaction index
    pub fn tx_index(&self) -> &TxRangeIndex {
        &self.tx_index
    }

    /// Get mutable reference to transaction index
    pub fn tx_index_mut(&mut self) -> &mut TxRangeIndex {
        &mut self.tx_index
    }

    /// Validate WAL record contiguity invariant
    ///
    /// This enforces the WAL contiguity invariant:
    /// - Data records must have an active transaction
    /// - Begin records must not have an active transaction
    /// - Commit tx_id must match the active transaction
    ///
    /// Updates active_tx state on Begin/Commit.
    fn validate_record_contiguity(&mut self, record: &V2WALRecord) -> NativeResult<()> {
        match record {
            V2WALRecord::TransactionBegin { tx_id, .. } => {
                // Invariant: No active transaction when Begin appears
                if let Some(active) = self.active_tx {
                    return Err(NativeBackendError::WalContiguityViolation(format!(
                        "Begin tx_id={} while tx_id={} already active",
                        tx_id, active
                    )));
                }
                self.active_tx = Some(*tx_id);
                Ok(())
            }

            V2WALRecord::TransactionCommit { tx_id, .. } => {
                // Invariant: Commit tx_id must match active transaction
                if let Some(active) = self.active_tx {
                    if active != *tx_id {
                        return Err(NativeBackendError::WalContiguityViolation(format!(
                            "Commit tx_id={} but active tx_id={}",
                            tx_id, active
                        )));
                    }
                    self.active_tx = None; // Transaction complete
                    Ok(())
                } else {
                    return Err(NativeBackendError::WalContiguityViolation(format!(
                        "Commit tx_id={} with no active transaction",
                        tx_id
                    )));
                }
            }

            V2WALRecord::TransactionRollback { tx_id, .. } => {
                // Rollback also ends the transaction (similar to commit)
                if let Some(active) = self.active_tx {
                    if active != *tx_id {
                        return Err(NativeBackendError::WalContiguityViolation(format!(
                            "Rollback tx_id={} but active tx_id={}",
                            tx_id, active
                        )));
                    }
                    self.active_tx = None; // Transaction complete
                    Ok(())
                } else {
                    return Err(NativeBackendError::WalContiguityViolation(format!(
                        "Rollback tx_id={} with no active transaction",
                        tx_id
                    )));
                }
            }

            // Data records (NodeInsert, NodeUpdate, NodeDelete, EdgeInsert, etc.)
            _ => {
                // Invariant: Data records require active transaction
                if self.active_tx.is_none() {
                    return Err(NativeBackendError::WalContiguityViolation(
                        "Data record without active transaction".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }

    /// Read the next WAL record from current position
    pub fn read_next_record(&mut self) -> NativeResult<Option<(u64, V2WALRecord)>> {
        self.read_next_record_with_validation(true)
    }

    /// Read the next WAL record from current position with optional validation
    fn read_next_record_with_validation(
        &mut self,
        validate_contiguity: bool,
    ) -> NativeResult<Option<(u64, V2WALRecord)>> {
        if self.current_position >= self.wal_end {
            return Ok(None); // End of WAL
        }

        let file = Arc::get_mut(&mut self.file).ok_or(NativeBackendError::InvalidHeader {
            field: "file_access".to_string(),
            reason: "Cannot get mutable reference to WAL file".to_string(),
        })?;

        file.seek(SeekFrom::Start(self.current_position))
            .map_err(NativeBackendError::Io)?;

        // Read record type and size
        let mut header_bytes = [0u8; 5]; // record_type (1) + size (4)
        file.read_exact(&mut header_bytes)
            .map_err(NativeBackendError::Io)?;

        let _record_type = V2WALRecordType::try_from(header_bytes[0])?;
        let record_size = u32::from_le_bytes([
            header_bytes[1],
            header_bytes[2],
            header_bytes[3],
            header_bytes[4],
        ]) as usize;

        if self.current_position + 5 + record_size as u64 > self.wal_end {
            return Err(NativeBackendError::RecordTooLarge {
                size: record_size as u32,
                max_size: (self.wal_end - self.current_position - 5) as u32,
            });
        }

        // Read record data
        let mut record_data = vec![0u8; record_size];
        file.read_exact(&mut record_data)
            .map_err(NativeBackendError::Io)?;

        // Combine header and data for deserialization
        let mut full_record = Vec::with_capacity(5 + record_size);
        full_record.extend_from_slice(&header_bytes);
        full_record.extend_from_slice(&record_data);

        // Deserialize record
        let record = V2WALSerializer::deserialize(&full_record)?;

        // Validate WAL contiguity invariant (if enabled)
        if validate_contiguity {
            self.validate_record_contiguity(&record)?;
        } else {
            // Update active_tx state without validation (for index building)
            self.update_active_tx_for_record(&record);
        }

        // Calculate LSN (simplified - in real implementation this would track LSNs)
        let lsn = self.position_to_lsn(self.current_position)?;

        // Update position
        self.current_position += 5 + record_size as u64;

        Ok(Some((lsn, record)))
    }

    /// Update active_tx state for a record (without validation)
    fn update_active_tx_for_record(&mut self, record: &V2WALRecord) {
        match record {
            V2WALRecord::TransactionBegin { tx_id, .. } => {
                self.active_tx = Some(*tx_id);
            }
            V2WALRecord::TransactionCommit { tx_id, .. }
            | V2WALRecord::TransactionRollback { tx_id, .. } => {
                // Only clear if it matches active transaction
                if self.active_tx == Some(*tx_id) {
                    self.active_tx = None;
                }
            }
            _ => {}
        }
    }

    /// Read all records matching the given filter
    pub fn read_filtered_records(
        &mut self,
        filter: &WALReadFilter,
    ) -> NativeResult<Vec<(u64, V2WALRecord)>> {
        let mut records = Vec::new();
        let current_pos = std::mem::size_of::<V2WALHeader>() as u64;

        // Reset position to start of records
        self.current_position = current_pos;

        while let Some((lsn, record)) = self.read_next_record()? {
            if filter.matches(&record, lsn) {
                records.push((lsn, record));
            }
        }

        Ok(records)
    }

    /// Seek to a specific LSN position
    pub fn seek_to_lsn(&mut self, target_lsn: u64) -> NativeResult<()> {
        let target_position = self.lsn_to_position(target_lsn)?;

        if target_position >= self.wal_end {
            return Err(NativeBackendError::InvalidHeader {
                field: "target_lsn".to_string(),
                reason: "LSN beyond WAL end".to_string(),
            });
        }

        self.current_position = target_position;

        Ok(())
    }

    /// Read all records from a specific LSN
    pub fn read_from_lsn(&mut self, start_lsn: u64) -> NativeResult<Vec<(u64, V2WALRecord)>> {
        self.seek_to_lsn(start_lsn)?;

        let mut records = Vec::new();
        while let Some((lsn, record)) = self.read_next_record()? {
            records.push((lsn, record));
        }

        Ok(records)
    }

    /// Get WAL statistics
    pub fn get_statistics(&mut self) -> NativeResult<WALStatistics> {
        let mut stats = WALStatistics::default();

        // Save current position
        let original_position = self.current_position;

        // Reset to start of records
        self.current_position = std::mem::size_of::<V2WALHeader>() as u64;

        // Count records by type
        while let Some((lsn, record)) = self.read_next_record()? {
            stats.total_records += 1;

            match record.record_type() {
                V2WALRecordType::NodeInsert => stats.node_inserts += 1,
                V2WALRecordType::NodeUpdate => stats.node_updates += 1,
                V2WALRecordType::NodeDelete => stats.node_deletes += 1,
                V2WALRecordType::ClusterCreate => stats.cluster_creates += 1,
                V2WALRecordType::EdgeInsert => stats.edge_inserts += 1,
                V2WALRecordType::EdgeUpdate => stats.edge_updates += 1,
                V2WALRecordType::EdgeDelete => stats.edge_deletes += 1,
                V2WALRecordType::TransactionBegin => stats.transaction_begins += 1,
                V2WALRecordType::TransactionCommit => stats.transaction_commits += 1,
                V2WALRecordType::TransactionRollback => stats.transaction_rollbacks += 1,
                V2WALRecordType::Checkpoint => stats.checkpoints += 1,
                _ => {}
            }

            // Update LSN range
            if stats.min_lsn == 0 || lsn < stats.min_lsn {
                stats.min_lsn = lsn;
            }
            if lsn > stats.max_lsn {
                stats.max_lsn = lsn;
            }
        }

        // Restore original position
        self.current_position = original_position;

        Ok(stats)
    }

    /// Create an iterator over all WAL records
    pub fn iter(&mut self) -> WALRecordIterator<'_> {
        WALRecordIterator {
            reader: self,
            end_lsn: None,
        }
    }

    /// Create an iterator up to a specific LSN
    pub fn iter_until(&mut self, end_lsn: u64) -> WALRecordIterator<'_> {
        WALRecordIterator {
            reader: self,
            end_lsn: Some(end_lsn),
        }
    }

    /// Get current WAL header
    pub fn header(&self) -> &V2WALHeader {
        &self.header
    }

    /// Get current file position
    pub fn current_position(&self) -> u64 {
        self.current_position
    }

    /// Convert position to LSN (simplified implementation)
    fn position_to_lsn(&self, position: u64) -> NativeResult<u64> {
        if position < std::mem::size_of::<V2WALHeader>() as u64 {
            return Err(NativeBackendError::InvalidHeader {
                field: "position".to_string(),
                reason: "Position is before WAL records".to_string(),
            });
        }

        // Simplified LSN calculation - in practice this would track LSNs more precisely
        let offset_from_header = position - std::mem::size_of::<V2WALHeader>() as u64;
        Ok(self.header.current_lsn - (offset_from_header / 100)) // Rough estimate
    }

    /// Convert LSN to file position (simplified implementation)
    fn lsn_to_position(&self, lsn: u64) -> NativeResult<u64> {
        if lsn < 1 {
            return Err(NativeBackendError::InvalidHeader {
                field: "lsn".to_string(),
                reason: "LSN must be >= 1".to_string(),
            });
        }

        // Simplified position calculation - in practice this would use an LSN index
        let estimated_offset = (self.header.current_lsn - lsn) * 100;
        Ok(std::mem::size_of::<V2WALHeader>() as u64 + estimated_offset)
    }
}

/// WAL statistics for analysis and monitoring
#[derive(Debug, Default)]
pub struct WALStatistics {
    /// Total number of records
    pub total_records: u64,

    /// Record type counts
    pub node_inserts: u64,
    pub node_updates: u64,
    pub node_deletes: u64,
    pub cluster_creates: u64,
    pub edge_inserts: u64,
    pub edge_updates: u64,
    pub edge_deletes: u64,
    pub transaction_begins: u64,
    pub transaction_commits: u64,
    pub transaction_rollbacks: u64,
    pub checkpoints: u64,

    /// LSN range
    pub min_lsn: u64,
    pub max_lsn: u64,
}

impl<'a> Iterator for WALRecordIterator<'a> {
    type Item = NativeResult<(u64, V2WALRecord)>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(end_lsn) = self.end_lsn {
            // Check if we've reached the end LSN
            let current_pos = self.reader.current_position();
            if let Ok(current_lsn) = self.reader.position_to_lsn(current_pos) {
                if current_lsn > end_lsn {
                    return None;
                }
            }
        }

        match self.reader.read_next_record() {
            Ok(Some(record)) => Some(Ok(record)),
            Ok(None) => None, // End of WAL
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::native::v2::wal::writer::V2WALWriter;
    use tempfile::tempdir;

    #[test]
    fn test_wal_reader_create() {
        let temp_dir = tempdir().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        // Create a WAL file first
        let config = crate::backend::native::v2::wal::V2WALConfig {
            wal_path: wal_path.clone(),
            ..Default::default()
        };
        let writer = V2WALWriter::create(config).unwrap();
        writer.shutdown().unwrap();

        // Now try to read it
        let reader = V2WALReader::open(&wal_path);
        assert!(reader.is_ok());
    }

    #[test]
    fn test_wal_read_filter() {
        let filter = WALReadFilter::all();
        let record = V2WALRecord::NodeInsert {
            node_id: 42,
            slot_offset: 1024,
            node_data: vec![1, 2, 3],
        };

        assert!(filter.matches(&record, 1));

        // Test type filter
        let type_filter = WALReadFilter::by_types(vec![V2WALRecordType::NodeInsert]);
        assert!(type_filter.matches(&record, 1));

        let wrong_type_filter = WALReadFilter::by_types(vec![V2WALRecordType::NodeDelete]);
        assert!(!wrong_type_filter.matches(&record, 1));

        // Test LSN range filter
        let lsn_filter = WALReadFilter::by_lsn_range(5, 15);
        assert!(!lsn_filter.matches(&record, 1)); // LSN 1 is outside range
        assert!(lsn_filter.matches(&record, 10)); // LSN 10 is inside range
    }

    #[test]
    fn test_wal_statistics() {
        let mut stats = WALStatistics::default();
        assert_eq!(stats.total_records, 0);
        assert_eq!(stats.node_inserts, 0);
        assert_eq!(stats.min_lsn, 0);
        assert_eq!(stats.max_lsn, 0);
    }

    // WAL contiguity validation tests

    /// Helper function to write WAL records to a file
    fn write_wal_records(wal_path: &std::path::Path, records: Vec<V2WALRecord>) {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(wal_path)
            .unwrap();

        // Write WAL header
        let header = crate::backend::native::v2::wal::V2WALHeader::new();
        let header_bytes = unsafe {
            std::slice::from_raw_parts(
                &header as *const _ as *const u8,
                std::mem::size_of::<crate::backend::native::v2::wal::V2WALHeader>(),
            )
        };
        file.write_all(header_bytes).unwrap();
        file.flush().unwrap();

        // Write test records
        for record in records {
            let serialized =
                crate::backend::native::v2::wal::record::V2WALSerializer::serialize(&record)
                    .unwrap();
            file.write_all(&serialized).unwrap();
        }

        file.flush().unwrap();
    }

    #[test]
    fn test_contiguity_valid_wal() {
        let temp_dir = tempdir().unwrap();
        let wal_path = temp_dir.path().join("test_contiguity.wal");

        // Create valid WAL with proper transaction structure
        write_wal_records(
            &wal_path,
            vec![
                V2WALRecord::TransactionBegin {
                    tx_id: 1,
                    timestamp: 100,
                },
                V2WALRecord::NodeInsert {
                    node_id: 1,
                    slot_offset: 1024,
                    node_data: vec![1, 2, 3],
                },
                V2WALRecord::TransactionCommit {
                    tx_id: 1,
                    timestamp: 150,
                },
                V2WALRecord::TransactionBegin {
                    tx_id: 2,
                    timestamp: 160,
                },
                V2WALRecord::NodeInsert {
                    node_id: 2,
                    slot_offset: 2048,
                    node_data: vec![4, 5, 6],
                },
                V2WALRecord::TransactionCommit {
                    tx_id: 2,
                    timestamp: 200,
                },
            ],
        );

        // Open and read WAL - should succeed
        let mut reader = V2WALReader::open(&wal_path).unwrap();

        // Read all records
        let mut count = 0;
        while let Ok(Some(_)) = reader.read_next_record() {
            count += 1;
        }

        // Should read all 6 records successfully
        assert_eq!(count, 6);
    }

    #[test]
    fn test_contiguity_data_without_active_tx() {
        let temp_dir = tempdir().unwrap();
        let wal_path = temp_dir.path().join("test_contiguity.wal");

        // Create WAL with data record but no Begin
        write_wal_records(
            &wal_path,
            vec![V2WALRecord::NodeInsert {
                node_id: 1,
                slot_offset: 1024,
                node_data: vec![1, 2, 3],
            }],
        );

        // Open WAL
        let mut reader = V2WALReader::open(&wal_path).unwrap();

        // First read should fail with contiguity violation
        let result = reader.read_next_record();
        assert!(result.is_err());
        match result.unwrap_err() {
            NativeBackendError::WalContiguityViolation(msg) => {
                assert!(msg.contains("Data record without active transaction"));
            }
            _ => panic!("Expected WalContiguityViolation error"),
        }
    }

    #[test]
    fn test_contiguity_nested_begin() {
        let temp_dir = tempdir().unwrap();
        let wal_path = temp_dir.path().join("test_contiguity.wal");

        // Create WAL with Begin while another transaction active
        write_wal_records(
            &wal_path,
            vec![
                V2WALRecord::TransactionBegin {
                    tx_id: 1,
                    timestamp: 100,
                },
                V2WALRecord::TransactionBegin {
                    tx_id: 2,
                    timestamp: 110,
                }, // ERROR: tx_id 1 still active
            ],
        );

        // Open WAL
        let mut reader = V2WALReader::open(&wal_path).unwrap();

        // First Begin should succeed
        assert!(reader.read_next_record().is_ok());

        // Second Begin should fail with contiguity violation
        let result = reader.read_next_record();
        assert!(result.is_err());
        match result.unwrap_err() {
            NativeBackendError::WalContiguityViolation(msg) => {
                assert!(msg.contains("Begin tx_id=2 while tx_id=1 already active"));
            }
            _ => panic!("Expected WalContiguityViolation error"),
        }
    }

    #[test]
    fn test_contiguity_commit_wrong_tx() {
        let temp_dir = tempdir().unwrap();
        let wal_path = temp_dir.path().join("test_contiguity.wal");

        // Create WAL with Commit for wrong transaction
        write_wal_records(
            &wal_path,
            vec![
                V2WALRecord::TransactionBegin {
                    tx_id: 1,
                    timestamp: 100,
                },
                V2WALRecord::TransactionCommit {
                    tx_id: 2,
                    timestamp: 110,
                }, // ERROR: committing tx_id=2 but tx_id=1 active
            ],
        );

        // Open WAL
        let mut reader = V2WALReader::open(&wal_path).unwrap();

        // Begin should succeed
        assert!(reader.read_next_record().is_ok());

        // Commit with wrong tx_id should fail
        let result = reader.read_next_record();
        assert!(result.is_err());
        match result.unwrap_err() {
            NativeBackendError::WalContiguityViolation(msg) => {
                assert!(msg.contains("Commit tx_id=2 but active tx_id=1"));
            }
            _ => panic!("Expected WalContiguityViolation error"),
        }
    }

    #[test]
    fn test_contiguity_commit_without_begin() {
        let temp_dir = tempdir().unwrap();
        let wal_path = temp_dir.path().join("test_contiguity.wal");

        // Create WAL with Commit but no Begin
        write_wal_records(
            &wal_path,
            vec![V2WALRecord::TransactionCommit {
                tx_id: 1,
                timestamp: 100,
            }],
        );

        // Open WAL
        let mut reader = V2WALReader::open(&wal_path).unwrap();

        // Commit without Begin should fail
        let result = reader.read_next_record();
        assert!(result.is_err());
        match result.unwrap_err() {
            NativeBackendError::WalContiguityViolation(msg) => {
                assert!(msg.contains("Commit tx_id=1 with no active transaction"));
            }
            _ => panic!("Expected WalContiguityViolation error"),
        }
    }

    #[test]
    fn test_contiguity_rollback_without_begin() {
        let temp_dir = tempdir().unwrap();
        let wal_path = temp_dir.path().join("test_contiguity.wal");

        // Create WAL with Rollback but no Begin
        write_wal_records(
            &wal_path,
            vec![V2WALRecord::TransactionRollback {
                tx_id: 1,
                timestamp: 100,
            }],
        );

        // Open WAL
        let mut reader = V2WALReader::open(&wal_path).unwrap();

        // Rollback without Begin should fail
        let result = reader.read_next_record();
        assert!(result.is_err());
        match result.unwrap_err() {
            NativeBackendError::WalContiguityViolation(msg) => {
                assert!(msg.contains("Rollback tx_id=1 with no active transaction"));
            }
            _ => panic!("Expected WalContiguityViolation error"),
        }
    }

    #[test]
    fn test_contiguity_rollback_clears_active_tx() {
        let temp_dir = tempdir().unwrap();
        let wal_path = temp_dir.path().join("test_contiguity.wal");

        // Create WAL with Begin, data, Rollback, then new transaction
        write_wal_records(
            &wal_path,
            vec![
                V2WALRecord::TransactionBegin {
                    tx_id: 1,
                    timestamp: 100,
                },
                V2WALRecord::NodeInsert {
                    node_id: 1,
                    slot_offset: 1024,
                    node_data: vec![1, 2, 3],
                },
                V2WALRecord::TransactionRollback {
                    tx_id: 1,
                    timestamp: 150,
                },
                V2WALRecord::TransactionBegin {
                    tx_id: 2,
                    timestamp: 160,
                }, // Should succeed after rollback
                V2WALRecord::NodeInsert {
                    node_id: 2,
                    slot_offset: 2048,
                    node_data: vec![4, 5, 6],
                },
                V2WALRecord::TransactionCommit {
                    tx_id: 2,
                    timestamp: 200,
                },
            ],
        );

        // Open WAL
        let mut reader = V2WALReader::open(&wal_path).unwrap();

        // All records should succeed
        let mut count = 0;
        while let Ok(Some(_)) = reader.read_next_record() {
            count += 1;
        }

        assert_eq!(count, 6);
    }
}
