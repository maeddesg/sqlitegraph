use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Clone, Debug, ValueEnum, Default)]
pub enum BackendType {
    #[default]
    Sqlite,
    #[cfg(feature = "native-v3")]
    V3,
}

#[derive(Parser)]
#[command(name = "sqlitegraph")]
#[command(about = "SQLiteGraph CLI - Graph database query tool")]
#[command(version)]
pub struct Cli {
    /// Database file path
    #[arg(short, long, default_value = "graph.db")]
    pub db: PathBuf,

    /// Backend type
    #[arg(short, long, value_enum, default_value = "sqlite")]
    pub backend: BackendType,

    /// Allow write operations (default is read-only)
    #[arg(long, global = true)]
    pub write: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Query using Cypher-like syntax (read-only)
    Query {
        /// Cypher-like query (e.g., "MATCH (n:User) RETURN n.name")
        query: String,
    },

    /// Show database status
    Status,

    /// List all nodes
    List {
        /// Filter by kind
        #[arg(short, long)]
        kind: Option<String>,
    },

    /// Breadth-first search
    Bfs {
        #[arg(short, long)]
        start: i64,
        #[arg(short, long, default_value = "3")]
        depth: u32,
    },

    /// Shortest path
    Path {
        #[arg(short, long)]
        from: i64,
        #[arg(short, long)]
        to: i64,
    },

    /// Get neighbors
    Neighbors {
        #[arg(short, long)]
        id: i64,
        #[arg(short, long, default_value = "outgoing")]
        direction: Direction,
    },

    /// Run graph algorithm
    Algo {
        #[command(subcommand)]
        command: AlgoCommands,
    },

    /// Export graph to file (requires --write)
    Export {
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Import graph from file (requires --write)
    Import {
        #[arg(short, long)]
        input: PathBuf,
    },

    /// Insert node (requires --write)
    Insert {
        #[arg(short, long)]
        kind: String,
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        data: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum AlgoCommands {
    /// PageRank centrality
    Pagerank {
        #[arg(short, long, default_value = "100")]
        iterations: usize,
    },
    /// Betweenness centrality
    Betweenness,
    /// Connected components
    Components,
    /// Topological sort
    Topo,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum Direction {
    Incoming,
    Outgoing,
    Both,
}
