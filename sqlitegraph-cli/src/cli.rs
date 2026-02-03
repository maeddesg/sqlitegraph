#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandLineConfig {
    pub backend: String,
    pub database: String,
    pub command: String,
    pub command_args: Vec<String>,
}

impl CommandLineConfig {
    pub fn from_args(args: &[&str]) -> Result<Self, String> {
        let mut backend = String::from("sqlite");
        let mut database = String::from("memory");
        let mut command = String::from("status");
        let mut command_args = Vec::new();
        let mut command_set = false;
        let mut iter = args.iter().skip(1);
        while let Some(arg) = iter.next() {
            if command_set {
                command_args.push(arg.to_string());
                continue;
            }
            match *arg {
                "--backend" => {
                    backend = iter
                        .next()
                        .ok_or_else(|| "--backend requires a value".to_string())?
                        .to_string();
                }
                "--db" | "--database" => {
                    database = iter
                        .next()
                        .ok_or_else(|| "--db requires a value".to_string())?
                        .to_string();
                }
                "--command" => {
                    command = iter
                        .next()
                        .ok_or_else(|| "--command requires a value".to_string())?
                        .to_string();
                    command_set = true;
                }
                other if other.starts_with('-') => {
                    return Err(format!("unknown flag {other}"));
                }
                _ => {
                    command = arg.to_string();
                    command_set = true;
                }
            }
        }
        Ok(Self {
            backend,
            database,
            command,
            command_args,
        })
    }

    pub fn help() -> &'static str {
        r#"Usage: sqlitegraph [--backend sqlite|native] [--db memory|PATH] [command] [args]

Commands:
  status                    Show database status and statistics
  list                      List all entities in the graph
  migrate [--dry-run]       Run pending schema migrations
  dump-graph --output PATH  Dump graph data to file
  load-graph --input PATH   Load graph data from file
  bulk-insert-entities --input FILE  Bulk insert entities from JSON
  bulk-insert-edges --input FILE    Bulk insert edges from JSON
  bfs --start ID --max-depth N    Breadth-first search traversal
  k-hop --start ID --depth N [--direction incoming|outgoing]  K-hop neighbor query
  shortest-path --from ID --to ID    Find shortest path between nodes
  neighbors --id ID [--direction incoming|outgoing]  Direct neighbor query
  pattern-match --edge-type TYPE [--start-label LABEL] [--end-label LABEL] [--direction incoming|outgoing] [--start-prop KEY:VAL] [--end-prop KEY:VAL]  Match triple patterns
  pattern-match-fast --edge-type TYPE [--start-label LABEL] [--end-label LABEL] [--direction incoming|outgoing] [--start-prop KEY:VAL] [--end-prop KEY:VAL]  Fast-path pattern match
  forward-reachability --start ID    Find all nodes reachable from start node
  backward-reachability --target ID  Find all nodes that can reach target node
  can-reach --from ID --to ID        Check if from node can reach to node
  unreachable-nodes --entry ID       Find nodes unreachable from entry point
  hnsw-create --dimension N --m M --ef-construction N --distance-metric TYPE [--index-name NAME]  Create HNSW index
  hnsw-insert --input FILE [--name NAME]  Insert vectors into HNSW index
  hnsw-search --input FILE --k N [--name NAME]  Search HNSW index
  hnsw-stats [--name NAME]                Show HNSW index statistics
  hnsw-list                               List all HNSW indexes in database
  hnsw-delete --index-name NAME           Delete HNSW index and all vectors
  hnsw-info [--index-name NAME]           Show detailed HNSW index information
  wal-checkpoint            Trigger WAL checkpoint operation
  wal-metrics                Show WAL performance metrics and file sizes
  wal-config                 Show WAL configuration settings
  wal-stats                  Show detailed WAL statistics with derived metrics
  snapshot-create --dir DIR  Create database snapshot
  snapshot-load --dir DIR     Load database snapshot
  debug-stats                Show graph introspection data (JSON)
  debug-dump --output PATH   Export graph structure for debugging
  debug-trace COMMAND [...]  Enable trace logging for specific operation
  pagerank --iterations N [--damping-factor F]   PageRank centrality algorithm
  betweenness                Betweenness centrality algorithm
  louvain [--max-iterations N]    Louvain community detection algorithm
  enumerate-paths --start ID [--max-depth N] [--max-paths N]  Enumerate execution paths with bounds
  enumerate-paths-constrained --start ID [--enable-dominance] [--enable-cd] [--enable-loops]  Path enumeration with pruning
  critical-path              Longest weighted path in DAG (bottleneck identification)
  cycle-basis [--max-cycles N] [--max-cycle-length N]  Minimal cycle basis for cycle explanation
  wcc                       Weakly Connected Components (undirected connectivity)
  scc                       Strongly Connected Components (Tarjan's algorithm)
  transitive-closure [--max-depth N] [--max-sources N] [--max-pairs N]  All-pairs reachability
  transitive-reduction      Remove redundant edges while preserving reachability
  topological-sort          Topological ordering of nodes in DAGs
  structural-similarity --graph1 ID --graph2 ID  Structural similarity using isomorphism and MCS
  graph-diff --before PATH --after PATH  Structural graph delta between two snapshots
  validate-refactor --before PATH --after PATH  Refactor validation with safety heuristics
  taint-forward --sources-file FILE    Forward taint propagation from sources to sinks
  taint-backward --sink ID --sources-file FILE  Backward taint propagation from sink to sources
  sink-analysis --sources-file FILE --sinks-file FILE  Full vulnerability detection (all sinks)
  discover-sources-sinks             Discover sources/sinks using metadata-based detectors
  backward-slice --target ID         Backward program slicing (what affects this node?)
  forward-slice --source ID          Forward program slicing (what does this affect?)
  collapse-scc                       Collapse SCCs into supernodes for call graph analysis
  min-cut --source ID --sink ID      Minimum s-t edge cut for fault tolerance analysis
  min-vertex-cut --source ID --sink ID  Minimum vertex cut for critical node identification
  dominators --entry ID              Compute dominators and immediate dominator tree
  post-dominators [--exit ID]        Compute post-dominators (auto-detects exit if omitted)
  control-dependence [--exit ID]     Compute Control Dependence Graph
  dominance-frontiers --entry ID     Compute dominance frontiers for SSA phi-placement
  natural-loops --entry ID           Detect natural loops using back-edge dominance
  happens-before --events-file FILE  Event ordering analysis for concurrent traces
  impact-radius --start ID [--max-distance N]  Blast zone computation using bounded reachability
  partition --k N [--max-size N]      Size-bounded k-way graph partitioning
  subgraph-isomorphism --pattern-file FILE  Bounded subgraph isomorphism for pattern matching
  graph-rewrite --rules-file FILE    DPO-style graph rewriting for pattern transformation

Traversal Options:
  --start                   Starting node ID for traversal
  --max-depth                Maximum depth for BFS (default: 3)
  --depth                    Hop depth for k-hop (default: 2)
  --direction               Traversal direction: incoming|outgoing (default: outgoing)
  --from                     Source node ID for shortest path or can-reach
  --to                       Target node ID for shortest path or can-reach
  --id                       Node ID for neighbors query
  --target                   Target node ID for backward reachability or backward-slice
  --entry                    Entry node ID for unreachable nodes
  --source                   Source node ID for forward-slice, min-cut, or min-vertex-cut
  --sink                     Sink node ID for min-cut or min-vertex-cut

Pattern Options:
  --edge-type                Edge type to match (required)
  --start-label              Start node label filter
  --end-label                End node label filter
  --start-prop               Start node property filter (format: key:value)
  --end-prop                 End node property filter (format: key:value)

HNSW Options:
  --dimension               Vector dimension (e.g., 768)
  --m                       Number of bi-directional links (default: 16)
  --ef-construction         HNSW ef_construction parameter (default: 200)
  --distance-metric         Distance metric: cosine|euclidean|dot|manhattan
  --index-name              Index name for create (default: "default")
  --name                    Index name for insert/search/stats (default: "default")
  --k                       Number of nearest neighbors to return

Examples:
  sqlitegraph status
  sqlitegraph --db /path/to/graph.db list
  sqlitegraph bfs --start 123 --max-depth 3
  sqlitegraph k-hop --start 123 --depth 2 --direction outgoing
  sqlitegraph shortest-path --from 123 --to 456
  sqlitegraph neighbors --id 123 --direction incoming
  sqlitegraph forward-reachability --start 123
  sqlitegraph backward-reachability --target 456
  sqlitegraph can-reach --from 123 --to 456
  sqlitegraph unreachable-nodes --entry 1
  sqlitegraph bulk-insert-entities --input entities.json
  sqlitegraph pattern-match --edge-type DEPENDS_ON --start-label "Function" --end-label "Module"
  sqlitegraph pattern-match-fast --edge-type CALLS --direction outgoing
  sqlitegraph hnsw-create --dimension 768 --m 16 --ef-construction 200 --distance-metric cosine
  sqlitegraph migrate --dry-run
  sqlitegraph backward-slice --target 456
  sqlitegraph forward-slice --source 123
  sqlitegraph collapse-scc
  sqlitegraph min-cut --source 1 --sink 10
  sqlitegraph min-vertex-cut --source 1 --sink 10
"#
    }
}
