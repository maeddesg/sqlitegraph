//! KV WAL replay operations for V2 recovery
//!
//! This module handles replay of KV store WAL records during crash recovery.

use crate::backend::native::v2::kv_store::wal;
use crate::backend::native::v2::wal::recovery::errors::RecoveryError;
use crate::backend::native::v2::wal::recovery::replayer::types::RollbackOperation;
use crate::debug::debug_log;

impl super::DefaultReplayOperations {
    /// Handle KvSet record during WAL replay
    ///
    /// This is called by V2WALRecovery when replaying a committed transaction
    /// that contains KV set operations. It delegates to the kv_store::wal::apply_set helper.
    pub fn handle_kv_set(
        &self,
        key: Vec<u8>,
        value_bytes: Vec<u8>,
        value_type: u8,
        ttl_seconds: Option<u64>,
        version: u64,
        rollback_data: &mut Vec<RollbackOperation>,
    ) -> Result<(), RecoveryError> {
        debug_log!(
            "Replaying KvSet: key_len={}, version={}, ttl={:?}",
            key.len(),
            version,
            ttl_seconds
        );

        // Get access to KvStore
        let mut kv_store = self.kv_store.lock().map_err(|e| {
            RecoveryError::replay_failure(format!("Failed to lock KV store: {}", e))
        })?;

        // Apply the KV set operation using the recovery helper
        wal::apply_set(
            &mut *kv_store,
            key.clone(),
            value_bytes.clone(),
            value_type,
            ttl_seconds,
            version,
        )
        .map_err(|e| {
            RecoveryError::replay_failure(format!("Failed to apply KV set during recovery: {}", e))
        })?;

        // Add rollback operation
        rollback_data.push(RollbackOperation::KvSet {
            key,
            value_bytes,
            value_type,
            ttl_seconds,
            version,
        });

        // Update statistics
        self.statistics.record_kv_operation();

        debug_log!("Successfully replayed KvSet for version {}", version);
        Ok(())
    }

    /// Handle KvDelete record during WAL replay
    pub fn handle_kv_delete(
        &self,
        key: Vec<u8>,
        old_value_bytes: Option<Vec<u8>>,
        old_value_type: u8,
        old_version: u64,
        rollback_data: &mut Vec<RollbackOperation>,
    ) -> Result<(), RecoveryError> {
        debug_log!(
            "Replaying KvDelete: key_len={}, old_version={}",
            key.len(),
            old_version
        );

        // Get access to KvStore
        let mut kv_store = self.kv_store.lock().map_err(|e| {
            RecoveryError::replay_failure(format!("Failed to lock KV store: {}", e))
        })?;

        // Apply the KV delete operation using the recovery helper
        wal::apply_delete(&mut *kv_store, key.clone(), old_version).map_err(|e| {
            RecoveryError::replay_failure(format!(
                "Failed to apply KV delete during recovery: {}",
                e
            ))
        })?;

        // Add rollback operation
        rollback_data.push(RollbackOperation::KvDelete {
            key,
            old_value_bytes,
            old_value_type,
            version: old_version,
        });

        // Update statistics
        self.statistics.record_kv_operation();

        debug_log!("Successfully replayed KvDelete");
        Ok(())
    }
}
