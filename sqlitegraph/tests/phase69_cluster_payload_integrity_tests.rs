//! Phase 69 — Cluster payload integrity guardrails.
//!
//! This suite focuses on ensuring framed V2 clusters never leave bytes unread
//! at the cursor after reopen and that strict framing stays enabled.

use sqlitegraph::backend::native::constants::FLAG_V2_FRAMED_RECORDS;
use sqlitegraph::backend::native::graph_file::GraphFile;
use sqlitegraph::{BackendDirection, EdgeSpec, GraphConfig, NeighborQuery, NodeSpec, open_graph};
use std::error::Error;

#[test]
fn test_reopen_requires_zero_cursor_remainder() -> Result<(), Box<dyn Error>> {
    const NUM_NODES: usize = 100;

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("phase69_cluster_payload.db");

    // Create a native graph and populate it with deterministic nodes + edges.
    let cfg = GraphConfig::native();
    let mut graph = open_graph(&db_path, &cfg)?;

    let mut node_ids = Vec::with_capacity(NUM_NODES);
    for i in 0..NUM_NODES {
        let node_id = graph.insert_node(NodeSpec {
            kind: "Phase69Node".to_string(),
            name: format!("node_{}", i),
            file_path: None,
            data: serde_json::json!({ "index": i, "phase": 69 }),
        })?;
        node_ids.push(node_id);
    }

    let mut total_edges = 0usize;
    for source_idx in 0..NUM_NODES {
        let from_id = node_ids[source_idx];

        // Pattern A: fan out to two successive nodes.
        for offset in 1..=2 {
            let target = node_ids[(source_idx + offset) % NUM_NODES];
            graph.insert_edge(EdgeSpec {
                from: from_id,
                to: target,
                edge_type: "phase69_distinct".to_string(),
                data: serde_json::json!({
                    "pattern": "distinct",
                    "source_idx": source_idx,
                    "target_idx": (source_idx + offset) % NUM_NODES,
                    "edge_index": offset
                }),
            })?;
            total_edges += 1;
        }

        // Pattern B: multi-edge pressure to a fixed offset target.
        let repeated_target = node_ids[(source_idx + 5) % NUM_NODES];
        for edge_idx in 0..2 {
            graph.insert_edge(EdgeSpec {
                from: from_id,
                to: repeated_target,
                edge_type: "phase69_multi".to_string(),
                data: serde_json::json!({
                    "pattern": "multi",
                    "source_idx": source_idx,
                    "target_idx": (source_idx + 5) % NUM_NODES,
                    "edge_index": edge_idx
                }),
            })?;
            total_edges += 1;
        }
    }

    assert_eq!(total_edges, NUM_NODES * 4, "edge fanout mismatch");

    // Capture expected neighbor sets before closing the graph file.
    let mut expected_outgoing = Vec::with_capacity(NUM_NODES);
    let mut expected_incoming = Vec::with_capacity(NUM_NODES);
    for &node_id in &node_ids {
        let outgoing = graph.neighbors(
            node_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: None,
            },
        )?;
        expected_outgoing.push((node_id, outgoing));

        let incoming = graph.neighbors(
            node_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: None,
            },
        )?;
        expected_incoming.push((node_id, incoming));
    }

    // Drop the writer before reopening.
    drop(graph);

    // Verify the persisted header advertises framed clusters.
    let graph_file = GraphFile::open(&db_path)?;
    assert!(
        graph_file.header().flags & FLAG_V2_FRAMED_RECORDS != 0,
        "phase69: V2 framed flag must be persisted before reopen"
    );
    drop(graph_file);

    // Reopen without create_if_missing so we hit the recorded bytes.
    let mut reopen_cfg = GraphConfig::native();
    reopen_cfg.native.create_if_missing = false;
    let reopened = open_graph(&db_path, &reopen_cfg)?;

    assert_neighbors_clean(&reopened, &expected_outgoing, BackendDirection::Outgoing);
    assert_neighbors_clean(&reopened, &expected_incoming, BackendDirection::Incoming);

    Ok(())
}

fn assert_neighbors_clean(
    graph: &Box<dyn sqlitegraph::backend::GraphBackend>,
    cases: &[(i64, Vec<i64>)],
    direction: BackendDirection,
) {
    for (node_id, expected) in cases {
        let result = graph.neighbors(
            *node_id,
            NeighborQuery {
                direction,
                edge_type: None,
            },
        );
        match result {
            Ok(actual) => assert_eq!(
                actual, *expected,
                "phase69: neighbor drift detected for node {} ({:?})",
                node_id, direction
            ),
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    !msg.contains("preview_ascii"),
                    "phase69: reopen must not emit preview_ascii fragments: {}",
                    msg
                );
                assert!(
                    msg.contains("remaining=0"),
                    "phase69: cursor must reach payload end (node {} {:?}): {}",
                    node_id,
                    direction,
                    msg
                );
                panic!(
                    "phase69: clustered neighbor read failed for node {} {:?}: {}",
                    node_id, direction, msg
                );
            }
        }
    }
}
