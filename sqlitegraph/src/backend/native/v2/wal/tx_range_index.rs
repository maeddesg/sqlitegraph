//! Transaction range index for WAL-based snapshot isolation.
//!
//! This module provides TxRangeIndex which tracks transaction commit LSNs
//! for snapshot isolation. The index maps transaction IDs to their LSN ranges,
//! enabling efficient filtering of WAL records by snapshot visibility.
//!
//! # Architecture
//!
//! - **TxRange**: Tracks begin_lsn and commit_lsn for a transaction
//! - **TxRangeIndex**: In-memory index of all transaction ranges
//! - **Visibility**: tx.commit_lsn <= snapshot_id determines visibility
//!
//! # WAL Contiguity Invariant
//!
//! Records between TransactionBegin and TransactionCommit belong to that transaction
//! by WAL contiguity. We don't need explicit tx_id on each record - position determines
//! ownership.

use crate::snapshot::SnapshotId;
use super::record::V2WALRecord;
use std::collections::HashMap;

/// Transaction range tracking LSN boundaries
#[derive(Debug, Clone)]
pub struct TxRange {
    /// Transaction ID
    pub tx_id: u64,

    /// LSN when transaction began (TransactionBegin record)
    pub begin_lsn: u64,

    /// LSN when transaction committed (TransactionCommit record)
    /// None if transaction is still active or was rolled back
    pub commit_lsn: Option<u64>,
}

impl TxRange {
    /// Create a new active transaction range
    pub fn new(tx_id: u64, begin_lsn: u64) -> Self {
        Self {
            tx_id,
            begin_lsn,
            commit_lsn: None,
        }
    }

    /// Check if this transaction is committed
    pub fn is_committed(&self) -> bool {
        self.commit_lsn.is_some()
    }

    /// Check if this LSN falls within this transaction's range
    pub fn contains_lsn(&self, lsn: u64) -> bool {
        if lsn < self.begin_lsn {
            return false;
        }
        if let Some(commit_lsn) = self.commit_lsn {
            lsn <= commit_lsn
        } else {
            // Active transaction - LSN is after begin, no commit yet
            true
        }
    }
}

/// Transaction range index for snapshot isolation
///
/// Tracks all transactions and their LSN ranges to enable efficient
/// WAL record filtering by snapshot visibility.
#[derive(Debug, Default)]
pub struct TxRangeIndex {
    /// Map from transaction ID to its LSN range
    tx_ranges: HashMap<u64, TxRange>,

    /// Current maximum committed LSN (for SnapshotId::current())
    max_committed_lsn: u64,
}

impl TxRangeIndex {
    /// Create a new empty transaction index
    pub fn new() -> Self {
        Self {
            tx_ranges: HashMap::new(),
            max_committed_lsn: 0,
        }
    }

    /// Begin tracking a new transaction
    pub fn begin_tx(&mut self, tx_id: u64, begin_lsn: u64) {
        let range = TxRange::new(tx_id, begin_lsn);
        self.tx_ranges.insert(tx_id, range);
    }

    /// Mark a transaction as committed
    pub fn commit_tx(&mut self, tx_id: u64, commit_lsn: u64) {
        if let Some(range) = self.tx_ranges.get_mut(&tx_id) {
            range.commit_lsn = Some(commit_lsn);
            // Update max committed LSN
            if commit_lsn > self.max_committed_lsn {
                self.max_committed_lsn = commit_lsn;
            }
        }
    }

    /// Remove a rolled-back transaction
    pub fn rollback_tx(&mut self, tx_id: u64) {
        self.tx_ranges.remove(&tx_id);
    }

    /// Get transaction range by transaction ID
    pub fn get_tx_range(&self, tx_id: u64) -> Option<&TxRange> {
        self.tx_ranges.get(&tx_id)
    }

    /// Find transaction range containing a given LSN
    ///
    /// This uses linear scan for simplicity. For production with many
    /// concurrent transactions, this could be optimized with binary search
    /// or an interval tree.
    pub fn get_tx_range_for_lsn(&self, lsn: u64) -> Option<&TxRange> {
        self.tx_ranges.values().find(|range| range.contains_lsn(lsn))
    }

    /// Get current maximum committed LSN
    ///
    /// This can be used to implement SnapshotId::current()
    pub fn max_committed_lsn(&self) -> u64 {
        self.max_committed_lsn
    }

    /// Check if a record at given LSN is visible at snapshot
    ///
    /// A record is visible iff its transaction's commit_lsn <= snapshot_lsn
    pub fn is_visible_at(&self, lsn: u64, snapshot_lsn: u64) -> bool {
        if let Some(range) = self.get_tx_range_for_lsn(lsn) {
            if let Some(commit_lsn) = range.commit_lsn {
                commit_lsn <= snapshot_lsn
            } else {
                // Active transaction - not visible to any snapshot
                false
            }
        } else {
            // No transaction found - this LSN is from checkpoint (always visible)
            true
        }
    }

    /// Clear all transaction ranges
    pub fn clear(&mut self) {
        self.tx_ranges.clear();
        self.max_committed_lsn = 0;
    }

    /// Get number of tracked transactions
    pub fn len(&self) -> usize {
        self.tx_ranges.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.tx_ranges.is_empty()
    }

    /// Check if a transaction is visible from the given snapshot.
    ///
    /// A transaction is visible iff:
    /// 1. It exists in the index
    /// 2. It has committed (commit_lsn != 0)
    /// 3. Its commit_lsn <= snapshot_id
    ///
    /// # Arguments
    /// * `tx_id` - Transaction ID to check
    /// * `snapshot_id` - Snapshot ID (LSN) to check visibility against
    ///
    /// # Returns
    /// * true if transaction is visible from snapshot
    /// * false if transaction not visible (not committed, not in index, or committed after snapshot)
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlitegraph::snapshot::SnapshotId;
    /// use sqlitegraph::backend::native::v2::wal::TxRangeIndex;
    ///
    /// let mut index = TxRangeIndex::new();
    /// index.begin_tx(1, 100);
    /// index.commit_tx(1, 150);
    ///
    /// let snapshot = SnapshotId::from_lsn(200);
    /// assert!(index.is_tx_visible(1, snapshot));
    /// ```
    pub fn is_tx_visible(&self, tx_id: u64, snapshot_id: SnapshotId) -> bool {
        if let Some(range) = self.tx_ranges.get(&tx_id) {
            // Transaction must be committed AND commit_lsn <= snapshot_id
            if let Some(commit_lsn) = range.commit_lsn {
                commit_lsn != 0 && commit_lsn <= snapshot_id.as_lsn()
            } else {
                // Uncommitted transaction (commit_lsn is None)
                false
            }
        } else {
            // Transaction not found in index - not visible
            false
        }
    }
}

/// Iterate over WAL records, yielding only those visible from the snapshot.
///
/// This function respects the WAL contiguity invariant: records are processed
/// in transaction order, and entire transactions are either visible or not.
///
/// # Arguments
/// * `tx_index` - Transaction index to check visibility against
/// * `wal_records` - Iterator over (tx_id, WalRecord) pairs
/// * `snapshot_id` - Snapshot ID for visibility filtering
///
/// # Yields
/// WalRecords from transactions where is_tx_visible(tx_id, snapshot_id) == true
///
/// # Examples
///
/// ```
/// use sqlitegraph::snapshot::SnapshotId;
/// use sqlitegraph::backend::native::v2::wal::{TxRangeIndex, iter_visible_wal_records};
/// use sqlitegraph::backend::native::v2::wal::record::V2WALRecord;
///
/// let mut index = TxRangeIndex::new();
/// index.begin_tx(1, 100);
/// index.commit_tx(1, 150);
///
/// let records = vec![
///     (1, V2WALRecord::TransactionBegin { tx_id: 1, timestamp: 0 }),
///     (1, V2WALRecord::TransactionCommit { tx_id: 1, timestamp: 100 }),
/// ];
///
/// let snapshot = SnapshotId::from_lsn(200);
/// let visible: Vec<_> = iter_visible_wal_records(&index, records.into_iter(), snapshot).collect();
/// assert_eq!(visible.len(), 2);
/// ```
pub fn iter_visible_wal_records<'a, I>(
    tx_index: &'a TxRangeIndex,
    wal_records: I,
    snapshot_id: SnapshotId,
) -> impl Iterator<Item = (u64, V2WALRecord)> + 'a
where
    I: Iterator<Item = (u64, V2WALRecord)> + 'a,
{
    wal_records.filter(move |(tx_id, _record)| {
        tx_index.is_tx_visible(*tx_id, snapshot_id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx_range_creation() {
        let range = TxRange::new(100, 1000);
        assert_eq!(range.tx_id, 100);
        assert_eq!(range.begin_lsn, 1000);
        assert_eq!(range.commit_lsn, None);
        assert!(!range.is_committed());
    }

    #[test]
    fn test_tx_range_commit() {
        let mut range = TxRange::new(100, 1000);
        range.commit_lsn = Some(2000);
        assert!(range.is_committed());
    }

    #[test]
    fn test_tx_range_contains_lsn() {
        let mut range = TxRange::new(100, 1000);

        // Before begin - not contained
        assert!(!range.contains_lsn(999));

        // At begin - contained
        assert!(range.contains_lsn(1000));

        // After begin, before commit (active) - contained
        assert!(range.contains_lsn(1500));

        // After commit - contained if commit_lsn set
        range.commit_lsn = Some(2000);
        assert!(range.contains_lsn(2000));
        assert!(!range.contains_lsn(2001));
    }

    #[test]
    fn test_tx_index_begin_commit() {
        let mut index = TxRangeIndex::new();

        index.begin_tx(1, 100);
        index.begin_tx(2, 200);

        assert_eq!(index.len(), 2);

        // Commit tx 1
        index.commit_tx(1, 150);
        assert_eq!(index.max_committed_lsn(), 150);

        // Commit tx 2
        index.commit_tx(2, 250);
        assert_eq!(index.max_committed_lsn(), 250);

        // Check committed status
        let range1 = index.get_tx_range(1).unwrap();
        assert!(range1.is_committed());
        assert_eq!(range1.commit_lsn, Some(150));

        let range2 = index.get_tx_range(2).unwrap();
        assert!(range2.is_committed());
        assert_eq!(range2.commit_lsn, Some(250));
    }

    #[test]
    fn test_tx_index_rollback() {
        let mut index = TxRangeIndex::new();

        index.begin_tx(1, 100);
        index.begin_tx(2, 200);

        assert_eq!(index.len(), 2);

        // Rollback tx 1
        index.rollback_tx(1);
        assert_eq!(index.len(), 1);
        assert!(index.get_tx_range(1).is_none());
        assert!(index.get_tx_range(2).is_some());
    }

    #[test]
    fn test_tx_index_get_tx_range_for_lsn() {
        let mut index = TxRangeIndex::new();

        index.begin_tx(1, 100);
        index.commit_tx(1, 200);

        index.begin_tx(2, 300);
        index.commit_tx(2, 400);

        // LSN 150 should be in tx 1
        let range = index.get_tx_range_for_lsn(150).unwrap();
        assert_eq!(range.tx_id, 1);

        // LSN 350 should be in tx 2
        let range = index.get_tx_range_for_lsn(350).unwrap();
        assert_eq!(range.tx_id, 2);

        // LSN 250 should be in neither (between transactions)
        assert!(index.get_tx_range_for_lsn(250).is_none());
    }

    #[test]
    fn test_tx_index_is_visible_at() {
        let mut index = TxRangeIndex::new();

        // Transaction 1: LSN 100-200, commits at LSN 200
        index.begin_tx(1, 100);
        index.commit_tx(1, 200);

        // Transaction 2: LSN 300-400, commits at LSN 400
        index.begin_tx(2, 300);
        index.commit_tx(2, 400);

        // Snapshot at 150 should NOT see tx 1's data (tx1 commits at 200, after snapshot 150)
        assert!(!index.is_visible_at(150, 150));

        // Snapshot at 200 SHOULD see tx 1's data (tx1 commits at 200, snapshot at 200)
        assert!(index.is_visible_at(150, 200));

        // Snapshot at 150 should NOT see tx 2's data (tx2 commits at 400, after snapshot 150)
        assert!(!index.is_visible_at(350, 150));

        // Snapshot at 400 should see both transactions
        assert!(index.is_visible_at(150, 400));
        assert!(index.is_visible_at(350, 400));

        // LSN outside any transaction (checkpoint data) always visible
        assert!(index.is_visible_at(50, 100));
        assert!(index.is_visible_at(500, 400));
    }

    #[test]
    fn test_tx_index_clear() {
        let mut index = TxRangeIndex::new();

        index.begin_tx(1, 100);
        index.commit_tx(1, 200);

        assert_eq!(index.len(), 1);
        assert_eq!(index.max_committed_lsn(), 200);

        index.clear();

        assert_eq!(index.len(), 0);
        assert!(index.is_empty());
        assert_eq!(index.max_committed_lsn(), 0);
    }

    #[test]
    fn test_is_tx_visible_committed_before_snapshot() {
        let mut index = TxRangeIndex::new();
        index.begin_tx(1, 100);
        index.commit_tx(1, 150);

        let snapshot = SnapshotId::from_lsn(200);
        assert!(index.is_tx_visible(1, snapshot));
    }

    #[test]
    fn test_is_tx_visible_committed_after_snapshot() {
        let mut index = TxRangeIndex::new();
        index.begin_tx(1, 100);
        index.commit_tx(1, 250);

        let snapshot = SnapshotId::from_lsn(200);
        assert!(!index.is_tx_visible(1, snapshot));
    }

    #[test]
    fn test_is_tx_visible_uncommitted() {
        let mut index = TxRangeIndex::new();
        index.begin_tx(1, 100);

        let snapshot = SnapshotId::from_lsn(200);
        assert!(!index.is_tx_visible(1, snapshot));
    }

    #[test]
    fn test_is_tx_visible_nonexistent() {
        let index = TxRangeIndex::new();
        let snapshot = SnapshotId::current();
        assert!(!index.is_tx_visible(999, snapshot));
    }

    #[test]
    fn test_is_tx_visible_at_exact_commit_lsn() {
        let mut index = TxRangeIndex::new();
        index.begin_tx(1, 100);
        index.commit_tx(1, 150);

        let snapshot = SnapshotId::from_lsn(150);
        assert!(index.is_tx_visible(1, snapshot));
    }

    #[test]
    fn test_iter_visible_wal_records_filters_correctly() {
        use V2WALRecord::*;

        let mut index = TxRangeIndex::new();
        index.begin_tx(1, 100);
        index.commit_tx(1, 150); // visible from snapshot 200

        index.begin_tx(2, 160);
        // tx2 not committed - not visible

        index.begin_tx(3, 170);
        index.commit_tx(3, 250); // committed after snapshot 200 - not visible

        let wal_records = vec![
            (1, TransactionBegin { tx_id: 1, timestamp: 100 }),
            (1, TransactionCommit { tx_id: 1, timestamp: 150 }),
            (2, TransactionBegin { tx_id: 2, timestamp: 160 }),
            (3, TransactionBegin { tx_id: 3, timestamp: 170 }),
            (3, TransactionCommit { tx_id: 3, timestamp: 250 }),
        ];

        let snapshot = SnapshotId::from_lsn(200);
        let visible: Vec<_> = iter_visible_wal_records(
            &index,
            wal_records.into_iter(),
            snapshot
        ).collect();

        // Only tx1 records should be visible
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].0, 1);
        assert_eq!(visible[1].0, 1);

        // Verify the record types
        match &visible[0].1 {
            TransactionBegin { tx_id, .. } => assert_eq!(*tx_id, 1),
            _ => panic!("Expected TransactionBegin"),
        }
        match &visible[1].1 {
            TransactionCommit { tx_id, .. } => assert_eq!(*tx_id, 1),
            _ => panic!("Expected TransactionCommit"),
        }
    }

    #[test]
    fn test_iter_visible_wal_records_empty_iterator() {
        let index = TxRangeIndex::new();
        let wal_records = vec![];
        let snapshot = SnapshotId::current();

        let visible: Vec<_> = iter_visible_wal_records(
            &index,
            wal_records.into_iter(),
            snapshot
        ).collect();

        assert_eq!(visible.len(), 0);
    }

    #[test]
    fn test_iter_visible_wal_records_all_invisible() {
        use V2WALRecord::*;

        let mut index = TxRangeIndex::new();
        index.begin_tx(1, 100);
        // tx1 not committed - all records invisible

        let wal_records = vec![
            (1, TransactionBegin { tx_id: 1, timestamp: 100 }),
            (1, NodeInsert {
                node_id: 1,
                slot_offset: 0,
                node_data: vec![],
            }),
        ];

        let snapshot = SnapshotId::from_lsn(200);
        let visible: Vec<_> = iter_visible_wal_records(
            &index,
            wal_records.into_iter(),
            snapshot
        ).collect();

        assert_eq!(visible.len(), 0);
    }
}
