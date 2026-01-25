//! Committed-Delta Index for Snapshot Isolation

use crate::backend::native::types::NativeBackendError;
use crate::backend::native::v2::wal::V2WALRecord;
use crate::snapshot::SnapshotId;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Delta record for a single WAL operation
#[derive(Debug, Clone)]
pub struct DeltaRecord {
    pub record: V2WALRecord,
    pub commit_lsn: u64,
}

pub type SharedDeltaIndex = Arc<RwLock<DeltaIndex>>;

#[derive(Debug, Default)]
pub struct DeltaIndex {
    node_deltas: HashMap<i64, Vec<DeltaRecord>>,
}

impl DeltaIndex {
    pub fn new() -> Self { Self::default() }

    pub fn apply_commit(&mut self, records: Vec<V2WALRecord>, commit_lsn: u64) -> Result<(), NativeBackendError> {
        for record in records {
            let delta = DeltaRecord { record: record.clone(), commit_lsn };
            match record {
                V2WALRecord::NodeInsert { node_id, .. } | V2WALRecord::NodeDelete { node_id, .. } => {
                    self.node_deltas.entry(node_id).or_insert_with(Vec::new).push(delta);
                }
                V2WALRecord::NodeUpdate { node_id, .. } => {
                    self.node_deltas.entry(node_id).or_insert_with(Vec::new).push(delta);
                }
                V2WALRecord::TransactionBegin { .. }
                | V2WALRecord::TransactionCommit { .. }
                | V2WALRecord::TransactionRollback { .. }
                | V2WALRecord::Checkpoint { .. }
                | V2WALRecord::SegmentEnd { .. }
                | V2WALRecord::HeaderUpdate { .. } => { continue; }
                _ => { continue; }
            }
        }
        Ok(())
    }

    pub fn get_node_delta(&self, node_id: i64, snapshot_id: SnapshotId) -> Option<&DeltaRecord> {
        self.node_deltas.get(&node_id).and_then(|deltas| {
            let snapshot_lsn = snapshot_id.as_lsn();
            deltas.iter().rev().find(|delta| delta.commit_lsn <= snapshot_lsn)
        })
    }

    pub fn has_node_delta(&self, node_id: i64, snapshot_id: SnapshotId) -> bool {
        self.node_deltas.get(&node_id).map_or(false, |deltas| {
            let snapshot_lsn = snapshot_id.as_lsn();
            deltas.iter().any(|delta| delta.commit_lsn <= snapshot_lsn)
        })
    }

    pub fn checkpoint_completed(&mut self, checkpoint_lsn: u64) -> usize {
        let mut nodes_dropped = 0;
        self.node_deltas.retain(|_, deltas| {
            let before_len = deltas.len();
            deltas.retain(|delta| delta.commit_lsn > checkpoint_lsn);
            let after_len = deltas.len();
            nodes_dropped += before_len - after_len;
            after_len > 0
        });
        nodes_dropped
    }

    pub fn delta_count(&self) -> usize {
        self.node_deltas.values().map(|v| v.len()).sum()
    }

    #[cfg(test)]
    pub fn clear(&mut self) {
        self.node_deltas.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_index_new() {
        let index = DeltaIndex::new();
        assert_eq!(index.delta_count(), 0);
    }

    #[test]
    fn test_apply_commit() {
        let mut index = DeltaIndex::new();
        let records = vec![
            V2WALRecord::NodeInsert {
                node_id: 1,
                slot_offset: 100,
                node_data: vec![1, 2, 3],
            },
        ];
        let result = index.apply_commit(records, 100);
        assert!(result.is_ok());
        assert_eq!(index.delta_count(), 1);
    }

    #[test]
    fn test_get_node_delta_visible() {
        let mut index = DeltaIndex::new();
        let records = vec![
            V2WALRecord::NodeInsert {
                node_id: 1,
                slot_offset: 100,
                node_data: vec![1, 2, 3],
            },
        ];
        index.apply_commit(records, 100).unwrap();

        let snapshot = SnapshotId::from_lsn(150);
        let delta = index.get_node_delta(1, snapshot);
        assert!(delta.is_some());
        assert_eq!(delta.unwrap().commit_lsn, 100);
    }

    #[test]
    fn test_get_node_delta_not_visible() {
        let mut index = DeltaIndex::new();
        let records = vec![
            V2WALRecord::NodeInsert {
                node_id: 1,
                slot_offset: 100,
                node_data: vec![1, 2, 3],
            },
        ];
        index.apply_commit(records, 200).unwrap();

        let snapshot = SnapshotId::from_lsn(150);
        let delta = index.get_node_delta(1, snapshot);
        assert!(delta.is_none());
    }

    #[test]
    fn test_checkpoint_completed() {
        let mut index = DeltaIndex::new();
        let records = vec![
            V2WALRecord::NodeInsert {
                node_id: 1,
                slot_offset: 100,
                node_data: vec![1, 2, 3],
            },
        ];
        index.apply_commit(records, 100).unwrap();
        assert_eq!(index.delta_count(), 1);

        let dropped = index.checkpoint_completed(150);
        assert_eq!(dropped, 1);
        assert_eq!(index.delta_count(), 0);
    }

    #[test]
    fn test_multiple_deltas_same_entity() {
        let mut index = DeltaIndex::new();

        let records1 = vec![
            V2WALRecord::NodeInsert {
                node_id: 1,
                slot_offset: 100,
                node_data: vec![1, 2, 3],
            },
        ];
        index.apply_commit(records1, 100).unwrap();

        let records2 = vec![
            V2WALRecord::NodeDelete {
                node_id: 1,
                slot_offset: 100,
                old_data: vec![1, 2, 3],
                outgoing_edges: vec![],
                incoming_edges: vec![],
            },
        ];
        index.apply_commit(records2, 200).unwrap();

        let snapshot1 = SnapshotId::from_lsn(150);
        let delta1 = index.get_node_delta(1, snapshot1);
        assert!(delta1.is_some());
        assert!(matches!(delta1.unwrap().record, V2WALRecord::NodeInsert { .. }));

        let snapshot2 = SnapshotId::from_lsn(250);
        let delta2 = index.get_node_delta(1, snapshot2);
        assert!(delta2.is_some());
        assert!(matches!(delta2.unwrap().record, V2WALRecord::NodeDelete { .. }));
    }
}

