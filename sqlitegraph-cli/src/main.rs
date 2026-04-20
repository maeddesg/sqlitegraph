use anyhow::{Context, Result};
use clap::Parser;
use serde_json::json;
use sqlitegraph_cli::cli::{AlgoCommands, Cli, Commands, Direction};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Check for write operations without --write flag
    if !cli.write {
        match &cli.command {
            Commands::Import { .. } | Commands::Insert { .. } | Commands::Export { .. } => {
                anyhow::bail!(
                    "This command modifies the database. Use --write flag to enable write mode"
                );
            }
            _ => {}
        }
    }

    // Open client
    let client = if cli.db.to_string_lossy() == ":memory:" {
        sqlitegraph_cli::client::CliClient::open_in_memory(cli.backend)?
    } else {
        sqlitegraph_cli::client::CliClient::open(cli.backend, &cli.db)?
    };

    // Execute command
    match cli.command {
        Commands::Query { query } => run_query(&client, &query),
        Commands::Status => run_status(&client),
        Commands::List { kind } => run_list(&client, kind),
        Commands::Bfs { start, depth } => run_bfs(&client, start, depth),
        Commands::Path { from, to } => run_path(&client, from, to),
        Commands::Neighbors { id, direction } => run_neighbors(&client, id, direction),
        Commands::Algo { command } => run_algo(&client, command),
        Commands::Export { output } => run_export(&client, &output),
        Commands::Import { input } => run_import(&client, &input),
        Commands::Insert { kind, name, data } => run_insert(&client, &kind, &name, data),
    }
}

fn run_query(client: &sqlitegraph_cli::client::CliClient, query_str: &str) -> Result<()> {
    let query = sqlitegraph_cli::query::parse(query_str)?;
    let result = sqlitegraph_cli::query::execute(client.backend(), &query)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn run_status(client: &sqlitegraph_cli::client::CliClient) -> Result<()> {
    let nodes = client.node_count()?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "backend": client.backend_name(),
            "nodes": nodes,
        }))?
    );
    Ok(())
}

fn run_list(client: &sqlitegraph_cli::client::CliClient, kind: Option<String>) -> Result<()> {
    use sqlitegraph::snapshot::SnapshotId;

    let backend = client.backend();
    let node_ids = backend.entity_ids()?;
    let snapshot = SnapshotId::current();

    let mut nodes = Vec::new();
    for id in node_ids {
        if let Ok(node) = backend.get_node(snapshot, id) {
            if let Some(ref filter_kind) = kind {
                if node.kind != *filter_kind {
                    continue;
                }
            }
            nodes.push(json!({
                "id": node.id,
                "kind": node.kind,
                "name": node.name,
            }));
        }
    }

    println!("{}", serde_json::to_string_pretty(&nodes)?);
    Ok(())
}

fn run_bfs(client: &sqlitegraph_cli::client::CliClient, start: i64, depth: u32) -> Result<()> {
    use sqlitegraph::bfs::bfs_neighbors;

    let graph = client
        .sqlite_graph()
        .context("BFS requires SQLite backend")?;

    let visited = bfs_neighbors(graph, start, depth)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "start": start,
            "max_depth": depth,
            "visited": visited,
            "count": visited.len(),
        }))?
    );
    Ok(())
}

fn run_path(client: &sqlitegraph_cli::client::CliClient, from: i64, to: i64) -> Result<()> {
    use sqlitegraph::bfs::shortest_path;

    let graph = client
        .sqlite_graph()
        .context("Shortest path requires SQLite backend")?;

    let path = shortest_path(graph, from, to)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "from": from,
            "to": to,
            "path": path,
            "found": path.is_some(),
        }))?
    );
    Ok(())
}

fn run_neighbors(
    client: &sqlitegraph_cli::client::CliClient,
    id: i64,
    direction: Direction,
) -> Result<()> {
    let graph = client
        .sqlite_graph()
        .context("Neighbors requires SQLite backend")?;

    let query = graph.query();
    let neighbors = match direction {
        Direction::Incoming => query.incoming(id)?,
        Direction::Outgoing => query.outgoing(id)?,
        Direction::Both => {
            let mut n = query.incoming(id)?;
            n.extend(query.outgoing(id)?);
            n
        }
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "id": id,
            "direction": format!("{:?}", direction),
            "neighbors": neighbors,
            "count": neighbors.len(),
        }))?
    );
    Ok(())
}

fn run_algo(client: &sqlitegraph_cli::client::CliClient, command: AlgoCommands) -> Result<()> {
    use sqlitegraph::algo::*;
    use sqlitegraph::progress::ConsoleProgress;

    let graph = client
        .sqlite_graph()
        .context("Algorithms require SQLite backend")?;
    let progress = ConsoleProgress::new();

    match command {
        AlgoCommands::Pagerank { iterations } => {
            let scores = pagerank_with_progress(graph, 0.85, iterations, &progress)?;
            let top: Vec<_> = scores
                .iter()
                .take(10)
                .map(|(id, score)| json!({"id": id, "score": score}))
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "algorithm": "pagerank",
                    "iterations": iterations,
                    "top_scores": top,
                }))?
            );
        }
        AlgoCommands::Betweenness => {
            let scores = betweenness_centrality_with_progress(graph, &progress)?;
            let top: Vec<_> = scores
                .iter()
                .take(10)
                .map(|(id, score)| json!({"id": id, "score": score}))
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "algorithm": "betweenness",
                    "top_scores": top,
                }))?
            );
        }
        AlgoCommands::Components => {
            let components = weakly_connected_components_with_progress(graph, &progress)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "algorithm": "wcc",
                    "component_count": components.len(),
                    "components": components.iter().take(10).collect::<Vec<_>>(),
                }))?
            );
        }
        AlgoCommands::Topo => {
            let order = topological_sort(graph)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "algorithm": "topological_sort",
                    "node_count": order.len(),
                    "order": order.iter().take(100).collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

fn run_export(client: &sqlitegraph_cli::client::CliClient, output: &std::path::Path) -> Result<()> {
    use sqlitegraph::recovery::dump_graph_to_path;

    let graph = client
        .sqlite_graph()
        .context("Export requires SQLite backend")?;

    dump_graph_to_path(graph, output)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "exported_to": output,
            "status": "ok",
        }))?
    );
    Ok(())
}

fn run_import(client: &sqlitegraph_cli::client::CliClient, input: &std::path::Path) -> Result<()> {
    use sqlitegraph::recovery::load_graph_from_path;

    let graph = client
        .sqlite_graph()
        .context("Import requires SQLite backend")?;

    load_graph_from_path(graph, input)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "imported_from": input,
            "status": "ok",
        }))?
    );
    Ok(())
}

fn run_insert(
    client: &sqlitegraph_cli::client::CliClient,
    kind: &str,
    name: &str,
    data: Option<String>,
) -> Result<()> {
    use sqlitegraph::backend::NodeSpec;

    let backend = client.backend();
    let data_json = match data {
        Some(s) => serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!({"data": s})),
        None => serde_json::json!({}),
    };

    let id = backend.insert_node(NodeSpec {
        kind: kind.to_string(),
        name: name.to_string(),
        file_path: None,
        data: data_json,
    })?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "id": id,
            "kind": kind,
            "name": name,
            "status": "created",
        }))?
    );
    Ok(())
}
