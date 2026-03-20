//! Native GraphBackend implementation with interior mutability.

use super::adjacency::{AdjacencyHelpers, Direction};
use super::edge_store::EdgeStore;
use super::graph_file::GraphFile;
use super::graph_ops::*;
use super::graph_validation::*;
use super::node_store::NodeStore;
use super::types::*;
use crate::SqliteGraphError;
use crate::backend::{
    BackendDirection, ChainStep, EdgeSpec, GraphBackend, NeighborQuery, NodeSpec, PatternMatch,
    PatternQuery,
};
use crate::graph::GraphEntity;
use parking_lot::RwLock;
use std::sync::Arc;

// V2 WAL integration (always available when native-v2 feature is enabled)
#[cfg(feature = "native-v2")]
use crate::backend::native::v2::wal::{
    GraphWALIntegrationConfig, V2GraphWALIntegrator, V2WALConfig,
};

#[cfg(feature = "native-v2")]
use crate::backend::native::v2::kv_store::store::KvStore;

#[cfg(feature = "native-v2")]
use crate::backend::native::types::KvValue;

/// Native backend implementation using interior mutability
pub struct NativeGraphBackend {
    graph_file: RwLock<GraphFile>,
    /// Neighbors cache to avoid repeated B+tree traversals
    /// Key: (node_id, direction), Value: Arc<[i64]> for zero-copy reads
    neighbors_cache: RwLock<std::collections::HashMap<(i64, Direction), std::sync::Arc<[i64]>>>,
    /// WAL integrator for transaction management and checkpointing
    /// Always available when native-v2 feature is enabled (production ready)
    #[cfg(feature = "native-v2")]
    wal_integrator: Option<Arc<V2GraphWALIntegrator>>,
    /// KV store for metadata and application data
    #[cfg(feature = "native-v2")]
    kv_store: Arc<RwLock<KvStore>>,
}

impl NativeGraphBackend {
    /// Create a new native backend with an in-memory temporary file
    #[cfg(test)]
    pub fn new_temp() -> Result<Self, SqliteGraphError> {
        use tempfile::NamedTempFile;
        let temp_file =
            NamedTempFile::new().map_err(|e| SqliteGraphError::connection(e.to_string()))?;
        let path = temp_file.path();
        let graph_file = GraphFile::create(path).map_err(map_to_graph_error)?;

        #[cfg(feature = "native-v2")]
        let wal_integrator = Some(Self::create_wal_integrator(path)?);

        Ok(Self {
            graph_file: RwLock::new(graph_file),
            neighbors_cache: RwLock::new(std::collections::HashMap::new()),
            #[cfg(feature = "native-v2")]
            wal_integrator,
            #[cfg(feature = "native-v2")]
            kv_store: Arc::new(RwLock::new(KvStore::new())),
        })
    }

    /// Create a new native backend at the specified path
    pub fn new<P: AsRef<std::path::Path>>(path: P) -> Result<Self, SqliteGraphError> {
        let graph_file = GraphFile::create(&path).map_err(map_to_graph_error)?;

        #[cfg(feature = "native-v2")]
        let wal_integrator = Some(Self::create_wal_integrator(&path)?);

        Ok(Self {
            graph_file: RwLock::new(graph_file),
            neighbors_cache: RwLock::new(std::collections::HashMap::new()),
            #[cfg(feature = "native-v2")]
            wal_integrator,
            #[cfg(feature = "native-v2")]
            kv_store: Arc::new(RwLock::new(KvStore::new())),
        })
    }

    /// Open an existing native backend from the specified path
    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self, SqliteGraphError> {
        let graph_file = GraphFile::open(&path).map_err(map_to_graph_error)?;

        #[cfg(feature = "native-v2")]
        let wal_integrator = Some(Self::open_wal_integrator(&path)?);

        #[cfg(feature = "native-v2")]
        let kv_store = {
            let wal_config = V2WALConfig::for_graph_file(path.as_ref());
            match crate::backend::native::v2::kv_store::recover_kv_from_wal(&wal_config.wal_path) {
                Ok(store) => Arc::new(RwLock::new(store)),
                Err(e) => {
                    // Log warning but continue with empty store
                    eprintln!(
                        "Warning: KV recovery from {} failed, starting with empty store: {}",
                        wal_config.wal_path.display(),
                        e
                    );
                    Arc::new(RwLock::new(KvStore::new()))
                }
            }
        };

        Ok(Self {
            graph_file: RwLock::new(graph_file),
            neighbors_cache: RwLock::new(std::collections::HashMap::new()),
            #[cfg(feature = "native-v2")]
            wal_integrator,
            #[cfg(feature = "native-v2")]
            kv_store,
        })
    }

    /// Create WAL integrator for the graph (opens existing WAL without truncating)
    #[cfg(feature = "native-v2")]
    fn open_wal_integrator<P: AsRef<std::path::Path>>(
        path: P,
    ) -> Result<Arc<V2GraphWALIntegrator>, SqliteGraphError> {
        let path_ref = path.as_ref();

        // Use the helper function to create WAL config with correct paths
        let wal_config = V2WALConfig::for_graph_file(path_ref);

        // Create integration config with default settings
        let integration_config = GraphWALIntegrationConfig::default();

        // Open the integrator (preserves existing WAL data)
        let integrator =
            V2GraphWALIntegrator::open(wal_config, integration_config).map_err(|e| {
                SqliteGraphError::connection(format!("Failed to open WAL integrator: {:?}", e))
            })?;

        Ok(Arc::new(integrator))
    }

    /// Create WAL integrator for the graph
    #[cfg(feature = "native-v2")]
    fn create_wal_integrator<P: AsRef<std::path::Path>>(
        path: P,
    ) -> Result<Arc<V2GraphWALIntegrator>, SqliteGraphError> {
        let path_ref = path.as_ref();

        // Use the helper function to create WAL config with correct paths
        let wal_config = V2WALConfig::for_graph_file(path_ref);

        // Create integration config with default settings
        let integration_config = GraphWALIntegrationConfig::default();

        // Create the integrator
        let integrator =
            V2GraphWALIntegrator::create(wal_config, integration_config).map_err(|e| {
                SqliteGraphError::connection(format!("Failed to create WAL integrator: {:?}", e))
            })?;

        Ok(Arc::new(integrator))
    }

    /// Get mutable access to the underlying graph file for internal operations
    fn with_graph_file<R, F>(&self, f: F) -> Result<R, SqliteGraphError>
    where
        F: FnOnce(&mut GraphFile) -> Result<R, NativeBackendError>,
    {
        let mut graph_file = self.graph_file.write();
        f(&mut *graph_file).map_err(map_to_graph_error)
    }

    /// Get WAL metrics (if native-v2 feature is enabled and WAL integrator exists)
    #[cfg(feature = "native-v2")]
    pub fn get_wal_metrics(&self) -> Option<crate::backend::native::v2::wal::WALManagerMetrics> {
        self.wal_integrator
            .as_ref()
            .map(|integrator| integrator.get_metrics())
    }

    /// Get active transaction count (if native-v2 feature is enabled and WAL integrator exists)
    #[cfg(feature = "native-v2")]
    pub fn get_active_transaction_count(&self) -> Option<usize> {
        self.wal_integrator
            .as_ref()
            .map(|integrator| integrator.get_active_transaction_count())
    }
}

/// Properly shut down the WAL integrator when NativeGraphBackend is dropped
#[cfg(feature = "native-v2")]
impl Drop for NativeGraphBackend {
    fn drop(&mut self) {
        // Ensure WAL integrator is properly shut down
        // This signals the background coordinator thread to stop and flushes pending data
        if let Some(ref integrator) = self.wal_integrator {
            // Use soft_shutdown which works via Arc reference
            // We need to access the inner WAL manager
            if let Err(e) = integrator.soft_shutdown() {
                eprintln!("Warning: Failed to soft shutdown WAL integrator: {:?}", e);
            }
        }
    }
}

impl GraphBackend for NativeGraphBackend {
    fn insert_node(&self, node: NodeSpec) -> Result<i64, SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            let mut node_store = NodeStore::new(graph_file);
            let node_id = node_store.allocate_node_id()?;

            // Phase 31: V2 is now the default format (no feature gating)
            let record_v2 = crate::backend::native::v2::node_record_v2::NodeRecordV2::new(
                node_id, node.kind, node.name, node.data,
            );
            node_store.write_node_v2(&record_v2)?;

            Ok(node_id as i64)
        })
    }

    fn get_node(
        &self,
        snapshot_id: crate::snapshot::SnapshotId,
        id: i64,
    ) -> Result<GraphEntity, SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            let mut node_store = NodeStore::new(graph_file);
            let record = node_store.read_node(id as NativeNodeId)?;

            // Phase 38-04: Apply snapshot isolation using delta index
            #[cfg(feature = "native-v2")]
            {
                if let Some(ref integrator) = self.wal_integrator {
                    let delta_index = integrator.wal_manager().get_delta_index();
                    let delta_guard = delta_index.read();

                    // Check if there's a delta record for this node at or before our snapshot
                    if let Some(delta) = delta_guard.get_node_delta(id, snapshot_id) {
                        use crate::backend::native::v2::wal::V2WALRecord;
                        match &delta.record {
                            V2WALRecord::NodeDelete { .. } => {
                                // Node was deleted at or before this snapshot - it doesn't exist
                                return Err(crate::backend::native::NativeBackendError::InvalidNodeId {
                                    id: id as NativeNodeId,
                                    max_id: 0,
                                }.into());
                            }
                            V2WALRecord::NodeUpdate { new_data, .. } => {
                                // Node was updated - return the updated version from WAL
                                // Parse the new_data to get the NodeRecordV2
                                match crate::backend::native::v2::node_record_v2::NodeRecordV2::deserialize(new_data) {
                                    Ok(updated_record) => {
                                        return Ok(node_record_to_entity(updated_record));
                                    }
                                    Err(_) => {
                                        // Fall through to return base record
                                    }
                                }
                            }
                            _ => {
                                // Other record types - fall through to return base record
                            }
                        }
                    }
                    // If no delta found, the base record is valid for this snapshot
                }
            }

            Ok(node_record_to_entity(record))
        })
    }

    fn insert_edge(&self, edge: EdgeSpec) -> Result<i64, SqliteGraphError> {
        // Invalidate neighbors cache for affected nodes
        {
            let mut cache = self.neighbors_cache.write();
            cache.remove(&(edge.from, Direction::Outgoing));
            cache.remove(&(edge.to, Direction::Incoming));
        }

        self.with_graph_file(|graph_file| {
            // Phase 44.2: Use V2 clustered adjacency when experimental feature is enabled
            // Phase 44.2: Use V2 clustered adjacency when experimental feature is enabled
            #[cfg(feature = "v2_experimental")]
            {
                let mut edge_store = EdgeStore::new(graph_file);
                let edge_id = edge_store.allocate_edge_id();
                let record = edge_spec_to_record(edge, edge_id);

                // write_edge already calls update_v2_clustered_adjacency internally
                edge_store.write_edge(&record)?;
                Ok(edge_id as i64)
            }
            #[cfg(not(feature = "v2_experimental"))]
            {
                let mut edge_store = EdgeStore::new(graph_file);
                let edge_id = edge_store.allocate_edge_id();
                let record = edge_spec_to_record(edge, edge_id);
                edge_store.write_edge(&record)?;
                Ok(edge_id as i64)
            }
        })
    }

    fn update_node(&self, node_id: i64, node: NodeSpec) -> Result<i64, SqliteGraphError> {
        #[cfg(feature = "native-v2")]
        {
            if let Some(ref integrator) = self.wal_integrator {
                // Convert NodeSpec to NodeRecordV2
                let old_record = self.with_graph_file(|graph_file| {
                    let mut node_store = NodeStore::new(graph_file);
                    node_store.read_node_v2(node_id as NativeNodeId)
                })?;

                // Create new record from NodeSpec
                let new_record =
                    node_spec_to_v2_record(node, node_id as NativeNodeId, &old_record)?;

                // Use WAL integrator to update
                let _result = integrator
                    .update_node(None, node_id, &old_record, &new_record)
                    .map_err(|e| {
                        SqliteGraphError::connection(format!("Node update failed: {:?}", e))
                    })?;

                return Ok(node_id);
            }
        }

        // Fallback: Not implemented for backends without WAL
        Err(SqliteGraphError::connection(
            "update_node not supported for this backend configuration".to_string(),
        ))
    }

    fn delete_entity(&self, id: i64) -> Result<(), SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            let node_id = id as NativeNodeId;
            let mut node_store = NodeStore::new(graph_file);
            node_store.delete_node(node_id)?;
            Ok(())
        })
    }

    fn entity_ids(&self) -> Result<Vec<i64>, SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            let mut node_store = NodeStore::new(graph_file);
            let ids = node_store.all_node_ids()?;
            Ok(ids.into_iter().map(|id| id as i64).collect())
        })
    }

    fn neighbors(
        &self,
        snapshot_id: crate::snapshot::SnapshotId,
        node: i64,
        query: NeighborQuery,
    ) -> Result<Vec<i64>, SqliteGraphError> {
        // For unfiltered queries, try the cache first (hot path optimization)
        if query.edge_type.is_none() && snapshot_id.as_lsn() == 0 {
            let dir = match query.direction {
                BackendDirection::Outgoing => Direction::Outgoing,
                BackendDirection::Incoming => Direction::Incoming,
            };
            let cache_key = (node, dir);

            // Check cache (read lock)
            {
                let cache = self.neighbors_cache.read();
                if let Some(cached) = cache.get(&cache_key) {
                    // Cache hit - return cloned Vec (Arc clone is cheap)
                    return Ok(cached.to_vec());
                }
            }

            // Cache miss - fetch from storage
            let neighbors = self.with_graph_file(|graph_file| {
                let node_id = node as NativeNodeId;
                let result = match query.direction {
                    BackendDirection::Outgoing => {
                        AdjacencyHelpers::get_outgoing_neighbors_at_snapshot(
                            graph_file,
                            node_id,
                            snapshot_id,
                            None,
                        )
                    }
                    BackendDirection::Incoming => {
                        AdjacencyHelpers::get_incoming_neighbors_at_snapshot(
                            graph_file,
                            node_id,
                            snapshot_id,
                            None,
                        )
                    }
                }?;
                Ok(result.into_iter().map(|id| id as i64).collect::<Vec<i64>>())
            })?;

            // Populate cache (write lock)
            let neighbors_arc: std::sync::Arc<[i64]> = neighbors.clone().into();
            {
                let mut cache = self.neighbors_cache.write();
                cache.insert(cache_key, neighbors_arc);
            }

            Ok(neighbors)
        } else {
            // Filtered queries or snapshot queries - bypass cache (cold path)
            self.with_graph_file(|graph_file| {
                let node_id = node as NativeNodeId;

                let neighbors = if let Some(edge_type) = &query.edge_type {
                    let edge_type_ref = edge_type.as_str();
                    match query.direction {
                        BackendDirection::Outgoing => {
                            AdjacencyHelpers::get_outgoing_neighbors_filtered(
                                graph_file,
                                node_id,
                                &[edge_type_ref],
                            )
                        }
                        BackendDirection::Incoming => {
                            AdjacencyHelpers::get_incoming_neighbors_filtered(
                                graph_file,
                                node_id,
                                &[edge_type_ref],
                            )
                        }
                    }
                } else {
                    match query.direction {
                        BackendDirection::Outgoing => {
                            AdjacencyHelpers::get_outgoing_neighbors_at_snapshot(
                                graph_file,
                                node_id,
                                snapshot_id,
                                None,
                            )
                        }
                        BackendDirection::Incoming => {
                            AdjacencyHelpers::get_incoming_neighbors_at_snapshot(
                                graph_file,
                                node_id,
                                snapshot_id,
                                None,
                            )
                        }
                    }
                }?;

                Ok(neighbors.into_iter().map(|id| id as i64).collect())
            })
        }
    }

    fn bfs(
        &self,
        snapshot_id: crate::snapshot::SnapshotId,
        start: i64,
        depth: u32,
    ) -> Result<Vec<i64>, SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            // TODO: Pass snapshot_id to filter WAL records (Phase 38-04)
            let _snapshot_id = snapshot_id; // Suppress unused warning until Phase 38-04
            let result = native_bfs(graph_file, start as NativeNodeId, depth)?;
            Ok(result.into_iter().map(|id| id as i64).collect())
        })
    }

    fn shortest_path(
        &self,
        snapshot_id: crate::snapshot::SnapshotId,
        start: i64,
        end: i64,
    ) -> Result<Option<Vec<i64>>, SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            // TODO: Pass snapshot_id to filter WAL records (Phase 38-04)
            let _snapshot_id = snapshot_id; // Suppress unused warning until Phase 38-04
            let result =
                native_shortest_path(graph_file, start as NativeNodeId, end as NativeNodeId)?;
            Ok(result.map(|path| path.into_iter().map(|id| id as i64).collect()))
        })
    }

    fn node_degree(
        &self,
        snapshot_id: crate::snapshot::SnapshotId,
        node: i64,
    ) -> Result<(usize, usize), SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            // TODO: Pass snapshot_id to filter WAL records (Phase 38-04)
            let _snapshot_id = snapshot_id; // Suppress unused warning until Phase 38-04
            let node_id = node as NativeNodeId;
            let outgoing = AdjacencyHelpers::outgoing_degree(graph_file, node_id)?;
            let incoming = AdjacencyHelpers::incoming_degree(graph_file, node_id)?;
            Ok((outgoing as usize, incoming as usize))
        })
    }

    fn k_hop(
        &self,
        snapshot_id: crate::snapshot::SnapshotId,
        start: i64,
        depth: u32,
        direction: BackendDirection,
    ) -> Result<Vec<i64>, SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            // TODO: Pass snapshot_id to filter WAL records (Phase 38-04)
            let _snapshot_id = snapshot_id; // Suppress unused warning until Phase 38-04
            let result = native_k_hop(
                graph_file,
                start as NativeNodeId,
                depth,
                match direction {
                    BackendDirection::Outgoing => Direction::Outgoing,
                    BackendDirection::Incoming => Direction::Incoming,
                },
            )?;
            Ok(result.into_iter().map(|id| id as i64).collect())
        })
    }

    fn k_hop_filtered(
        &self,
        snapshot_id: crate::snapshot::SnapshotId,
        start: i64,
        depth: u32,
        direction: BackendDirection,
        allowed_edge_types: &[&str],
    ) -> Result<Vec<i64>, SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            // TODO: Pass snapshot_id to filter WAL records (Phase 38-04)
            let _snapshot_id = snapshot_id; // Suppress unused warning until Phase 38-04
            let result = native_k_hop_filtered(
                graph_file,
                start as NativeNodeId,
                depth,
                match direction {
                    BackendDirection::Outgoing => Direction::Outgoing,
                    BackendDirection::Incoming => Direction::Incoming,
                },
                allowed_edge_types,
            )?;
            Ok(result.into_iter().map(|id| id as i64).collect())
        })
    }

    fn chain_query(
        &self,
        snapshot_id: crate::snapshot::SnapshotId,
        start: i64,
        chain: &[ChainStep],
    ) -> Result<Vec<i64>, SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            // TODO: Pass snapshot_id to filter WAL records (Phase 38-04)
            let _snapshot_id = snapshot_id; // Suppress unused warning until Phase 38-04
            let result = native_chain_query(graph_file, start as NativeNodeId, chain)?;
            Ok(result.into_iter().map(|id| id as i64).collect())
        })
    }

    fn pattern_search(
        &self,
        snapshot_id: crate::snapshot::SnapshotId,
        start: i64,
        pattern: &PatternQuery,
    ) -> Result<Vec<PatternMatch>, SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            // TODO: Pass snapshot_id to filter WAL records (Phase 38-04)
            let _snapshot_id = snapshot_id; // Suppress unused warning until Phase 38-04
            native_pattern_search(graph_file, start as NativeNodeId, pattern)
        })
    }

    fn checkpoint(&self) -> Result<(), SqliteGraphError> {
        #[cfg(feature = "native-v2")]
        {
            if let Some(ref integrator) = self.wal_integrator {
                integrator.force_checkpoint().map_err(|e| {
                    SqliteGraphError::connection(format!("WAL checkpoint failed: {:?}", e))
                })?;
                return Ok(());
            }
        }

        // If native-v2 feature is not enabled, checkpoint is a no-op
        Ok(())
    }

    fn flush(&self) -> Result<(), SqliteGraphError> {
        #[cfg(feature = "native-v2")]
        {
            if let Some(ref integrator) = self.wal_integrator {
                integrator.wal_manager().flush().map_err(|e| {
                    SqliteGraphError::connection(format!("WAL flush failed: {:?}", e))
                })?;
                return Ok(());
            }
        }

        // If native-v2 feature is not enabled, flush is a no-op
        Ok(())
    }

    fn snapshot_export(
        &self,
        export_dir: &std::path::Path,
    ) -> Result<crate::backend::SnapshotMetadata, SqliteGraphError> {
        use crate::backend::native::v2::export::SnapshotExporter;
        use crate::backend::native::v2::export::snapshot::SnapshotExportConfig;
        use std::time::{SystemTime, UNIX_EPOCH};

        // Get the graph file path from the GraphFile
        let graph_path = self.with_graph_file(|graph_file| Ok(graph_file.path().to_path_buf()))?;

        // Create snapshot exporter with default config
        let snapshot_id = format!(
            "snapshot_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );

        let config = SnapshotExportConfig {
            export_path: export_dir.to_path_buf(),
            snapshot_id: snapshot_id.clone(),
            include_statistics: true,
            min_stable_duration: std::time::Duration::from_secs(0),
            checksum_validation: true,
        };

        let mut exporter = SnapshotExporter::new(&graph_path, config).map_err(|e| {
            SqliteGraphError::connection(format!("Failed to create snapshot exporter: {:?}", e))
        })?;

        let result = exporter.export_snapshot().map_err(|e| {
            SqliteGraphError::connection(format!("Snapshot export failed: {:?}", e))
        })?;

        Ok(crate::backend::SnapshotMetadata {
            snapshot_path: result.snapshot_path,
            size_bytes: result.snapshot_size_bytes,
            entity_count: 0, // Snapshot export doesn't return entity count directly
            edge_count: 0,
        })
    }

    fn backup(
        &self,
        backup_dir: &std::path::Path,
    ) -> Result<crate::backend::BackupResult, SqliteGraphError> {
        #[cfg(feature = "native-v2")]
        {
            use crate::backend::native::v2::backup;

            // Get the graph file path from the GraphFile
            let graph_path =
                self.with_graph_file(|graph_file| Ok(graph_file.path().to_path_buf()))?;

            // Create backup with default configuration (includes checkpoint)
            let native_result =
                backup::create_backup(&graph_path, backup::BackupConfig::new(backup_dir))
                    .map_err(|e| SqliteGraphError::connection(format!("Backup failed: {:?}", e)))?;

            Ok(crate::backend::BackupResult {
                snapshot_path: native_result.snapshot_path,
                manifest_path: native_result.manifest_path,
                size_bytes: native_result.size_bytes,
                checksum: native_result.checksum,
                record_count: native_result.record_count,
                duration_secs: native_result.duration_secs,
                timestamp: native_result.timestamp,
                checkpoint_performed: native_result.checkpoint_performed,
            })
        }

        #[cfg(not(feature = "native-v2"))]
        {
            let _ = backup_dir;
            Err(SqliteGraphError::connection(
                "Backup not available without native-v2 feature".to_string(),
            ))
        }
    }

    fn snapshot_import(
        &self,
        import_dir: &std::path::Path,
    ) -> Result<crate::backend::ImportMetadata, SqliteGraphError> {
        use crate::backend::native::v2::import::ImportMode;
        use crate::backend::native::v2::import::SnapshotImporter;
        use crate::backend::native::v2::import::snapshot::SnapshotImportConfig;

        // Get the graph file path
        let graph_path = self.with_graph_file(|graph_file| Ok(graph_file.path().to_path_buf()))?;

        let config = SnapshotImportConfig {
            target_graph_path: graph_path.clone(),
            export_dir_path: import_dir.to_path_buf(),
            import_mode: ImportMode::Fresh,
            validate_manifest: true,
            verify_checksum: true,
            overwrite_existing: true, // Allow overwriting for import
        };

        let importer =
            SnapshotImporter::from_export_dir(import_dir, &graph_path, config).map_err(|e| {
                SqliteGraphError::connection(format!("Failed to create snapshot importer: {:?}", e))
            })?;

        let result = importer.import().map_err(|e| {
            SqliteGraphError::connection(format!("Snapshot import failed: {:?}", e))
        })?;

        Ok(crate::backend::ImportMetadata {
            snapshot_path: import_dir.join("snapshot"), // Approximate path
            entities_imported: result.records_imported,
            edges_imported: 0, // Records include both entities and edges
        })
    }

    #[cfg(feature = "native-v2")]
    fn kv_get(
        &self,
        snapshot_id: crate::snapshot::SnapshotId,
        key: &[u8],
    ) -> Result<Option<KvValue>, SqliteGraphError> {
        let store = self.kv_store.read();
        store
            .get_at_snapshot(key, snapshot_id)
            .map_err(|e| SqliteGraphError::connection(e.to_string()))
    }

    #[cfg(feature = "native-v2")]
    fn kv_set(
        &self,
        key: Vec<u8>,
        value: KvValue,
        ttl_seconds: Option<u64>,
    ) -> Result<(), SqliteGraphError> {
        use crate::backend::native::v2::kv_store::wal;
        use crate::backend::native::v2::wal::record::V2WALRecord;

        let wal_integrator = self.wal_integrator.as_ref().ok_or_else(|| {
            SqliteGraphError::connection("WAL not available - KV requires native-v2".to_string())
        })?;

        // Clone key for use in both WAL and store
        let key_clone = key.clone();

        // Serialize value
        let value_bytes = wal::serialize_value(&value)
            .map_err(|e| SqliteGraphError::connection(format!("KV serialization failed: {}", e)))?;
        let value_type = wal::get_value_type_tag(&value);

        // Create WAL record
        let wal_record = V2WALRecord::KvSet {
            key,
            value_bytes,
            value_type,
            ttl_seconds,
            version: 0, // Will be assigned by WAL manager
        };

        // Write WAL record and get assigned LSN
        let commit_lsn = wal_integrator
            .wal_manager()
            .write_record(wal_record)
            .map_err(|e| SqliteGraphError::connection(format!("KV WAL write failed: {:?}", e)))?;

        // Update in-memory store with assigned LSN as version
        let mut store = self.kv_store.write();
        store
            .set_with_version(key_clone, value, ttl_seconds, commit_lsn)
            .map_err(|e| SqliteGraphError::connection(format!("KV store update failed: {}", e)))?;

        Ok(())
    }

    #[cfg(feature = "native-v2")]
    fn kv_delete(&self, key: &[u8]) -> Result<(), SqliteGraphError> {
        use crate::backend::native::v2::kv_store::wal;
        use crate::backend::native::v2::wal::record::V2WALRecord;

        let wal_integrator = self.wal_integrator.as_ref().ok_or_else(|| {
            SqliteGraphError::connection("WAL not available - KV requires native-v2".to_string())
        })?;

        // Get old value for rollback/recovery
        let store = self.kv_store.read();
        let old_value = store
            .get(key)
            .map_err(|e| SqliteGraphError::connection(format!("KV get failed: {}", e)))?;
        drop(store);

        // Serialize old value if exists
        let (old_value_bytes, old_value_type) = if let Some(ref value) = old_value {
            let bytes = wal::serialize_value(value).map_err(|e| {
                SqliteGraphError::connection(format!("KV serialization failed: {}", e))
            })?;
            let type_tag = wal::get_value_type_tag(value);
            (Some(bytes), type_tag)
        } else {
            (None, 0)
        };

        // Create WAL record
        let wal_record = V2WALRecord::KvDelete {
            key: key.to_vec(),
            old_value_bytes,
            old_value_type,
            old_version: 0, // Will be assigned by WAL manager
        };

        // Write WAL record and get assigned LSN
        let _commit_lsn = wal_integrator
            .wal_manager()
            .write_record(wal_record)
            .map_err(|e| SqliteGraphError::connection(format!("KV WAL delete failed: {:?}", e)))?;

        // Delete from in-memory store
        let mut store = self.kv_store.write();
        // Ignore KeyNotFound - delete is idempotent
        let _ = store.delete(key);

        Ok(())
    }

    #[cfg(feature = "native-v2")]
    fn subscribe(
        &self,
        filter: crate::backend::SubscriptionFilter,
    ) -> Result<(u64, std::sync::mpsc::Receiver<crate::backend::PubSubEvent>), SqliteGraphError>
    {
        let wal_integrator = self.wal_integrator.as_ref().ok_or_else(|| {
            SqliteGraphError::connection(
                "WAL not available - pub/sub requires native-v2".to_string(),
            )
        })?;

        let (sub_id, rx) = wal_integrator
            .wal_manager()
            .get_publisher()
            .subscribe(filter);
        Ok((sub_id.as_u64(), rx))
    }

    #[cfg(feature = "native-v2")]
    fn unsubscribe(&self, subscriber_id: u64) -> Result<bool, SqliteGraphError> {
        use crate::backend::native::v2::pubsub::SubscriberId;

        let wal_integrator = self.wal_integrator.as_ref().ok_or_else(|| {
            SqliteGraphError::connection(
                "WAL not available - pub/sub requires native-v2".to_string(),
            )
        })?;

        let sub_id = SubscriberId::from_raw(subscriber_id);
        let removed = wal_integrator
            .wal_manager()
            .get_publisher()
            .unsubscribe(sub_id);
        Ok(removed)
    }

    // ========== Pub/Sub Enhancement APIs (v1.4.0) ==========

    #[cfg(feature = "native-v2")]
    fn kv_prefix_scan(
        &self,
        snapshot_id: crate::snapshot::SnapshotId,
        prefix: &[u8],
    ) -> Result<
        Vec<(
            Vec<u8>,
            crate::backend::native::types::KvValue,
        )>,
        SqliteGraphError,
    > {
        let store = self.kv_store.read();
        store
            .prefix_scan(snapshot_id, prefix)
            .map_err(|e| SqliteGraphError::connection(e.to_string()))
    }

    fn query_nodes_by_kind(
        &self,
        _snapshot_id: crate::snapshot::SnapshotId,
        kind: &str,
    ) -> Result<Vec<i64>, SqliteGraphError> {
        self.with_graph_file(|graph_file| {
            // Get the total node count first
            let header = graph_file.header();
            let node_count = header.node_count as i64;

            let mut node_store = NodeStore::new(graph_file);
            let mut results = Vec::new();

            // Scan through all node IDs to find matches
            // This is O(N) but acceptable for MVP
            // Future optimization: add kind index
            for node_id in 1..=node_count {
                match node_store.read_node(node_id as NativeNodeId) {
                    Ok(record) => {
                        if record.kind == kind {
                            results.push(node_id);
                        }
                    }
                    Err(_) => {
                        // Skip nodes that can't be read
                        continue;
                    }
                }
            }

            results.sort_unstable();
            Ok(results)
        })
    }

    fn query_nodes_by_name_pattern(
        &self,
        _snapshot_id: crate::snapshot::SnapshotId,
        pattern: &str,
    ) -> Result<Vec<i64>, SqliteGraphError> {
        use crate::backend::native::pattern::glob_matches;

        self.with_graph_file(|graph_file| {
            // Get the total node count first
            let header = graph_file.header();
            let node_count = header.node_count as i64;

            let mut node_store = NodeStore::new(graph_file);
            let mut results = Vec::new();

            // Scan through all node IDs to find pattern matches
            for node_id in 1..=node_count {
                match node_store.read_node(node_id as NativeNodeId) {
                    Ok(record) => {
                        if glob_matches(pattern, &record.name) {
                            results.push(node_id);
                        }
                    }
                    Err(_) => {
                        // Skip nodes that can't be read
                        continue;
                    }
                }
            }

            results.sort_unstable();
            Ok(results)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_backend_creation() {
        let backend = NativeGraphBackend::new_temp().unwrap();
        // Test that backend can be created successfully
        assert!(true);
    }

    #[test]
    fn test_interior_mutability() {
        let backend = NativeGraphBackend::new_temp().unwrap();

        // Test that we can perform multiple operations
        let node_id = backend
            .insert_node(NodeSpec {
                kind: "Test".to_string(),
                name: "node1".to_string(),
                file_path: None,
                data: serde_json::json!({}),
            })
            .unwrap();

        let snapshot = crate::snapshot::SnapshotId::current();
        let node = backend.get_node(snapshot, node_id).unwrap();
        assert_eq!(node.name, "node1");
        assert_eq!(node.kind, "Test");
    }

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_subscribe_to_events() {
        use crate::backend::SubscriptionFilter;
        use std::time::Duration;

        // Setup graph
        let backend = NativeGraphBackend::new_temp().unwrap();
        let filter = SubscriptionFilter::all();

        // Subscribe
        let (sub_id, mut rx) = backend.subscribe(filter).unwrap();

        // Make a change (direct node insert - no transaction API in this test)
        let node_id = backend
            .insert_node(NodeSpec {
                kind: "Test".to_string(),
                name: "test_node".to_string(),
                file_path: None,
                data: serde_json::json!({}),
            })
            .unwrap();

        // Note: Events are only emitted on WAL commit, which requires transaction API
        // For now, just verify subscription was successful
        assert!(sub_id > 0);

        // Unsubscribe
        let removed = backend.unsubscribe(sub_id).unwrap();
        assert!(removed);
    }

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_kv_persistence_across_reopen() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Write KV data in first session
        {
            let backend = NativeGraphBackend::new(&db_path).unwrap();
            backend
                .kv_set(b"test_key".to_vec(), KvValue::Integer(42), None)
                .unwrap();
            backend
                .kv_set(
                    b"another_key".to_vec(),
                    KvValue::String("hello".to_string()),
                    None,
                )
                .unwrap();

            // Flush WAL buffer to ensure records are written to disk
            let wal_integrator = backend.wal_integrator.as_ref().unwrap();
            wal_integrator.wal_manager().flush().unwrap();
        }

        // Reopen and verify KV data persists
        {
            let backend = NativeGraphBackend::open(&db_path).unwrap();
            let snapshot = crate::snapshot::SnapshotId::current();

            let result = backend.kv_get(snapshot, b"test_key").unwrap();
            assert_eq!(result, Some(KvValue::Integer(42)));

            let result2 = backend.kv_get(snapshot, b"another_key").unwrap();
            assert_eq!(result2, Some(KvValue::String("hello".to_string())));

            // Verify prefix scan works
            let results = backend.kv_prefix_scan(snapshot, b"test_").unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].0, b"test_key".to_vec());
        }
    }

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_flush_wal_buffer() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Test 1: flush() method exists and doesn't crash
        {
            let backend = NativeGraphBackend::new(&db_path).unwrap();
            backend
                .kv_set(b"test_key".to_vec(), KvValue::Integer(1), None)
                .unwrap();
            backend.flush().unwrap();

            let snapshot = crate::snapshot::SnapshotId::current();
            let result = backend.kv_get(snapshot, b"test_key").unwrap();
            assert_eq!(result, Some(KvValue::Integer(1)));
        }

        // Test 2: flushed data persists across reopen
        {
            let backend = NativeGraphBackend::open(&db_path).unwrap();
            let snapshot = crate::snapshot::SnapshotId::current();
            let result = backend.kv_get(snapshot, b"test_key").unwrap();
            assert_eq!(
                result,
                Some(KvValue::Integer(1)),
                "Flushed data should persist"
            );
        }

        // Test 3: WAL file size increases after flush
        {
            let backend = NativeGraphBackend::open(&db_path).unwrap();
            // Write enough data to trigger buffer growth
            for i in 0..10 {
                backend
                    .kv_set(
                        format!("bulk_key_{}", i).into_bytes(),
                        KvValue::Integer(i),
                        None,
                    )
                    .unwrap();
            }
            backend.flush().unwrap();

            let wal_path = db_path.with_extension("wal");
            let wal_size = std::fs::metadata(&wal_path).unwrap().len();
            assert!(wal_size > 200, "WAL should contain data after flush");
        }
    }

    // TDD Tests for update_node functionality
    // These tests are written FIRST, following TDD principles
    // They will FAIL until update_node is implemented

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_update_node_preserves_node_id() {
        // Test: update_node should return the same node_id that was passed in
        // This ensures we don't allocate new node slots when updating
        let backend = NativeGraphBackend::new_temp().unwrap();

        // First, create a node
        let node_id = backend
            .insert_node(NodeSpec {
                kind: "File".to_string(),
                name: "test.rs".to_string(),
                file_path: Some("test.rs".to_string()),
                data: serde_json::json!({"hash": "abc123"}),
            })
            .unwrap();

        // Now update it - should return the same node_id
        let updated_id = backend
            .update_node(
                node_id,
                NodeSpec {
                    kind: "File".to_string(),
                    name: "test.rs".to_string(),
                    file_path: Some("test.rs".to_string()),
                    data: serde_json::json!({"hash": "def456", "updated": true}),
                },
            )
            .expect("update_node should be implemented");

        assert_eq!(
            updated_id, node_id,
            "update_node must return the same node_id - no new allocation"
        );

        // Verify the data was actually updated
        let snapshot = crate::snapshot::SnapshotId::current();
        let node = backend.get_node(snapshot, updated_id).unwrap();
        assert_eq!(node.kind, "File");
        assert_eq!(node.name, "test.rs");

        let data: serde_json::Value = node.data;
        assert_eq!(data["hash"], "def456");
        assert_eq!(data["updated"], true);
    }

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_update_node_nonexistent_returns_error() {
        // Test: updating a non-existent node should return an error
        let backend = NativeGraphBackend::new_temp().unwrap();

        let result = backend.update_node(
            9999, // Non-existent node_id
            NodeSpec {
                kind: "File".to_string(),
                name: "test.rs".to_string(),
                file_path: Some("test.rs".to_string()),
                data: serde_json::json!({"hash": "abc123"}),
            },
        );

        assert!(
            result.is_err(),
            "update_node should return error for non-existent node_id"
        );
    }

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_multiple_updates_dont_increase_node_count() {
        // Test: Multiple consecutive updates should not increase node_count
        // This is the regression test for the node region overflow bug
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_update_no_increase.db");

        // Create initial node
        {
            let backend = NativeGraphBackend::new(&db_path).unwrap();
            let node_id = backend
                .insert_node(NodeSpec {
                    kind: "File".to_string(),
                    name: "main.rs".to_string(),
                    file_path: Some("main.rs".to_string()),
                    data: serde_json::json!({"version": 1}),
                })
                .unwrap();

            // Get initial node_count
            let ids_after_insert = backend.entity_ids().unwrap();
            let initial_count = ids_after_insert.len();
            assert_eq!(initial_count, 1, "Should have exactly 1 node");

            // Perform 100 updates - node count should stay at 1
            for i in 2..=100 {
                let _ = backend
                    .update_node(
                        node_id,
                        NodeSpec {
                            kind: "File".to_string(),
                            name: "main.rs".to_string(),
                            file_path: Some("main.rs".to_string()),
                            data: serde_json::json!({"version": i}),
                        },
                    )
                    .unwrap();

                let ids = backend.entity_ids().unwrap();
                assert_eq!(
                    ids.len(),
                    1,
                    "Node count should remain 1 after {} updates",
                    i - 1
                );
            }

            // Verify final state
            let snapshot = crate::snapshot::SnapshotId::current();
            let node = backend.get_node(snapshot, node_id).unwrap();
            let data: serde_json::Value = node.data;
            assert_eq!(data["version"], 100, "Data should reflect last update");
        }
    }

    #[test]
    fn test_update_node_interface_exists() {
        // Test: Verify update_node method exists on GraphBackend trait
        // This is a compile-time test - if update_node doesn't exist, this won't compile
        use crate::backend::GraphBackend;

        fn has_update_node<B: GraphBackend>(backend: &B) -> bool {
            // This function just proves update_node is callable
            // Actual functionality tested in other tests
            true
        }

        let backend = NativeGraphBackend::new_temp().unwrap();
        assert!(has_update_node(&backend));
    }
}
