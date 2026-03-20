use sqlitegraph::backend::GraphBackend;
use sqlitegraph::backend::SqliteGraphBackend;

/// CLI backend client supporting SQLite and V3 backends
pub enum CliClient {
    Sqlite(SqliteGraphBackend),
    #[cfg(feature = "native-v3")]
    V3(sqlitegraph::backend::native::v3::V3Backend),
}

impl CliClient {
    /// Open database with specified backend
    pub fn open(backend: super::cli::BackendType, path: &std::path::Path) -> anyhow::Result<Self> {
        match backend {
            super::cli::BackendType::Sqlite => {
                let graph = sqlitegraph::SqliteGraph::open(path)?;
                Ok(Self::Sqlite(SqliteGraphBackend::from_graph(graph)))
            }
            #[cfg(feature = "native-v3")]
            super::cli::BackendType::V3 => {
                use sqlitegraph::backend::native::v3::V3Backend;
                let backend = if path.exists() {
                    V3Backend::open(path)?
                } else {
                    V3Backend::create(path)?
                };
                Ok(Self::V3(backend))
            }
        }
    }

    /// Open in-memory database
    pub fn open_in_memory(backend: super::cli::BackendType) -> anyhow::Result<Self> {
        match backend {
            super::cli::BackendType::Sqlite => {
                let graph = sqlitegraph::SqliteGraph::open_in_memory()?;
                Ok(Self::Sqlite(SqliteGraphBackend::from_graph(graph)))
            }
            #[cfg(feature = "native-v3")]
            super::cli::BackendType::V3 => {
                anyhow::bail!("V3 backend does not support in-memory mode")
            }
        }
    }

    /// Get backend trait object
    pub fn backend(&self) -> &dyn GraphBackend {
        match self {
            Self::Sqlite(b) => b,
            #[cfg(feature = "native-v3")]
            Self::V3(b) => b,
        }
    }

    /// Get SQLite graph reference (if SQLite backend)
    pub fn sqlite_graph(&self) -> Option<&sqlitegraph::SqliteGraph> {
        match self {
            Self::Sqlite(b) => Some(b.graph()),
            #[cfg(feature = "native-v3")]
            Self::V3(_) => None,
        }
    }

    /// Get backend name
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Sqlite(_) => "sqlite",
            #[cfg(feature = "native-v3")]
            Self::V3(_) => "v3",
        }
    }

    /// Get node count
    pub fn node_count(&self) -> anyhow::Result<usize> {
        let ids = self.backend().entity_ids()?;
        Ok(ids.len())
    }
}
