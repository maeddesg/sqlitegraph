//! Snapshot isolation for ACID compliance
//!
//! This module provides the SnapshotId type which enforces that all read operations
//! only observe data committed at or before a specific transaction snapshot.
//!
//! # Hard Rule
//!
//! **No API may observe state not bound to a committed snapshot_id.**
//!
//! If a value cannot be tied to a committed snapshot → it does not exist.
//!
//! # Example
//!
//! ```rust
//! use sqlitegraph::snapshot::SnapshotId;
//!
//! // Get current snapshot (only committed data visible)
//! let snapshot = SnapshotId::current();
//!
//! // Read from database using snapshot
//! let node = backend.get_node(snapshot, node_id)?;
//!
//! // Create snapshot from specific transaction
//! let snapshot = SnapshotId::from_tx(12345);
//! ```

/// Snapshot identifier - points to committed transaction state
///
/// Only data committed at or before this snapshot_id is visible.
/// If a value cannot be tied to a committed snapshot_id → it does not exist.
///
/// # Invariant
///
/// snapshot_id.0 MUST correspond to a committed transaction.
/// Uncommitted transactions do not create valid snapshots.
///
/// # Representation
///
/// Internally, SnapshotId wraps a TransactionId (u64). This is valid because:
/// - TransactionId is allocated at begin_transaction()
/// - SnapshotId is created at commit_transaction()
/// - The 1:1 mapping ensures snapshot_id uniquely identifies committed state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnapshotId(pub u64);

#[cfg(feature = "native-v2")]
use std::sync::Arc;
#[cfg(feature = "native-v2")]
use std::sync::OnceLock;
#[cfg(feature = "native-v2")]
use crate::backend::native::v2::wal::manager::V2WALManager;

/// Global storage for the current WAL manager reference.
///
/// This is set by V2GraphBackend when initialized and allows
/// SnapshotId::current() to access the actual max committed LSN.
///
/// We use OnceLock to provide safe shared access to the WAL manager.
#[cfg(feature = "native-v2")]
static CURRENT_WAL_MANAGER: OnceLock<Arc<V2WALManager>> = OnceLock::new();

/// Register the WAL manager for use by SnapshotId::current()
///
/// This should be called by the V2GraphBackend when it is initialized.
/// Returns an error if a WAL manager is already registered.
///
/// # Note
///
/// This function takes ownership of an Arc clone. The WAL manager will
/// remain available until explicitly replaced or the program exits.
#[cfg(feature = "native-v2")]
pub(crate) fn register_wal_manager(manager: Arc<V2WALManager>) -> Result<(), Arc<V2WALManager>> {
    CURRENT_WAL_MANAGER.set(manager)
}

/// Unregister the WAL manager (called on backend shutdown)
///
/// Note: OnceLock cannot be cleared, so this is a no-op.
/// The WAL manager reference remains valid for the lifetime of the program.
#[cfg(feature = "native-v2")]
pub(crate) fn unregister_wal_manager() {
    // OnceLock cannot be cleared, so we intentionally do nothing here.
    // The WAL manager reference remains valid for the lifetime of the program.
}

/// Get access to the current WAL manager (if registered)
#[cfg(feature = "native-v2")]
fn with_wal_manager<F, R>(f: F) -> R
where
    F: FnOnce(Option<&V2WALManager>) -> R,
{
    let manager = CURRENT_WAL_MANAGER.get();
    f(manager.map(|m| m.as_ref()))
}

impl SnapshotId {
    /// The "current" snapshot - sees only committed data
    ///
    /// This returns the most recent committed transaction ID.
    /// All reads using this snapshot are guaranteed to see only
    /// data that has been durably committed.
    ///
    /// # Implementation Note
    ///
    /// - For native-v2 backend: Returns the maximum committed LSN from the WAL manager
    /// - For SQLite backend: Returns 0 to indicate "all committed data"
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sqlitegraph::snapshot::SnapshotId;
    /// let snapshot = SnapshotId::current();
    /// // snapshot now points to the most recent committed transaction
    /// ```
    #[cfg(not(feature = "native-v2"))]
    pub fn current() -> Self {
        // For SQLite backend, 0 means "all committed data visible"
        SnapshotId(0)
    }

    /// The "current" snapshot - sees only committed data (native-v2)
    ///
    /// This returns the most recent committed LSN from the WAL manager.
    /// All reads using this snapshot are guaranteed to see only
    /// data that has been durably committed.
    ///
    /// Returns 0 if no WAL manager is registered or no transactions
    /// have been committed yet.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sqlitegraph::snapshot::SnapshotId;
    /// let snapshot = SnapshotId::current();
    /// // snapshot now points to the most recent committed transaction
    /// ```
    #[cfg(feature = "native-v2")]
    pub fn current() -> Self {
        let lsn = with_wal_manager(|manager| {
            manager
                .map(|m| m.max_committed_lsn())
                .unwrap_or(0)
        });
        SnapshotId(lsn)
    }

    /// Create from explicit transaction ID
    ///
    /// # Arguments
    ///
    /// * `tx_id` - A committed transaction ID
    ///
    /// # Important
    ///
    /// The caller MUST ensure that tx_id corresponds to a committed transaction.
    /// Using an uncommitted transaction ID violates snapshot isolation guarantees.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sqlitegraph::snapshot::SnapshotId;
    /// // After commit returns SnapshotId
    /// let snapshot = coordinator.commit_transaction(tx_id)?;
    /// // Later, reuse same snapshot for repeatable reads
    /// let node = backend.get_node(snapshot, node_id)?;
    /// ```
    pub fn from_tx(tx_id: u64) -> Self {
        SnapshotId(tx_id)
    }

    /// Create snapshot from explicit LSN (Log Sequence Number)
    ///
    /// # Arguments
    ///
    /// * `lsn` - A commit LSN representing a committed transaction
    ///
    /// # Important
    ///
    /// The caller MUST ensure that lsn corresponds to a committed transaction.
    /// Using an uncommitted LSN violates snapshot isolation guarantees.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sqlitegraph::snapshot::SnapshotId;
    /// // Create snapshot at specific LSN
    /// let snapshot = SnapshotId::from_lsn(12345);
    /// ```
    pub fn from_lsn(lsn: u64) -> Self {
        SnapshotId(lsn)
    }

    /// Invalid snapshot - used for error cases
    ///
    /// This sentinel value indicates that no valid snapshot exists.
    /// Read operations receiving this snapshot should return an error.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sqlitegraph::snapshot::SnapshotId;
    /// fn validate_snapshot(snapshot: SnapshotId) -> Result<()> {
    ///     if snapshot == SnapshotId::invalid() {
    ///         return Err("Invalid snapshot");
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub fn invalid() -> Self {
        SnapshotId(u64::MAX)
    }

    /// Check if this snapshot is valid
    ///
    /// Returns false for the invalid sentinel value.
    pub fn is_valid(&self) -> bool {
        self.0 != u64::MAX
    }

    /// Get the underlying transaction ID
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Get snapshot as LSN (Log Sequence Number)
    ///
    /// Since SnapshotId wraps a commit LSN, this returns the LSN directly.
    /// Used for WAL record visibility checks.
    pub fn as_lsn(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_id_creation() {
        let snapshot = SnapshotId(12345);
        assert_eq!(snapshot.as_u64(), 12345);
    }

    #[test]
    fn test_snapshot_id_from_tx() {
        let snapshot = SnapshotId::from_tx(999);
        assert_eq!(snapshot.as_u64(), 999);
    }

    #[test]
    fn test_snapshot_id_from_lsn() {
        let snapshot = SnapshotId::from_lsn(12345);
        assert_eq!(snapshot.as_lsn(), 12345);
        assert_eq!(snapshot.as_u64(), 12345);
    }

    #[test]
    fn test_snapshot_id_invalid() {
        let invalid = SnapshotId::invalid();
        assert_eq!(invalid.as_u64(), u64::MAX);
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_snapshot_id_valid() {
        let valid = SnapshotId::from_tx(100);
        assert!(valid.is_valid());
    }

    #[test]
    fn test_snapshot_id_current() {
        let current = SnapshotId::current();
        // Current snapshot should always be valid
        assert!(current.is_valid());
    }

    #[test]
    fn test_snapshot_id_equality() {
        let s1 = SnapshotId(100);
        let s2 = SnapshotId(100);
        let s3 = SnapshotId(200);

        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_snapshot_id_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(SnapshotId(100));
        set.insert(SnapshotId(200));
        set.insert(SnapshotId(100)); // Duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_snapshot_id_copy() {
        let s1 = SnapshotId(100);
        let s2 = s1; // Copy, not move

        assert_eq!(s1.as_u64(), 100);
        assert_eq!(s2.as_u64(), 100);
    }
}
