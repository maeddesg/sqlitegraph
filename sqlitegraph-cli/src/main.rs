use std::{env, fs, path::PathBuf, process};

use serde_json::json;
use sqlitegraph::{
    algo::{
        backward_slice_with_progress, betweenness_centrality_with_progress,
        can_reach, collapse_sccs_with_progress, control_dependence_from_exit,
        control_dependence_graph, critical_path_with_progress, cycle_basis_with_progress,
        default_weight_fn, discover_sources_and_sinks_default,
        dominance_frontiers_with_progress, dominators_with_progress,
        enumerate_paths_with_dominance_progress, enumerate_paths_with_progress,
        forward_slice_with_progress, louvain_communities_with_progress, min_st_cut_with_progress,
        min_vertex_cut_with_progress, natural_loops_with_progress, pagerank_with_progress,
        post_dominators_auto_exit, post_dominators_with_progress,
        propagate_taint_backward_with_progress, propagate_taint_forward_with_progress,
        reachable_from_with_progress, reverse_reachable_from_with_progress,
        sink_reachability_analysis_with_progress, strongly_connected_components,
        structural_similarity_with_progress, topological_sort,
        transitive_closure_with_progress, TransitiveClosureBounds,
        transitive_reduction_with_progress, validate_refactor, weakly_connected_components_with_progress,
        SimilarityBounds, TopoError,
        ControlDependenceResult, CriticalPathError, DominanceFrontierResult, DominatorResult,
        NaturalLoopsResult, PathClassification, PathEnumerationConfig,
        PathEnumerationDominanceConfig, PathEnumerationResult, PostDominatorResult, SliceResult,
        unreachable_from,
    },
    backend::{BackendDirection, SqliteGraphBackend},
    bfs::{bfs_neighbors, shortest_path},
    graph_opt::{bulk_insert_edges, bulk_insert_entities, GraphEdgeCreate, GraphEntityCreate},
    hnsw::{DistanceMetric, HnswConfigBuilder},
    multi_hop::k_hop,
    pattern_engine::PatternTriple,
    progress::{ConsoleProgress, ProgressCallback},
    recovery::{dump_graph_to_path, load_graph_from_path},
    SqliteGraph, SqliteGraphError,
};
use sqlitegraph_cli::{cli::CommandLineConfig, client::BackendClient, reasoning};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", CommandLineConfig::help());
        return;
    }
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let config = match CommandLineConfig::from_args(&arg_refs) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("error: {err}");
            process::exit(2);
        }
    };

    let auto_migrate = config.command != "migrate";
    let client = match open_backend(&config, auto_migrate) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("{err}");
            process::exit(2);
        }
    };
    if let Err(err) = run_command(&client, &config.command, &config.command_args) {
        eprintln!("command failed: {err}");
        process::exit(1);
    }
}

fn open_backend(config: &CommandLineConfig, auto_migrate: bool) -> Result<BackendClient, String> {
    match config.backend.as_str() {
        "sqlite" => {
            if config.database == "memory" {
                let graph = if auto_migrate {
                    SqliteGraph::open_in_memory().map_err(|e| e.to_string())?
                } else {
                    SqliteGraph::open_in_memory_without_migrations().map_err(|e| e.to_string())?
                };
                Ok(BackendClient::new(SqliteGraphBackend::from_graph(graph)))
            } else {
                let path = PathBuf::from(&config.database);
                let graph = if auto_migrate {
                    SqliteGraph::open(&path).map_err(|e| e.to_string())?
                } else {
                    SqliteGraph::open_without_migrations(&path).map_err(|e| e.to_string())?
                };
                Ok(BackendClient::new(SqliteGraphBackend::from_graph(graph)))
            }
        }
        "native" | "native-v2" => {
            // Create NativeGraphBackend directly to have access to WAL-specific methods
            let path = PathBuf::from(&config.database);

            // Check if file exists to decide whether to create or open
            let backend = if path.exists() {
                sqlitegraph::NativeGraphBackend::open(&path)
                    .map_err(|e| format!("Failed to open native backend: {}", e))?
            } else {
                sqlitegraph::NativeGraphBackend::new(&path)
                    .map_err(|e| format!("Failed to create native backend: {}", e))?
            };

            Ok(BackendClient::new_native(backend))
        }
        other => Err(format!("unsupported backend {other}")),
    }
}

fn run_command(
    client: &BackendClient,
    command: &str,
    args: &[String],
) -> Result<(), SqliteGraphError> {
    if let Some(json) = reasoning::handle_command(client, command, args)? {
        println!("{json}");
        return Ok(());
    }
    match command {
        "status" => {
            let backend_type = client.backend_type();
            match client.graph() {
                Some(graph) => {
                    // SQLite backend - can get detailed info
                    let nodes = client
                        .entity_ids()?
                        .ok_or_else(|| SqliteGraphError::invalid_input("failed to get entity IDs"))?
                        .len();
                    let version = graph.schema_version()?;
                    println!("backend={backend_type} schema_version={version} nodes={nodes}");
                }
                None => {
                    // Native/Dynamic backend - limited info
                    println!("backend={backend_type}");
                    println!("Note: Detailed status not available for {backend_type} backend");
                }
            }
            Ok(())
        }
        "dump-graph" => {
            let graph = client.graph().ok_or_else(|| {
                SqliteGraphError::invalid_input("dump-graph command requires SQLite backend")
            })?;
            let output = required_flag_value(args, "--output")?;
            dump_graph_to_path(graph, &output)?;
            println!("dump_written=\"{output}\"");
            Ok(())
        }
        "load-graph" => {
            let graph = client.graph().ok_or_else(|| {
                SqliteGraphError::invalid_input("load-graph command requires SQLite backend")
            })?;
            let input = required_flag_value(args, "--input")?;
            load_graph_from_path(graph, &input)?;
            println!("load_applied=\"{input}\"");
            Ok(())
        }
        "migrate" => run_migrate(client, args),
        "bulk-insert-entities" => run_bulk_insert_entities(client, args),
        "bulk-insert-edges" => run_bulk_insert_edges(client, args),
        "hnsw-create" => run_hnsw_create(client, args),
        "hnsw-insert" => run_hnsw_insert(client, args),
        "hnsw-search" => run_hnsw_search(client, args),
        "hnsw-stats" => run_hnsw_stats(client, args),
        "hnsw-list" => run_hnsw_list(client, args),
        "hnsw-delete" => run_hnsw_delete(client, args),
        "hnsw-info" => run_hnsw_info(client, args),
        "bfs" => run_bfs(client, args),
        "k-hop" => run_k_hop(client, args),
        "shortest-path" => run_shortest_path(client, args),
        "neighbors" => run_neighbors(client, args),
        "pattern-match" => run_pattern_match(client, args),
        "pattern-match-fast" => run_pattern_match_fast(client, args),
        "wal-checkpoint" => run_wal_checkpoint(client, args),
        "wal-metrics" => run_wal_metrics(client, args),
        "wal-config" => run_wal_config(client, args),
        "wal-stats" => run_wal_stats(client, args),
        "snapshot-create" => run_snapshot_create(client, args),
        "snapshot-load" => run_snapshot_load(client, args),
        "debug-stats" => run_debug_stats(client, args),
        "debug-dump" => run_debug_dump(client, args),
        "debug-trace" => run_debug_trace(client, args),
        "pagerank" => run_pagerank(client, args),
        "betweenness" => run_betweenness(client, args),
        "louvain" => run_louvain(client, args),
        // Graph diff and refactor validation commands
        "structural-similarity" => run_structural_similarity(client, args),
        "graph-diff" => run_graph_diff(client, args),
        "validate-refactor" => run_validate_refactor(client, args),
        // Security and taint analysis commands
        "taint-forward" => run_taint_forward(client, args),
        "taint-backward" => run_taint_backward(client, args),
        "sink-analysis" => run_sink_analysis(client, args),
        "discover-sources-sinks" => run_discover_sources_sinks(client, args),
        // Reachability commands
        "forward-reachability" => run_forward_reachability(client, args),
        "backward-reachability" => run_backward_reachability(client, args),
        "can-reach" => run_can_reach(client, args),
        "unreachable-nodes" => run_unreachable_nodes(client, args),
        // CFG analysis commands
        "dominators" => run_dominators(client, args),
        "post-dominators" => run_post_dominators(client, args),
        "control-dependence" => run_control_dependence(client, args),
        "dominance-frontiers" => run_dominance_frontiers(client, args),
        "natural-loops" => run_natural_loops(client, args),
        // Core graph theory commands
        "wcc" => run_wcc(client, args),
        "scc" => run_scc(client, args),
        "transitive-closure" => run_transitive_closure(client, args),
        "transitive-reduction" => run_transitive_reduction(client, args),
        "topological-sort" => run_topological_sort(client, args),
        // Reindex commands removed - not available in v0.2.5
        // "reindex-all" => run_reindex_all(client, args),
        // "reindex-syncore" => run_reindex_syncore(client, args),
        // "reindex-sync-graph" => run_reindex_sync_graph(client, args),
        "list" => {
            let graph = client.graph().ok_or_else(|| {
                SqliteGraphError::invalid_input("list command requires SQLite backend")
            })?;
            for id in client
                .entity_ids()?
                .ok_or_else(|| SqliteGraphError::invalid_input("failed to get entity IDs"))?
            {
                let entity = graph.get_entity(id)?;
                println!("{}:{}", entity.id, entity.name);
            }
            Ok(())
        }
        other => {
            println!("unknown command {other}, defaulting to status");
            let graph = client.graph().ok_or_else(|| {
                SqliteGraphError::invalid_input("status command requires SQLite backend")
            })?;
            let nodes = client
                .entity_ids()?
                .ok_or_else(|| SqliteGraphError::invalid_input("failed to get entity IDs"))?
                .len();
            let version = graph.schema_version()?;
            println!("backend=sqlite schema_version={version} nodes={nodes}");
            Ok(())
        }
    }
}

fn required_flag_value(args: &[String], flag: &str) -> Result<String, SqliteGraphError> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().cloned().ok_or_else(|| {
                SqliteGraphError::invalid_input(format!("missing value for {flag}"))
            });
        }
    }
    Err(SqliteGraphError::invalid_input(format!(
        "{flag} is required"
    )))
}

fn run_migrate(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let dry_run = args.iter().any(|arg| arg == "--dry-run");
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("migrate command requires SQLite backend")
    })?;
    let report = graph.run_pending_migrations(dry_run)?;
    let payload = json!({
        "command": "migrate",
        "dry_run": dry_run,
        "from_version": report.from_version,
        "to_version": report.to_version,
        "statements": report.statements,
    });
    println!("{payload}");
    Ok(())
}

fn run_bulk_insert_entities(
    client: &BackendClient,
    args: &[String],
) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("bulk-insert-entities requires SQLite backend")
    })?;
    let input = required_flag_value(args, "--input")?;

    // Read JSON file
    let json_content = fs::read_to_string(&input)
        .map_err(|e| SqliteGraphError::invalid_input(format!("failed to read file: {e}")))?;

    // Parse JSON array manually since GraphEntityCreate doesn't implement Deserialize
    let json_array: Vec<serde_json::Value> = serde_json::from_str(&json_content)
        .map_err(|e| SqliteGraphError::invalid_input(format!("failed to parse JSON array: {e}")))?;

    let entities: Vec<GraphEntityCreate> = json_array
        .into_iter()
        .map(|v| {
            let kind = v["kind"].as_str().unwrap_or("").to_string();
            let name = v["name"].as_str().unwrap_or("").to_string();
            let file_path = v["file_path"].as_str().map(|s| s.to_string());
            let data = v.get("data").cloned().unwrap_or(serde_json::json!({}));
            GraphEntityCreate {
                kind,
                name,
                file_path,
                data,
            }
        })
        .collect();

    // Perform bulk insert
    let ids = bulk_insert_entities(graph, &entities)?;

    let payload = json!({
        "command": "bulk-insert-entities",
        "input": input,
        "entities_processed": entities.len(),
        "ids_created": ids,
    });
    println!("{payload}");
    Ok(())
}

fn run_bulk_insert_edges(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("bulk-insert-edges requires SQLite backend")
    })?;
    let input = required_flag_value(args, "--input")?;

    // Read JSON file
    let json_content = fs::read_to_string(&input)
        .map_err(|e| SqliteGraphError::invalid_input(format!("failed to read file: {e}")))?;

    // Parse JSON array manually since GraphEdgeCreate doesn't implement Deserialize
    let json_array: Vec<serde_json::Value> = serde_json::from_str(&json_content)
        .map_err(|e| SqliteGraphError::invalid_input(format!("failed to parse JSON array: {e}")))?;

    let edges: Vec<GraphEdgeCreate> = json_array
        .into_iter()
        .map(|v| {
            let from_id = v["from_id"].as_i64().unwrap_or(0);
            let to_id = v["to_id"].as_i64().unwrap_or(0);
            let edge_type = v["edge_type"].as_str().unwrap_or("").to_string();
            let data = v.get("data").cloned().unwrap_or(serde_json::json!({}));
            GraphEdgeCreate {
                from_id,
                to_id,
                edge_type,
                data,
            }
        })
        .collect();

    // Perform bulk insert
    let ids = bulk_insert_edges(graph, &edges)?;

    let payload = json!({
        "command": "bulk-insert-edges",
        "input": input,
        "edges_processed": edges.len(),
        "ids_created": ids,
    });
    println!("{payload}");
    Ok(())
}

fn run_hnsw_create(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client
        .graph()
        .ok_or_else(|| SqliteGraphError::invalid_input("hnsw-create requires SQLite backend"))?;

    // NOTE: HNSW indexes now persist to database for file-based databases.
    // Vectors inserted via hnsw-insert will be saved and restored on next CLI invocation.
    // For in-memory databases (--db memory), indexes remain in-memory only.

    // Parse HNSW configuration from command-line arguments
    let dimension = required_flag_value(args, "--dimension").and_then(|s| {
        s.parse::<usize>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid dimension: {e}")))
    })?;

    let m = required_flag_value(args, "--m").and_then(|s| {
        s.parse::<usize>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid m: {e}")))
    })?;

    let ef_construction = required_flag_value(args, "--ef-construction").and_then(|s| {
        s.parse::<usize>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid ef-construction: {e}")))
    })?;

    let distance_metric_str = required_flag_value(args, "--distance-metric")?;
    let distance_metric = match distance_metric_str.as_str() {
        "cosine" => DistanceMetric::Cosine,
        "euclidean" => DistanceMetric::Euclidean,
        "dot" | "dotproduct" => DistanceMetric::DotProduct,
        "manhattan" => DistanceMetric::Manhattan,
        _ => {
            return Err(SqliteGraphError::invalid_input(format!(
                "unsupported distance metric: {distance_metric_str}"
            )))
        }
    };

    // Get index name (default to "default" if not specified)
    let index_name = args
        .iter()
        .position(|arg| arg == "--index-name")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str())
        .unwrap_or("default");

    // Build HNSW configuration
    let config = HnswConfigBuilder::new()
        .dimension(dimension)
        .m_connections(m)
        .ef_construction(ef_construction)
        .ef_search(50) // Default ef_search
        .distance_metric(distance_metric)
        .build()
        .map_err(|e| SqliteGraphError::invalid_input(format!("invalid HNSW config: {e}")))?;

    // Create HNSW index with persistent storage
    let _hnsw = graph.hnsw_index_persistent(index_name, config)?;

    let payload = json!({
        "command": "hnsw-create",
        "index_name": index_name,
        "dimension": dimension,
        "m": m,
        "ef_construction": ef_construction,
        "distance_metric": distance_metric_str,
        "status": "created"
    });
    println!("{payload}");
    Ok(())
}

fn run_hnsw_insert(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client
        .graph()
        .ok_or_else(|| SqliteGraphError::invalid_input("hnsw-insert requires SQLite backend"))?;

    // NOTE: HNSW indexes now persist to database for file-based databases.
    // Vectors will be saved and available in subsequent CLI invocations.
    // For in-memory databases (--db memory), vectors remain in-memory only.

    // Get index name (default to "default" if not specified)
    let index_name = args
        .iter()
        .position(|arg| arg == "--name")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str())
        .unwrap_or("default");

    let input = required_flag_value(args, "--input")?;

    // Read JSON file with vectors
    let json_content = fs::read_to_string(&input)
        .map_err(|e| SqliteGraphError::invalid_input(format!("failed to read file: {e}")))?;

    let json_array: Vec<serde_json::Value> = serde_json::from_str(&json_content)
        .map_err(|e| SqliteGraphError::invalid_input(format!("failed to parse JSON array: {e}")))?;

    // Insert vectors into HNSW index
    let mut inserted_count = 0;
    let mut errors = Vec::new();

    for (idx, json_value) in json_array.iter().enumerate() {
        // Parse vector data
        let vector_array = json_value["vector"].as_array().ok_or_else(|| {
            SqliteGraphError::invalid_input(format!("vector {} missing 'vector' field", idx))
        })?;

        let vector_data: Vec<f32> = vector_array
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.as_f64()
                    .ok_or_else(|| {
                        SqliteGraphError::invalid_input(format!(
                            "vector element at index {} not a number",
                            i
                        ))
                    })
                    .map(|f| f as f32)
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Get metadata if present
        let metadata = json_value.get("metadata").cloned();

        // Insert vector
        let insert_result = graph.get_hnsw_index_mut(index_name, |hnsw| {
            hnsw.insert_vector(&vector_data, metadata)
        });

        match insert_result {
            Ok(_vector_id) => {
                inserted_count += 1;
            }
            Err(e) => {
                errors.push(format!("Vector {}: {}", idx, e));
            }
        }
    }

    let payload = json!({
        "command": "hnsw-insert",
        "index_name": index_name,
        "input": input,
        "vectors_processed": json_array.len(),
        "vectors_inserted": inserted_count,
        "errors": errors,
        "status": if errors.is_empty() { "completed" } else { "completed_with_errors" }
    });
    println!("{payload}");
    Ok(())
}

fn run_hnsw_search(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client
        .graph()
        .ok_or_else(|| SqliteGraphError::invalid_input("hnsw-search requires SQLite backend"))?;

    // NOTE: HNSW indexes now persist to database for file-based databases.
    // Searches will work across CLI invocations for persisted indexes.
    // For in-memory databases (--db memory), indexes remain in-memory only.

    // Get index name (default to "default" if not specified)
    let index_name = args
        .iter()
        .position(|arg| arg == "--name")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str())
        .unwrap_or("default");

    let input = required_flag_value(args, "--input")?;
    let k = required_flag_value(args, "--k").and_then(|s| {
        s.parse::<usize>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid k: {e}")))
    })?;

    // Read query vector from file
    let json_content = fs::read_to_string(&input)
        .map_err(|e| SqliteGraphError::invalid_input(format!("failed to read file: {e}")))?;

    let json_value: serde_json::Value = serde_json::from_str(&json_content)
        .map_err(|e| SqliteGraphError::invalid_input(format!("failed to parse JSON: {e}")))?;

    // Parse query vector
    let query_array = json_value["vector"]
        .as_array()
        .ok_or_else(|| SqliteGraphError::invalid_input("query missing 'vector' field"))?;

    let query_vector: Vec<f32> = query_array
        .iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_f64()
                .ok_or_else(|| {
                    SqliteGraphError::invalid_input(format!(
                        "query vector element at index {} not a number",
                        i
                    ))
                })
                .map(|f| f as f32)
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Perform search
    let search_result = graph.get_hnsw_index_ref(index_name, |hnsw| hnsw.search(&query_vector, k));

    match search_result {
        Ok(search_result) => match search_result {
            Ok(results) => {
                let results_json: Vec<_> = results
                    .iter()
                    .map(|(vector_id, distance)| {
                        json!({
                            "vector_id": vector_id,
                            "distance": distance
                        })
                    })
                    .collect();

                let payload = json!({
                    "command": "hnsw-search",
                    "index_name": index_name,
                    "k": k,
                    "results": results_json,
                    "found": results.len(),
                    "status": "completed"
                });
                println!("{payload}");
                Ok(())
            }
            Err(e) => {
                let payload = json!({
                    "command": "hnsw-search",
                    "index_name": index_name,
                    "error": e.to_string(),
                    "status": "error"
                });
                println!("{payload}");
                Ok(())
            }
        },
        Err(e) => {
            let payload = json!({
                "command": "hnsw-search",
                "index_name": index_name,
                "error": e.to_string(),
                "status": "error"
            });
            println!("{payload}");
            Ok(())
        }
    }
}

fn run_hnsw_stats(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client
        .graph()
        .ok_or_else(|| SqliteGraphError::invalid_input("hnsw-stats requires SQLite backend"))?;

    // NOTE: HNSW indexes now persist to database for file-based databases.
    // Statistics will show persisted indexes and their vectors across CLI invocations.
    // For in-memory databases (--db memory), indexes remain in-memory only.

    // Get index name (default to "default" if not specified)
    let index_name = args
        .iter()
        .position(|arg| arg == "--name")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str())
        .unwrap_or("default");

    // Get HNSW index statistics using read-only access
    let stats_result = graph.get_hnsw_index_ref(index_name, |hnsw| hnsw.statistics());

    match stats_result {
        Ok(stats_result) => match stats_result {
            Ok(stats) => {
                let payload = json!({
                    "command": "hnsw-stats",
                    "index_name": index_name,
                    "vector_count": stats.vector_count,
                    "layer_count": stats.layer_count,
                    "entry_point_count": stats.entry_point_count,
                    "dimension": stats.dimension,
                    "distance_metric": format!("{:?}", stats.distance_metric),
                    "storage_stats": {
                        "vector_count": stats.storage_stats.vector_count,
                        "total_dimensions": stats.storage_stats.total_dimensions,
                        "average_dimension": stats.storage_stats.average_dimension,
                        "estimated_memory_bytes": stats.storage_stats.estimated_memory_bytes,
                        "backend_type": stats.storage_stats.backend_type,
                    },
                    "layer_stats": stats.layer_stats.iter()
                        .map(|(layer_id, node_count, avg_conn)| json!({
                            "layer": layer_id,
                            "node_count": node_count,
                            "avg_connections": avg_conn
                        }))
                        .collect::<Vec<_>>()
                });
                println!("{payload}");
                Ok(())
            }
            Err(e) => {
                let payload = json!({
                    "command": "hnsw-stats",
                    "index_name": index_name,
                    "error": e.to_string(),
                    "status": "error"
                });
                println!("{payload}");
                Ok(())
            }
        },
        Err(e) => {
            let payload = json!({
                "command": "hnsw-stats",
                "index_name": index_name,
                "error": e.to_string(),
                "status": "error"
            });
            println!("{payload}");
            Ok(())
        }
    }
}

fn run_hnsw_list(client: &BackendClient, _args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client
        .graph()
        .ok_or_else(|| SqliteGraphError::invalid_input("hnsw-list requires SQLite backend"))?;

    // Get list of index names from in-memory registry (loaded on startup)
    let index_names = graph.list_hnsw_indexes()?;

    // Build response with index names
    let payload = json!({
        "command": "hnsw-list",
        "count": index_names.len(),
        "indexes": index_names,
        "status": "completed"
    });
    println!("{payload}");
    Ok(())
}

fn run_hnsw_delete(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client
        .graph()
        .ok_or_else(|| SqliteGraphError::invalid_input("hnsw-delete requires SQLite backend"))?;

    // Get index name from --index-name or --name parameter
    let index_name = args
        .iter()
        .position(|arg| arg == "--index-name" || arg == "--name")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str())
        .ok_or_else(|| {
            SqliteGraphError::invalid_input("--index-name is required for hnsw-delete")
        })?;

    // Check if index exists
    let exists = graph.list_hnsw_indexes()?.iter().any(|n| n == index_name);
    if !exists {
        let payload = json!({
            "command": "hnsw-delete",
            "index_name": index_name,
            "error": "Index not found",
            "status": "error"
        });
        println!("{payload}");
        return Ok(());
    }

    // Delete from database (CASCADE handles vectors)
    use sqlitegraph::hnsw::HnswIndex;
    let conn = graph
        .pool
        .get()
        .map_err(|e| SqliteGraphError::invalid_input(format!("Failed to get connection: {}", e)))?;
    HnswIndex::delete_index(&conn, index_name)
        .map_err(|e| SqliteGraphError::invalid_input(format!("Failed to delete index: {}", e)))?;

    // Remove from in-memory registry
    {
        let mut indexes = graph
            .hnsw_indexes
            .write()
            .map_err(|e| SqliteGraphError::invalid_input(format!("RwLock poisoned: {}", e)))?;
        indexes.remove(index_name);
    }

    let payload = json!({
        "command": "hnsw-delete",
        "index_name": index_name,
        "deleted": true,
        "status": "completed"
    });
    println!("{payload}");
    Ok(())
}

fn run_hnsw_info(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client
        .graph()
        .ok_or_else(|| SqliteGraphError::invalid_input("hnsw-info requires SQLite backend"))?;

    // Get index name (default to "default" if not specified)
    let index_name = args
        .iter()
        .position(|arg| arg == "--index-name" || arg == "--name")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str())
        .unwrap_or("default");

    // Check if index exists
    let exists = graph.list_hnsw_indexes()?.iter().any(|n| n == index_name);
    if !exists {
        let payload = json!({
            "command": "hnsw-info",
            "index_name": index_name,
            "error": "Index not found",
            "status": "error"
        });
        println!("{payload}");
        return Ok(());
    }

    // Get detailed statistics from the index
    let stats_result = graph.get_hnsw_index_ref(index_name, |hnsw| hnsw.statistics());

    let payload = match stats_result {
        Ok(Ok(stats)) => json!({
            "command": "hnsw-info",
            "index_name": index_name,
            "vector_count": stats.vector_count,
            "layer_count": stats.layer_count,
            "entry_point_count": stats.entry_point_count,
            "dimension": stats.dimension,
            "distance_metric": format!("{:?}", stats.distance_metric),
            "storage": {
                "backend_type": stats.storage_stats.backend_type,
                "estimated_memory_bytes": stats.storage_stats.estimated_memory_bytes
            },
            "layers": stats.layer_stats.iter()
                .map(|(layer, nodes, conn)| json!({
                    "layer": layer,
                    "node_count": nodes,
                    "avg_connections": conn
                }))
                .collect::<Vec<_>>(),
            "status": "completed"
        }),
        Ok(Err(e)) => json!({
            "command": "hnsw-info",
            "index_name": index_name,
            "error": e.to_string(),
            "status": "error"
        }),
        Err(e) => json!({
            "command": "hnsw-info",
            "index_name": index_name,
            "error": e.to_string(),
            "status": "error"
        }),
    };

    println!("{payload}");
    Ok(())
}

fn run_bfs(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client
        .graph()
        .ok_or_else(|| SqliteGraphError::invalid_input("bfs command requires SQLite backend"))?;

    let start = required_flag_value(args, "--start").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid start node: {e}")))
    })?;

    let max_depth = required_flag_value(args, "--max-depth").and_then(|s| {
        s.parse::<u32>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid max-depth: {e}")))
    })?;

    // Add progress reporting
    let progress = ConsoleProgress::new();
    eprintln!("BFS: starting from node {}", start);

    let visited = bfs_neighbors(graph, start, max_depth)?;

    progress.on_progress(visited.len(), Some(visited.len()), "BFS: visited nodes");
    progress.on_complete();

    let payload = json!({
        "command": "bfs",
        "start": start,
        "max_depth": max_depth,
        "visited_count": visited.len(),
        "visited_nodes": visited
    });
    println!("{payload}");
    Ok(())
}

fn run_k_hop(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client
        .graph()
        .ok_or_else(|| SqliteGraphError::invalid_input("k-hop command requires SQLite backend"))?;

    let start = required_flag_value(args, "--start").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid start node: {e}")))
    })?;

    let depth = required_flag_value(args, "--depth").and_then(|s| {
        s.parse::<u32>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid depth: {e}")))
    })?;

    let direction_str =
        required_flag_value(args, "--direction").unwrap_or_else(|_| "outgoing".to_string());
    let direction = match direction_str.as_str() {
        "incoming" => BackendDirection::Incoming,
        "outgoing" | _ => BackendDirection::Outgoing,
    };

    // Add progress reporting
    let progress = ConsoleProgress::new();
    eprintln!("K-hop: processing depth {}", depth);

    let neighbors = k_hop(graph, start, depth, direction)?;

    progress.on_progress(
        neighbors.len(),
        Some(neighbors.len()),
        "K-hop: neighbors found",
    );
    progress.on_complete();

    let payload = json!({
        "command": "k-hop",
        "start": start,
        "depth": depth,
        "direction": direction_str,
        "neighbor_count": neighbors.len(),
        "neighbors": neighbors
    });
    println!("{payload}");
    Ok(())
}

fn run_shortest_path(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("shortest-path command requires SQLite backend")
    })?;

    let start = required_flag_value(args, "--from").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid start node: {e}")))
    })?;

    let end = required_flag_value(args, "--to").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid end node: {e}")))
    })?;

    // Add progress reporting
    let progress = ConsoleProgress::new();
    eprintln!("Shortest path: searching from {} to {}", start, end);

    let path = shortest_path(graph, start, end)?;

    let visited_count = path.as_ref().map(|p| p.len()).unwrap_or(0);
    progress.on_progress(
        visited_count,
        Some(visited_count),
        "Shortest path: nodes visited",
    );
    progress.on_complete();

    let payload = json!({
        "command": "shortest-path",
        "from": start,
        "to": end,
        "path_exists": path.is_some(),
        "path": path
    });
    println!("{payload}");
    Ok(())
}

fn run_neighbors(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("neighbors command requires SQLite backend")
    })?;

    let id = required_flag_value(args, "--id").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid node id: {e}")))
    })?;

    let direction_str =
        required_flag_value(args, "--direction").unwrap_or_else(|_| "outgoing".to_string());
    let query = graph.query();

    let neighbors = match direction_str.as_str() {
        "incoming" => query.incoming(id)?,
        "outgoing" | _ => query.outgoing(id)?,
    };

    let payload = json!({
        "command": "neighbors",
        "id": id,
        "direction": direction_str,
        "neighbor_count": neighbors.len(),
        "neighbors": neighbors
    });
    println!("{payload}");
    Ok(())
}

fn run_pattern_match(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("pattern-match command requires SQLite backend")
    })?;

    // Parse required --edge-type parameter
    let edge_type = required_flag_value(args, "--edge-type")?;

    // Parse optional parameters
    let start_label = optional_flag_value(args, "--start-label");
    let end_label = optional_flag_value(args, "--end-label");
    let direction_str =
        optional_flag_value(args, "--direction").unwrap_or_else(|| "outgoing".to_string());

    // Parse property filters (format: --start-prop key:value --end-prop key:value)
    let mut start_props = std::collections::HashMap::new();
    let mut end_props = std::collections::HashMap::new();

    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--start-prop" {
            if let Some(prop_value) = iter.next() {
                if let Some((key, value)) = prop_value.split_once(':') {
                    start_props.insert(key.to_string(), value.to_string());
                }
            }
        } else if arg == "--end-prop" {
            if let Some(prop_value) = iter.next() {
                if let Some((key, value)) = prop_value.split_once(':') {
                    end_props.insert(key.to_string(), value.to_string());
                }
            }
        }
    }

    // Build pattern triple
    let direction = match direction_str.as_str() {
        "incoming" => BackendDirection::Incoming,
        "outgoing" | _ => BackendDirection::Outgoing,
    };

    let mut pattern = PatternTriple::new(&edge_type).direction(direction);

    if let Some(ref label) = start_label {
        pattern = pattern.start_label(label);
    }
    if let Some(ref label) = end_label {
        pattern = pattern.end_label(label);
    }

    // Add property filters
    for (key, value) in start_props {
        pattern = pattern.start_property(key, value);
    }
    for (key, value) in end_props {
        pattern = pattern.end_property(key, value);
    }

    // Execute pattern match
    let matches = graph.match_triples(&pattern)?;

    // Convert TripleMatch to serializable format
    let matches_json: Vec<serde_json::Value> = matches
        .into_iter()
        .map(|m| {
            json!({
                "start_id": m.start_id,
                "end_id": m.end_id,
                "edge_id": m.edge_id
            })
        })
        .collect();

    let payload = json!({
        "command": "pattern-match",
        "edge_type": edge_type,
        "start_label": start_label,
        "end_label": end_label,
        "direction": direction_str,
        "match_count": matches_json.len(),
        "matches": matches_json
    });
    println!("{payload}");
    Ok(())
}

fn run_pattern_match_fast(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("pattern-match-fast command requires SQLite backend")
    })?;

    // Parse required --edge-type parameter
    let edge_type = required_flag_value(args, "--edge-type")?;

    // Parse optional parameters
    let start_label = optional_flag_value(args, "--start-label");
    let end_label = optional_flag_value(args, "--end-label");
    let direction_str =
        optional_flag_value(args, "--direction").unwrap_or_else(|| "outgoing".to_string());

    // Parse property filters (format: --start-prop key:value --end-prop key:value)
    let mut start_props = std::collections::HashMap::new();
    let mut end_props = std::collections::HashMap::new();

    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--start-prop" {
            if let Some(prop_value) = iter.next() {
                if let Some((key, value)) = prop_value.split_once(':') {
                    start_props.insert(key.to_string(), value.to_string());
                }
            }
        } else if arg == "--end-prop" {
            if let Some(prop_value) = iter.next() {
                if let Some((key, value)) = prop_value.split_once(':') {
                    end_props.insert(key.to_string(), value.to_string());
                }
            }
        }
    }

    // Build pattern triple
    let direction = match direction_str.as_str() {
        "incoming" => BackendDirection::Incoming,
        "outgoing" | _ => BackendDirection::Outgoing,
    };

    let mut pattern = PatternTriple::new(&edge_type).direction(direction);

    if let Some(ref label) = start_label {
        pattern = pattern.start_label(label);
    }
    if let Some(ref label) = end_label {
        pattern = pattern.end_label(label);
    }

    // Add property filters
    for (key, value) in start_props {
        pattern = pattern.start_property(key, value);
    }
    for (key, value) in end_props {
        pattern = pattern.end_property(key, value);
    }

    // Execute fast-path pattern match
    let matches = graph.match_triples_fast(&pattern)?;

    // Convert TripleMatch to serializable format
    let matches_json: Vec<serde_json::Value> = matches
        .into_iter()
        .map(|m| {
            json!({
                "start_id": m.start_id,
                "end_id": m.end_id,
                "edge_id": m.edge_id
            })
        })
        .collect();

    let payload = json!({
        "command": "pattern-match-fast",
        "edge_type": edge_type,
        "start_label": start_label,
        "end_label": end_label,
        "direction": direction_str,
        "match_count": matches_json.len(),
        "matches": matches_json
    });
    println!("{payload}");
    Ok(())
}

/// Helper function to get optional flag value
fn optional_flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().cloned();
        }
    }
    None
}

fn run_wal_checkpoint(client: &BackendClient, _args: &[String]) -> Result<(), SqliteGraphError> {
    let backend = client.backend();

    backend.checkpoint()?;

    let payload = json!({
        "command": "wal-checkpoint",
        "status": "completed"
    });
    println!("{payload}");
    Ok(())
}

fn run_snapshot_create(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let backend = client.backend();

    let dir_str = required_flag_value(args, "--dir")?;
    let export_dir = PathBuf::from(&dir_str);

    let metadata = backend.snapshot_export(&export_dir)?;

    let payload = json!({
        "command": "snapshot-create",
        "snapshot_path": metadata.snapshot_path,
        "size_bytes": metadata.size_bytes,
        "entity_count": metadata.entity_count,
        "edge_count": metadata.edge_count,
        "status": "completed"
    });
    println!("{payload}");
    Ok(())
}

fn run_snapshot_load(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let backend = client.backend();

    let dir_str = required_flag_value(args, "--dir")?;
    let import_dir = PathBuf::from(&dir_str);

    let metadata = backend.snapshot_import(&import_dir)?;

    let payload = json!({
        "command": "snapshot-load",
        "snapshot_path": metadata.snapshot_path,
        "entities_imported": metadata.entities_imported,
        "edges_imported": metadata.edges_imported,
        "status": "completed"
    });
    println!("{payload}");
    Ok(())
}

#[cfg(feature = "native-v2")]
fn run_wal_metrics(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    use std::fs;

    // Get database path from args or use a default
    let db_path_str = args
        .iter()
        .position(|arg| arg == "--db")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str())
        .unwrap_or("/tmp/test.db"); // Default for testing

    let db_path = std::path::Path::new(db_path_str);
    let wal_path = db_path.with_extension("wal");
    let checkpoint_path = db_path.with_extension("checkpoint");

    let mut metrics = serde_json::json!({
        "command": "wal-metrics",
        "database_path": db_path_str,
        "wal_file": wal_path.display().to_string(),
        "checkpoint_file": checkpoint_path.display().to_string(),
    });

    // Check if WAL file exists and get its size
    if wal_path.exists() {
        if let Ok(metadata) = fs::metadata(&wal_path) {
            metrics["wal_size_bytes"] = json!(metadata.len());
            metrics["wal_size_mb"] = json!(metadata.len() as f64 / 1_048_576.0);
            metrics["wal_exists"] = json!(true);
        }
    } else {
        metrics["wal_exists"] = json!(false);
    }

    // Check checkpoint file
    if checkpoint_path.exists() {
        if let Ok(metadata) = fs::metadata(&checkpoint_path) {
            metrics["checkpoint_size_bytes"] = json!(metadata.len());
        }
    }

    // Get WAL manager metrics if available (only for Native backend)
    if let Some(wal_metrics) = client.get_wal_metrics() {
        metrics["total_transactions"] = json!(wal_metrics.total_transactions);
        metrics["committed_transactions"] = json!(wal_metrics.committed_transactions);
        metrics["rolled_back_transactions"] = json!(wal_metrics.rolled_back_transactions);
        metrics["avg_transaction_duration_us"] = json!(wal_metrics.avg_transaction_duration_us);
        metrics["total_records_written"] = json!(wal_metrics.total_records_written);
        metrics["checkpoint_count"] = json!(wal_metrics.checkpoint_count);
        metrics["recovery_count"] = json!(wal_metrics.recovery_count);
        metrics["group_commit_batches"] = json!(wal_metrics.group_commit_batches);
        metrics["avg_group_commit_size"] = json!(wal_metrics.avg_group_commit_size);
        metrics["compression_ratio"] = json!(wal_metrics.compression_ratio);

        // Get active transaction count
        if let Some(active_count) = client.get_active_transaction_count() {
            metrics["active_transactions"] = json!(active_count);
        }
    } else {
        metrics["note"] =
            json!("WAL metrics not available - may not be a Native backend or WAL not initialized");
    }

    println!("{metrics}");
    Ok(())
}

#[cfg(not(feature = "native-v2"))]
fn run_wal_metrics(_client: &BackendClient, _args: &[String]) -> Result<(), SqliteGraphError> {
    let payload = json!({
        "command": "wal-metrics",
        "error": "WAL metrics require native-v2 feature",
        "status": "unsupported"
    });
    println!("{payload}");
    Ok(())
}

#[cfg(feature = "native-v2")]
fn run_wal_config(_client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    use sqlitegraph::V2WALConfig;

    // Get database path from args or use a default
    let db_path_str = args
        .iter()
        .position(|arg| arg == "--db")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str())
        .unwrap_or("/tmp/test.db"); // Default for testing

    let db_path = std::path::Path::new(db_path_str);
    let config = V2WALConfig::for_graph_file(db_path);

    let payload = json!({
        "command": "wal-config",
        "database_path": db_path_str,
        "graph_path": config.graph_path.display().to_string(),
        "wal_path": config.wal_path.display().to_string(),
        "checkpoint_path": config.checkpoint_path.display().to_string(),
        "max_wal_size": config.max_wal_size,
        "max_wal_size_mb": config.max_wal_size / 1_048_576,
        "buffer_size": config.buffer_size,
        "buffer_size_kb": config.buffer_size / 1024,
        "checkpoint_interval": config.checkpoint_interval,
        "group_commit_timeout_ms": config.group_commit_timeout_ms,
        "max_group_commit_size": config.max_group_commit_size,
        "enable_compression": config.enable_compression,
        "compression_level": config.compression_level,
    });
    println!("{payload}");
    Ok(())
}

#[cfg(not(feature = "native-v2"))]
fn run_wal_config(_client: &BackendClient, _args: &[String]) -> Result<(), SqliteGraphError> {
    let payload = json!({
        "command": "wal-config",
        "error": "WAL config requires native-v2 feature",
        "status": "unsupported"
    });
    println!("{payload}");
    Ok(())
}

#[cfg(feature = "native-v2")]
fn run_wal_stats(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    use std::fs;

    // Get database path from args or use a default
    let db_path_str = args
        .iter()
        .position(|arg| arg == "--db")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str())
        .unwrap_or("/tmp/test.db");

    let db_path = std::path::Path::new(db_path_str);
    let wal_path = db_path.with_extension("wal");
    let checkpoint_path = db_path.with_extension("checkpoint");

    // Check if backend is Native and has WAL metrics
    let backend_type = client.backend_type();

    if backend_type != "native" {
        let payload = json!({
            "command": "wal-stats",
            "error": format!("wal-stats requires Native backend, got: {}", backend_type),
            "status": "unsupported"
        });
        println!("{payload}");
        return Ok(());
    }

    // Get WAL file info
    let wal_exists = wal_path.exists();
    let wal_size = if wal_exists {
        fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let checkpoint_exists = checkpoint_path.exists();
    let checkpoint_size = if checkpoint_exists {
        fs::metadata(&checkpoint_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    // Get WAL manager metrics
    let (metrics, active_count) = match client.get_wal_metrics() {
        Some(m) => (m, client.get_active_transaction_count().unwrap_or(0)),
        None => {
            let payload = json!({
                "command": "wal-stats",
                "error": "WAL metrics not available - WAL may not be initialized",
                "status": "unavailable"
            });
            println!("{payload}");
            return Ok(());
        }
    };

    // Calculate derived statistics
    let tx_success_rate = if metrics.total_transactions > 0 {
        (metrics.committed_transactions as f64 / metrics.total_transactions as f64) * 100.0
    } else {
        0.0
    };

    let tx_failure_rate = if metrics.total_transactions > 0 {
        (metrics.rolled_back_transactions as f64 / metrics.total_transactions as f64) * 100.0
    } else {
        0.0
    };

    let avg_records_per_tx = if metrics.committed_transactions > 0 {
        metrics.total_records_written as f64 / metrics.committed_transactions as f64
    } else {
        0.0
    };

    let avg_tx_duration_ms = if metrics.committed_transactions > 0 {
        metrics.avg_transaction_duration_us as f64 / 1000.0
    } else {
        0.0
    };

    // Build stats response
    let stats = json!({
        "command": "wal-stats",
        "backend": backend_type,
        "wal_file": wal_path.display().to_string(),
        "checkpoint_file": checkpoint_path.display().to_string(),

        // File Status
        "wal_status": {
            "exists": wal_exists,
            "size_bytes": wal_size,
            "size_mb": wal_size as f64 / 1_048_576.0
        },
        "checkpoint_status": {
            "exists": checkpoint_exists,
            "size_bytes": checkpoint_size,
            "size_mb": checkpoint_size as f64 / 1_048_576.0
        },

        // Transaction Statistics
        "transaction_stats": {
            "total": metrics.total_transactions,
            "committed": metrics.committed_transactions,
            "rolled_back": metrics.rolled_back_transactions,
            "active": active_count,
            "success_rate_percent": tx_success_rate,
            "failure_rate_percent": tx_failure_rate
        },

        // Performance Metrics
        "performance": {
            "avg_duration_ms": avg_tx_duration_ms,
            "avg_records_per_tx": avg_records_per_tx,
            "total_records_written": metrics.total_records_written,
            "throughput_tx_per_sec": if metrics.avg_transaction_duration_us > 0 {
                1_000_000.0 / metrics.avg_transaction_duration_us as f64
            } else {
                0.0
            }
        },

        // Checkpoint & Recovery
        "maintenance": {
            "checkpoint_count": metrics.checkpoint_count,
            "recovery_count": metrics.recovery_count,
            "requires_checkpoint": wal_size > (1024 * 1024 * 1024) // 1GB threshold
        },

        // Group Commit Statistics
        "group_commit": {
            "batches": metrics.group_commit_batches,
            "avg_batch_size": metrics.avg_group_commit_size,
            "total_transactions_grouped": if metrics.avg_group_commit_size > 0.0 && metrics.group_commit_batches > 0 {
                (metrics.avg_group_commit_size * metrics.group_commit_batches as f64) as u64
            } else {
                0
            }
        },

        // Compression
        "compression": {
            "enabled": metrics.compression_ratio < 1.0,
            "ratio": metrics.compression_ratio
        }
    });

    println!("{stats}");
    Ok(())
}

#[cfg(not(feature = "native-v2"))]
fn run_wal_stats(_client: &BackendClient, _args: &[String]) -> Result<(), SqliteGraphError> {
    let payload = json!({
        "command": "wal-stats",
        "error": "WAL stats require native-v2 feature",
        "status": "unsupported"
    });
    println!("{payload}");
    Ok(())
}

// Reindex functions removed - not available in v0.2.5

fn run_debug_stats(client: &BackendClient, _args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("debug-stats command requires SQLite backend")
    })?;

    // Get introspection data
    let intro = graph.introspect()?;

    // Convert to JSON
    let json = serde_json::to_string_pretty(&intro).map_err(|e| {
        SqliteGraphError::invalid_input(format!("failed to serialize introspection: {e}"))
    })?;

    println!("{json}");
    Ok(())
}

fn run_debug_dump(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("debug-dump command requires SQLite backend")
    })?;

    let output = required_flag_value(args, "--output")?;

    // Check format flag (default: jsonl)
    let format_str = args
        .iter()
        .position(|arg| arg == "--format")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str())
        .unwrap_or("jsonl");

    // Validate format
    if format_str != "jsonl" && format_str != "json" {
        return Err(SqliteGraphError::invalid_input(format!(
            "invalid format: {format_str} (must be jsonl or json)"
        )));
    }

    // Get all entities
    let entity_ids = client
        .entity_ids()?
        .ok_or_else(|| SqliteGraphError::invalid_input("failed to get entity IDs"))?;

    // Open output file
    use std::io::BufWriter;
    let file = std::fs::File::create(&output).map_err(|e| {
        SqliteGraphError::invalid_input(format!("failed to create output file: {e}"))
    })?;
    let mut writer = BufWriter::new(file);

    // Determine if we should use JSONL or JSON array format
    let use_json_array = format_str == "json" && entity_ids.len() < 1000;

    if use_json_array {
        // JSON array format for small graphs
        let mut entities = Vec::new();
        for id in &entity_ids {
            let entity = graph.get_entity(*id)?;
            entities.push(json!({
                "type": "node",
                "id": entity.id,
                "kind": entity.kind,
                "name": entity.name,
                "file_path": entity.file_path,
                "data": entity.data
            }));
        }

        // Get edges
        let query = graph.query();
        for id in &entity_ids {
            if let Ok(outgoing) = query.outgoing(*id) {
                for edge_id in outgoing {
                    if let Ok(edge) = graph.get_edge(edge_id) {
                        entities.push(json!({
                            "type": "edge",
                            "id": edge.id,
                            "from": edge.from_id,
                            "to": edge.to_id,
                            "edge_type": edge.edge_type,
                            "data": edge.data
                        }));
                    }
                }
            }
        }

        // Write as JSON array
        let json_output = serde_json::to_string_pretty(&entities).map_err(|e| {
            SqliteGraphError::invalid_input(format!("failed to serialize graph: {e}"))
        })?;
        use std::io::Write;
        write!(writer, "{}", json_output)
            .map_err(|e| SqliteGraphError::invalid_input(format!("failed to write output: {e}")))?;
    } else {
        // JSONL format (streaming, memory efficient)
        use std::io::Write;

        for id in &entity_ids {
            let entity = graph.get_entity(*id)?;
            let json_line = json!({
                "type": "node",
                "id": entity.id,
                "kind": entity.kind,
                "name": entity.name,
                "file_path": entity.file_path,
                "data": entity.data
            });
            let line = serde_json::to_string(&json_line).map_err(|e| {
                SqliteGraphError::invalid_input(format!("failed to serialize entity: {e}"))
            })?;
            writeln!(writer, "{}", line).map_err(|e| {
                SqliteGraphError::invalid_input(format!("failed to write entity: {e}"))
            })?;
        }

        // Get edges
        let query = graph.query();
        for id in &entity_ids {
            if let Ok(outgoing) = query.outgoing(*id) {
                for edge_id in outgoing {
                    if let Ok(edge) = graph.get_edge(edge_id) {
                        let json_line = json!({
                            "type": "edge",
                            "id": edge.id,
                            "from": edge.from_id,
                            "to": edge.to_id,
                            "edge_type": edge.edge_type,
                            "data": edge.data
                        });
                        let line = serde_json::to_string(&json_line).map_err(|e| {
                            SqliteGraphError::invalid_input(format!(
                                "failed to serialize edge: {e}"
                            ))
                        })?;
                        writeln!(writer, "{}", line).map_err(|e| {
                            SqliteGraphError::invalid_input(format!("failed to write edge: {e}"))
                        })?;
                    }
                }
            }
        }
    }

    // Flush the buffer
    use std::io::Write;
    writer
        .flush()
        .map_err(|e| SqliteGraphError::invalid_input(format!("failed to flush output: {e}")))?;

    let payload = json!({
        "command": "debug-dump",
        "output": output,
        "format": format_str,
        "entities_dumped": entity_ids.len(),
        "status": "completed"
    });
    println!("{payload}");
    Ok(())
}

fn run_debug_trace(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    // Parse the command to trace
    if args.is_empty() {
        return Err(SqliteGraphError::invalid_input(
            "debug-trace requires a command to trace",
        ));
    }

    let trace_command = &args[0];
    let trace_args = &args[1..];

    // Enable trace logging for the duration of the command
    // Note: This requires the env_logger or similar logging to be configured
    // For now, we'll set RUST_LOG environment variable and re-run the command
    eprintln!(
        "debug-trace: enabling trace logging for command: {}",
        trace_command
    );

    // Set RUST_LOG for this session
    std::env::set_var("RUST_LOG", "debug");

    // Re-run the command with trace logging enabled
    match run_command(client, trace_command, trace_args) {
        Ok(_) => {
            let payload = json!({
                "command": "debug-trace",
                "traced_command": trace_command,
                "status": "completed"
            });
            println!("{payload}");
            Ok(())
        }
        Err(e) => {
            let payload = json!({
                "command": "debug-trace",
                "traced_command": trace_command,
                "error": e.to_string(),
                "status": "error"
            });
            println!("{payload}");
            Err(e)
        }
    }
}

fn run_pagerank(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("pagerank command requires SQLite backend")
    })?;

    let iterations = required_flag_value(args, "--iterations").and_then(|s| {
        s.parse::<usize>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid iterations: {e}")))
    })?;

    let damping_factor = optional_flag_value(args, "--damping-factor")
        .map(|s| {
            s.parse::<f64>().map_err(|e| {
                SqliteGraphError::invalid_input(format!("invalid damping-factor: {e}"))
            })
        })
        .transpose()?
        .unwrap_or(0.85);

    // Use ConsoleProgress for progress reporting
    let progress = ConsoleProgress::new();

    let scores = pagerank_with_progress(graph, damping_factor, iterations, &progress)?;

    let payload = json!({
        "command": "pagerank",
        "iterations": iterations,
        "damping_factor": damping_factor,
        "node_count": scores.len(),
        "top_scores": scores.iter().take(10).map(|(node_id, score)| json!({
            "node_id": node_id,
            "score": score
        })).collect::<Vec<_>>()
    });
    println!("{payload}");
    Ok(())
}

fn run_betweenness(client: &BackendClient, _args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("betweenness command requires SQLite backend")
    })?;

    // Use ConsoleProgress for progress reporting
    let progress = ConsoleProgress::new();

    let centrality = betweenness_centrality_with_progress(graph, &progress)?;

    let payload = json!({
        "command": "betweenness",
        "node_count": centrality.len(),
        "top_centrality": centrality.iter().take(10).map(|(node_id, score)| json!({
            "node_id": node_id,
            "centrality": score
        })).collect::<Vec<_>>()
    });
    println!("{payload}");
    Ok(())
}

fn run_louvain(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("louvain command requires SQLite backend")
    })?;

    let max_iterations = optional_flag_value(args, "--max-iterations")
        .map(|s| {
            s.parse::<usize>().map_err(|e| {
                SqliteGraphError::invalid_input(format!("invalid max-iterations: {e}"))
            })
        })
        .transpose()?
        .unwrap_or(100);

    // Use ConsoleProgress for progress reporting
    let progress = ConsoleProgress::new();

    let communities = louvain_communities_with_progress(graph, max_iterations, &progress)?;

    let payload = json!({
        "command": "louvain",
        "max_iterations": max_iterations,
        "community_count": communities.len(),
        "communities": communities.iter().take(10).map(|members| json!({
            "members": members
        })).collect::<Vec<_>>()
    });
    println!("{payload}");
    Ok(())
}

fn run_backward_slice(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("backward-slice command requires SQLite backend")
    })?;

    let target = required_flag_value(args, "--target").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid target node: {e}")))
    })?;

    // Compute control dependence graph first (required for slicing)
    let cdg = sqlitegraph::algo::control_dependence_from_exit(graph)?;

    let progress = ConsoleProgress::new();
    let result = backward_slice_with_progress(graph, &cdg, target, &progress)?;

    let payload = json!({
        "command": "backward-slice",
        "target": target,
        "control_nodes": result.control_nodes.len(),
        "data_nodes": result.data_nodes.len(),
        "slice_nodes": result.sorted_nodes()
    });
    println!("{payload}");
    Ok(())
}

fn run_forward_slice(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("forward-slice command requires SQLite backend")
    })?;

    let source = required_flag_value(args, "--source").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid source node: {e}")))
    })?;

    // Compute control dependence graph first (required for slicing)
    let cdg = sqlitegraph::algo::control_dependence_from_exit(graph)?;

    let progress = ConsoleProgress::new();
    let result = forward_slice_with_progress(graph, &cdg, source, &progress)?;

    let payload = json!({
        "command": "forward-slice",
        "source": source,
        "control_nodes": result.control_nodes.len(),
        "data_nodes": result.data_nodes.len(),
        "slice_nodes": result.sorted_nodes()
    });
    println!("{payload}");
    Ok(())
}

fn run_dominators(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("dominators command requires SQLite backend")
    })?;

    let entry = required_flag_value(args, "--entry").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid entry node: {e}")))
    })?;

    let progress = ConsoleProgress::new();
    let result = dominators_with_progress(graph, entry, &progress)?;

    let payload = json!({
        "command": "dominators",
        "entry": entry,
        "immediate_dominator": result.immediate_dominator,
        "dominator_sets": result.dominators
    });
    println!("{payload}");
    Ok(())
}

fn run_post_dominators(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("post-dominators command requires SQLite backend")
    })?;

    let exit = optional_flag_value(args, "--exit")
        .map(|s| {
            s.parse::<i64>().map_err(|e| {
                SqliteGraphError::invalid_input(format!("invalid exit node: {e}"))
            })
        })
        .transpose()?;

    let progress = ConsoleProgress::new();
    let result = if let Some(exit_node) = exit {
        post_dominators_with_progress(graph, exit_node, &progress)?
    } else {
        post_dominators_auto_exit(graph)?
    };

    let payload = json!({
        "command": "post-dominators",
        "exit": exit,
        "immediate_post_dominator": result.immediate_post_dominator,
        "post_dominator_sets": result.post_dominators
    });
    println!("{payload}");
    Ok(())
}

fn run_control_dependence(
    client: &BackendClient,
    args: &[String],
) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("control-dependence command requires SQLite backend")
    })?;

    let exit = optional_flag_value(args, "--exit")
        .map(|s| {
            s.parse::<i64>().map_err(|e| {
                SqliteGraphError::invalid_input(format!("invalid exit node: {e}"))
            })
        })
        .transpose()?;

    let result = if let Some(exit_node) = exit {
        control_dependence_graph(graph, exit_node)?
    } else {
        control_dependence_from_exit(graph)?
    };

    let payload = json!({
        "command": "control-dependence",
        "exit": exit,
        "edge_count": result.edges.len(),
        "edges": result.edges
    });
    println!("{payload}");
    Ok(())
}

fn run_dominance_frontiers(
    client: &BackendClient,
    args: &[String],
) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("dominance-frontiers command requires SQLite backend")
    })?;

    let entry = required_flag_value(args, "--entry").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid entry node: {e}")))
    })?;

    let progress = ConsoleProgress::new();
    let frontiers = dominance_frontiers_with_progress(graph, entry, &progress)?;

    let payload = json!({
        "command": "dominance-frontiers",
        "entry": entry,
        "frontier_sets": frontiers
    });
    println!("{payload}");
    Ok(())
}

fn run_natural_loops(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("natural-loops command requires SQLite backend")
    })?;

    let entry = required_flag_value(args, "--entry").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid entry node: {e}")))
    })?;

    let progress = ConsoleProgress::new();
    let dom_result = dominators_with_progress(graph, entry, &progress)?;
    let result = natural_loops_with_progress(graph, &dom_result, &progress)?;

    let payload = json!({
        "command": "natural-loops",
        "entry": entry,
        "loop_count": result.loops.len(),
        "loops": result.loops.iter().map(|(header, loop_)| json!({
            "header": header,
            "back_edges": loop_.back_edges,
            "body_size": loop_.body.len()
        })).collect::<Vec<_>>()
    });
    println!("{payload}");
    Ok(())
}

fn run_forward_reachability(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("forward-reachability command requires SQLite backend")
    })?;

    let start = required_flag_value(args, "--start").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid start node: {e}")))
    })?;

    let progress = ConsoleProgress::new();
    let reachable = reachable_from_with_progress(graph, start, &progress)?;

    // Convert HashSet to Vec for JSON serialization
    let reachable_vec: Vec<i64> = reachable.into_iter().collect();

    let payload = json!({
        "command": "forward-reachability",
        "start": start,
        "reachable_count": reachable_vec.len(),
        "reachable_nodes": reachable_vec
    });
    println!("{payload}");
    Ok(())
}

fn run_backward_reachability(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("backward-reachability command requires SQLite backend")
    })?;

    let target = required_flag_value(args, "--target").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid target node: {e}")))
    })?;

    let progress = ConsoleProgress::new();
    let reachable = reverse_reachable_from_with_progress(graph, target, &progress)?;

    // Convert HashSet to Vec for JSON serialization
    let reachable_vec: Vec<i64> = reachable.into_iter().collect();

    let payload = json!({
        "command": "backward-reachability",
        "target": target,
        "reachable_count": reachable_vec.len(),
        "reachable_nodes": reachable_vec
    });
    println!("{payload}");
    Ok(())
}

fn run_can_reach(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("can-reach command requires SQLite backend")
    })?;

    let from = required_flag_value(args, "--from").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid from node: {e}")))
    })?;

    let to = required_flag_value(args, "--to").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid to node: {e}")))
    })?;

    let can_reach_result = can_reach(graph, from, to)?;

    let payload = json!({
        "command": "can-reach",
        "from": from,
        "to": to,
        "can_reach": can_reach_result
    });
    println!("{payload}");
    Ok(())
}

fn run_unreachable_nodes(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("unreachable-nodes command requires SQLite backend")
    })?;

    let entry = required_flag_value(args, "--entry").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid entry node: {e}")))
    })?;

    let unreachable = unreachable_from(graph, entry)?;

    // Convert HashSet to Vec for JSON serialization
    let unreachable_vec: Vec<i64> = unreachable.into_iter().collect();

    let payload = json!({
        "command": "unreachable-nodes",
        "entry": entry,
        "unreachable_count": unreachable_vec.len(),
        "unreachable_nodes": unreachable_vec
    });
    println!("{payload}");
    Ok(())
}
fn run_enumerate_paths(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("enumerate-paths command requires SQLite backend")
    })?;

    let start = required_flag_value(args, "--start").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid start node: {e}")))
    })?;

    let max_depth = optional_flag_value(args, "--max-depth")
        .map(|s| {
            s.parse::<usize>()
                .map_err(|e| SqliteGraphError::invalid_input(format!("invalid max-depth: {e}")))
        })
        .transpose()?
        .unwrap_or(100);

    let max_paths = optional_flag_value(args, "--max-paths")
        .map(|s| {
            s.parse::<usize>()
                .map_err(|e| SqliteGraphError::invalid_input(format!("invalid max-paths: {e}")))
        })
        .transpose()?
        .unwrap_or(1000);

    let progress = ConsoleProgress::new();
    let config = PathEnumerationConfig {
        max_depth,
        max_paths,
        revisit_cap: 100,
    };
    let result = enumerate_paths_with_progress(graph, start, config, &progress)?;

    let payload = json!({
        "command": "enumerate-paths",
        "start": start,
        "max_depth": max_depth,
        "max_paths": max_paths,
        "path_count": result.paths.len(),
        "normal_count": result.statistics.normal_paths,
        "error_count": result.statistics.error_paths,
        "paths": result.paths.iter().take(100).map(|p| json!({
            "nodes": p.nodes,
            "classification": format!("{:?}", p.classification)
        })).collect::<Vec<_>>()
    });
    println!("{payload}");
    Ok(())
}

fn run_enumerate_paths_constrained(
    client: &BackendClient,
    args: &[String],
) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("enumerate-paths-constrained command requires SQLite backend")
    })?;

    let start = required_flag_value(args, "--start").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid start node: {e}")))
    })?;

    let enable_dominance = args.iter().any(|a| a == "--enable-dominance");
    let enable_cd = args.iter().any(|a| a == "--enable-cd");
    let enable_loops = args.iter().any(|a| a == "--enable-loops");

    let progress = ConsoleProgress::new();
    let config = PathEnumerationDominanceConfig {
        enable_dominance_pruning: enable_dominance,
        enable_cd_pruning: enable_cd,
        enable_loop_pruning: enable_loops,
        max_depth: 100,
        max_paths: 1000,
        revisit_cap: 100,
    };
    let result = enumerate_paths_with_dominance_progress(graph, start, config, &progress)?;

    let payload = json!({
        "command": "enumerate-paths-constrained",
        "start": start,
        "enable_dominance": enable_dominance,
        "enable_cd": enable_cd,
        "enable_loops": enable_loops,
        "path_count": result.paths.len(),
        "pruning_stats": result.pruning_stats
    });
    println!("{payload}");
    Ok(())
}

fn run_critical_path(client: &BackendClient, _args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("critical-path command requires SQLite backend")
    })?;

    let progress = ConsoleProgress::new();
    match critical_path_with_progress(graph, &default_weight_fn, &progress) {
        Ok(result) => {
            let payload = json!({
                "command": "critical-path",
                "status": "success",
                "path_length": result.path.len(),
                "total_distance": result.total_distance,
                "path": result.path,
                "bottlenecks": result.bottlenecks
            });
            println!("{payload}");
            Ok(())
        }
        Err(e) => {
            let payload = json!({
                "command": "critical-path",
                "status": "error",
                "error": e.to_string()
            });
            println!("{payload}");
            Err(SqliteGraphError::invalid_input(format!("critical-path failed: {}", e)))
        }
    }
}

fn run_cycle_basis(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("cycle-basis command requires SQLite backend")
    })?;

    let max_cycles = optional_flag_value(args, "--max-cycles")
        .map(|s| {
            s.parse::<usize>()
                .map_err(|e| SqliteGraphError::invalid_input(format!("invalid max-cycles: {e}")))
        })
        .transpose()?
        .unwrap_or(100);

    let max_cycle_length = optional_flag_value(args, "--max-cycle-length")
        .map(|s| {
            s.parse::<usize>()
                .map_err(|e| SqliteGraphError::invalid_input(format!("invalid max-cycle-length: {e}")))
        })
        .transpose()?
        .unwrap_or(20);

    let progress = ConsoleProgress::new();
    let result = cycle_basis_with_progress(graph, max_cycles, max_cycle_length, &progress)?;

    let payload = json!({
        "command": "cycle-basis",
        "max_cycles": max_cycles,
        "max_cycle_length": max_cycle_length,
        "cycle_count": result.cycles.len(),
        "cycles": result.cycles
    });
    println!("{payload}");
    Ok(())
}

// ============================================================================
// Graph Diff and Refactor Validation Commands (Phase 55-56)
// ============================================================================

fn run_structural_similarity(
    client: &BackendClient,
    args: &[String],
) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("structural-similarity command requires SQLite backend")
    })?;

    let graph1 = required_flag_value(args, "--graph1").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid graph1 node: {e}")))
    })?;

    let graph2 = required_flag_value(args, "--graph2").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid graph2 node: {e}")))
    })?;

    let progress = ConsoleProgress::new();
    let bounds = SimilarityBounds {
        max_matches: Some(100),
        timeout_ms: Some(30000),
        similarity_threshold: None,
    };

    let result = structural_similarity_with_progress(graph, graph1, graph2, bounds, &progress)?;

    let payload = json!({
        "command": "structural-similarity",
        "graph1": graph1,
        "graph2": graph2,
        "isomorphic": result.isomorphic,
        "mcs_similarity": result.mcs_similarity,
        "ged_distance": result.ged_distance,
        "mcs_size": result.mcs_size,
        "similarity_class": result.similarity_class()
    });
    println!("{payload}");
    Ok(())
}

fn run_graph_diff(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("graph-diff command requires SQLite backend")
    })?;

    let before = required_flag_value(args, "--before")?;
    let after = required_flag_value(args, "--after")?;

    // For CLI, --before and --after refer to node IDs representing graph roots
    // This is a simplified implementation that compares subtrees
    let progress = ConsoleProgress::new();

    // Get all nodes in each "graph" (subtree rooted at the given node)
    let before_nodes = reachable_from_with_progress(graph, before, &progress)?;
    let after_nodes = reachable_from_with_progress(graph, after, &progress)?;

    let before_set: std::collections::HashSet<i64> = before_nodes.iter().copied().collect();
    let after_set: std::collections::HashSet<i64> = after_nodes.iter().copied().collect();

    let nodes_added: Vec<i64> = after_set.difference(&before_set).copied().collect();
    let nodes_removed: Vec<i64> = before_set.difference(&after_set).copied().collect();
    let nodes_common: Vec<i64> = before_set.intersection(&after_set).copied().collect();

    // Compute similarity on common structure
    let similarity_score = if before_set.is_empty() && after_set.is_empty() {
        1.0
    } else if before_set.is_empty() || after_set.is_empty() {
        0.0
    } else {
        let common_count = nodes_common.len() as f64;
        let max_size = before_set.len().max(after_set.len()) as f64;
        common_count / max_size
    };

    let payload = json!({
        "command": "graph-diff",
        "before": before,
        "after": after,
        "nodes_added": nodes_added,
        "nodes_removed": nodes_removed,
        "nodes_common": nodes_common,
        "similarity_score": similarity_score,
        "note": "Subtree comparison based on reachable nodes from given roots"
    });
    println!("{payload}");
    Ok(())
}

fn run_validate_refactor(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("validate-refactor command requires SQLite backend")
    })?;

    let before = required_flag_value(args, "--before")?;
    let after = required_flag_value(args, "--after")?;

    // Perform graph diff between before/after nodes
    let progress = ConsoleProgress::new();

    let before_nodes = reachable_from_with_progress(graph, before, &progress)?;
    let after_nodes = reachable_from_with_progress(graph, after, &progress)?;

    let before_set: std::collections::HashSet<i64> = before_nodes.iter().copied().collect();
    let after_set: std::collections::HashSet<i64> = after_nodes.iter().copied().collect();

    let nodes_removed: Vec<i64> = before_set.difference(&after_set).copied().collect();
    let edges_removed = nodes_removed.len(); // Simplified: treat removed nodes as removed edges

    let common_count = before_set.intersection(&after_set).count();
    let max_size = before_set.len().max(after_set.len());
    let similarity_score = if max_size == 0 {
        1.0
    } else {
        common_count as f64 / max_size as f64
    };

    // Build a mock GraphDiffResult for validation
    let diff_result = json!({
        "nodes_added": after_set.difference(&before_set).copied().collect::<Vec<_>>(),
        "nodes_removed": nodes_removed,
        "edges_added": 0,  // Not tracking edges in subtree comparison
        "edges_removed": edges_removed,
        "similarity_score": similarity_score,
        "is_isomorphic": similarity_score == 1.0,
        "graph_edit_distance": 1.0 - similarity_score,
        "graph1_size": before_set.len(),
        "graph2_size": after_set.len()
    });

    // Apply validation heuristics
    let is_safe = nodes_removed.is_empty() && similarity_score >= 0.5;
    let has_breaking = !nodes_removed.is_empty() || similarity_score < 0.5;

    let mut breaking_changes = Vec::new();
    let mut warnings = Vec::new();

    if !nodes_removed.is_empty() {
        breaking_changes.push(format!(
            "Removed {} nodes - potentially breaking",
            nodes_removed.len()
        ));
    }

    if similarity_score < 0.5 {
        breaking_changes.push(format!(
            "Low similarity score: {:.2} - significant structural changes",
            similarity_score
        ));
    } else if similarity_score < 0.8 {
        warnings.push(format!(
            "Moderate similarity: {:.2} - review recommended",
            similarity_score
        ));
    }

    if similarity_score == 1.0 {
        warnings.push("Structure preserved (isomorphic)".to_string());
    }

    if !nodes_removed.is_empty() {
        warnings.push(format!(
            "Removed {} nodes/edges - review control flow impact",
            nodes_removed.len()
        ));
    }

    let payload = json!({
        "command": "validate-refactor",
        "before": before,
        "after": after,
        "is_safe": is_safe,
        "has_breaking_changes": has_breaking,
        "breaking_changes": breaking_changes,
        "warnings": warnings,
        "diff_result": diff_result
    });
    println!("{payload}");
    Ok(())
}

// ============================================================================
// Security and Taint Analysis Commands (Phase 56)
// ============================================================================

fn run_taint_forward(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("taint-forward command requires SQLite backend")
    })?;

    let sources_file = required_flag_value(args, "--sources-file")?;

    // Read sources JSON file
    let json_content = fs::read_to_string(&sources_file).map_err(|e| {
        SqliteGraphError::invalid_input(format!("failed to read sources file: {e}"))
    })?;

    let sources_json: serde_json::Value = serde_json::from_str(&json_content).map_err(|e| {
        SqliteGraphError::invalid_input(format!("failed to parse sources file: {e}"))
    })?;

    let sources: Vec<i64> = sources_json["sources"]
        .as_array()
        .ok_or_else(|| {
            SqliteGraphError::invalid_input("sources file must contain 'sources' array")
        })?
        .iter()
        .map(|v| {
            v.as_i64()
                .ok_or_else(|| SqliteGraphError::invalid_input("source must be a number"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Discover sinks automatically
    let (_auto_sources, sinks) = discover_sources_and_sinks_default(graph)?;
    let sinks_vec = sinks;

    let progress = ConsoleProgress::new();
    let result =
        propagate_taint_forward_with_progress(graph, &sources, &sinks_vec, &progress)?;

    let payload = json!({
        "command": "taint-forward",
        "sources_file": sources_file,
        "sources": sources,
        "sinks_analyzed": sinks_vec.len(),
        "tainted_nodes": result.sorted_tainted_nodes(),
        "tainted_count": result.tainted_nodes.len(),
        "sinks_reached": result.sinks_reached.iter().copied().collect::<Vec<_>>(),
        "sinks_reached_count": result.sinks_reached.len(),
        "vulnerabilities": result.sorted_vulnerabilities(),
        "vulnerability_count": result.source_sink_paths.len(),
        "has_vulnerability": result.has_vulnerability()
    });
    println!("{payload}");
    Ok(())
}

fn run_taint_backward(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("taint-backward command requires SQLite backend")
    })?;

    let sink = required_flag_value(args, "--sink").and_then(|s| {
        s.parse::<i64>()
            .map_err(|e| SqliteGraphError::invalid_input(format!("invalid sink node: {e}")))
    })?;

    let sources_file = required_flag_value(args, "--sources-file")?;

    // Read sources JSON file (optional for backward propagation)
    let sources: Vec<i64> = if let Ok(json_content) = fs::read_to_string(&sources_file) {
        let sources_json: serde_json::Value =
            serde_json::from_str(&json_content).unwrap_or_default();
        sources_json["sources"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default()
    } else {
        vec![]
    };

    let progress = ConsoleProgress::new();
    let result = propagate_taint_backward_with_progress(graph, sink, &sources, &progress)?;

    let payload = json!({
        "command": "taint-backward",
        "sink": sink,
        "sources_file": sources_file,
        "sources_provided": sources,
        "sources_reached": result.sources.iter().copied().collect::<Vec<_>>(),
        "sources_count": result.sources.len(),
        "tainted_nodes": result.sorted_tainted_nodes(),
        "tainted_count": result.tainted_nodes.len(),
        "has_vulnerability": result.has_vulnerability()
    });
    println!("{payload}");
    Ok(())
}

fn run_sink_analysis(client: &BackendClient, args: &[String]) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("sink-analysis command requires SQLite backend")
    })?;

    let sources_file = required_flag_value(args, "--sources-file")?;
    let sinks_file = required_flag_value(args, "--sinks-file")?;

    // Read sources JSON file
    let sources_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&sources_file).map_err(|e| {
            SqliteGraphError::invalid_input(format!("failed to read sources file: {e}"))
        })?,
    )
    .map_err(|e| {
        SqliteGraphError::invalid_input(format!("failed to parse sources file: {e}"))
    })?;

    let sources: Vec<i64> = sources_json["sources"]
        .as_array()
        .ok_or_else(|| {
            SqliteGraphError::invalid_input("sources file must contain 'sources' array")
        })?
        .iter()
        .map(|v| {
            v.as_i64()
                .ok_or_else(|| SqliteGraphError::invalid_input("source must be a number"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Read sinks JSON file
    let sinks_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&sinks_file).map_err(|e| {
            SqliteGraphError::invalid_input(format!("failed to read sinks file: {e}"))
        })?,
    )
    .map_err(|e| {
        SqliteGraphError::invalid_input(format!("failed to parse sinks file: {e}"))
    })?;

    let sinks: Vec<i64> = sinks_json["sinks"]
        .as_array()
        .ok_or_else(|| {
            SqliteGraphError::invalid_input("sinks file must contain 'sinks' array")
        })?
        .iter()
        .map(|v| {
            v.as_i64()
                .ok_or_else(|| SqliteGraphError::invalid_input("sink must be a number"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let progress = ConsoleProgress::new();
    let vulnerabilities =
        sink_reachability_analysis_with_progress(graph, &sources, &sinks, &progress)?;

    // Convert to more structured output
    let vuln_list: Vec<serde_json::Value> = vulnerabilities
        .iter()
        .map(|(sink, affecting_sources)| {
            json!({
                "sink": sink,
                "affecting_sources": affecting_sources,
                "source_count": affecting_sources.len()
            })
        })
        .collect();

    let payload = json!({
        "command": "sink-analysis",
        "sources_file": sources_file,
        "sinks_file": sinks_file,
        "sources_count": sources.len(),
        "sinks_count": sinks.len(),
        "vulnerable_sinks_count": vulnerabilities.len(),
        "vulnerabilities": vuln_list
    });
    println!("{payload}");
    Ok(())
}

fn run_discover_sources_sinks(
    client: &BackendClient,
    _args: &[String],
) -> Result<(), SqliteGraphError> {
    let graph = client.graph().ok_or_else(|| {
        SqliteGraphError::invalid_input("discover-sources-sinks command requires SQLite backend")
    })?;

    let result = discover_sources_and_sinks_default(graph)?;

    let payload = json!({
        "command": "discover-sources-sinks",
        "sources_count": result.0.len(),
        "sources": result.0,
        "sinks_count": result.1.len(),
        "sinks": result.1
    });
    println!("{payload}");
    Ok(())
}
