//! SQLite backend snapshot isolation validation tests
//!
//! These tests verify that the SQLite backend correctly rejects historical
//! snapshot requests and only accepts SnapshotId::current().
//!
//! **Background**: SQLite backend does not support historical snapshot isolation.
//! Only SnapshotId::current() (which has as_lsn() == 0) is supported.
//!
//! Historical snapshot isolation would require:
//! - WAL-based versioning with timestamp/LSN indexing
//! - AS OF queries or point-in-time recovery mechanisms
//! - Multi-version concurrency control (MVCC) extensions
//!
//! These are not implemented in the current SQLite backend.

use sqlitegraph::{
    NodeSpec, SnapshotId,
    backend::{BackendDirection, GraphBackend, NeighborQuery, SqliteGraphBackend},
    multi_hop::ChainStep,
    pattern::PatternQuery,
};

/// Test that historical snapshot_id (non-zero LSN) is rejected in get_node
#[test]
fn test_sqlite_historical_snapshot_rejected_get_node() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert a test node
    let node_id = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "test_node".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    // Historical snapshot should be rejected
    let historical_snapshot = SnapshotId::from_lsn(12345);
    let result = backend.get_node(historical_snapshot, node_id);

    assert!(result.is_err(), "Historical snapshot should be rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support historical snapshots"),
        "Error message should explain limitation: {}",
        err_msg
    );
}

/// Test that SnapshotId::current() (lsn == 0) works in get_node
#[test]
fn test_sqlite_current_snapshot_works_get_node() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert a test node
    let node_id = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "test_node".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    // Current snapshot should work
    let current_snapshot = SnapshotId::current();
    let result = backend.get_node(current_snapshot, node_id);

    assert!(result.is_ok(), "Current snapshot should work: {:?}", result);
    let node = result.unwrap();
    assert_eq!(node.name, "test_node");
}

/// Test that historical snapshot_id is rejected in neighbors
#[test]
fn test_sqlite_historical_snapshot_rejected_neighbors() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert two test nodes and an edge
    let node1 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();
    let node2 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node2".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    backend
        .insert_edge(sqlitegraph::EdgeSpec {
            from: node1,
            to: node2,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!(null),
        })
        .unwrap();

    // Historical snapshot should be rejected
    let historical_snapshot = SnapshotId::from_lsn(999);
    let query = NeighborQuery {
        direction: BackendDirection::Outgoing,
        edge_type: None,
    };
    let result = backend.neighbors(historical_snapshot, node1, query);

    assert!(
        result.is_err(),
        "Historical snapshot should be rejected in neighbors"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support historical snapshots"),
        "Error message should explain limitation: {}",
        err_msg
    );
}

/// Test that historical snapshot_id is rejected in bfs
#[test]
fn test_sqlite_historical_snapshot_rejected_bfs() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert test nodes
    let node1 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    // Historical snapshot should be rejected in bfs
    let historical_snapshot = SnapshotId::from_lsn(555);
    let result = backend.bfs(historical_snapshot, node1, 2);

    assert!(
        result.is_err(),
        "Historical snapshot should be rejected in bfs"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support historical snapshots"),
        "Error message should explain limitation: {}",
        err_msg
    );
}

/// Test that historical snapshot_id is rejected in shortest_path
#[test]
fn test_sqlite_historical_snapshot_rejected_shortest_path() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert test nodes
    let node1 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();
    let node2 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node2".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    // Historical snapshot should be rejected in shortest_path
    let historical_snapshot = SnapshotId::from_lsn(777);
    let result = backend.shortest_path(historical_snapshot, node1, node2);

    assert!(
        result.is_err(),
        "Historical snapshot should be rejected in shortest_path"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support historical snapshots"),
        "Error message should explain limitation: {}",
        err_msg
    );
}

/// Test that historical snapshot_id is rejected in node_degree
#[test]
fn test_sqlite_historical_snapshot_rejected_node_degree() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert test node
    let node1 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    // Historical snapshot should be rejected in node_degree
    let historical_snapshot = SnapshotId::from_lsn(333);
    let result = backend.node_degree(historical_snapshot, node1);

    assert!(
        result.is_err(),
        "Historical snapshot should be rejected in node_degree"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support historical snapshots"),
        "Error message should explain limitation: {}",
        err_msg
    );
}

/// Test that historical snapshot_id is rejected in k_hop
#[test]
fn test_sqlite_historical_snapshot_rejected_k_hop() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert test node
    let node1 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    // Historical snapshot should be rejected in k_hop
    let historical_snapshot = SnapshotId::from_lsn(444);
    let result = backend.k_hop(historical_snapshot, node1, 2, BackendDirection::Outgoing);

    assert!(
        result.is_err(),
        "Historical snapshot should be rejected in k_hop"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support historical snapshots"),
        "Error message should explain limitation: {}",
        err_msg
    );
}

/// Test that historical snapshot_id is rejected in k_hop_filtered
#[test]
fn test_sqlite_historical_snapshot_rejected_k_hop_filtered() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert test node
    let node1 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    // Historical snapshot should be rejected in k_hop_filtered
    let historical_snapshot = SnapshotId::from_lsn(666);
    let allowed_types = vec!["test_edge"];
    let result = backend.k_hop_filtered(
        historical_snapshot,
        node1,
        2,
        BackendDirection::Outgoing,
        &allowed_types,
    );

    assert!(
        result.is_err(),
        "Historical snapshot should be rejected in k_hop_filtered"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support historical snapshots"),
        "Error message should explain limitation: {}",
        err_msg
    );
}

/// Test that historical snapshot_id is rejected in chain_query
#[test]
fn test_sqlite_historical_snapshot_rejected_chain_query() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert test node
    let node1 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    // Historical snapshot should be rejected in chain_query
    let historical_snapshot = SnapshotId::from_lsn(888);
    let chain = vec![ChainStep {
        direction: BackendDirection::Outgoing,
        edge_type: None,
    }];
    let result = backend.chain_query(historical_snapshot, node1, &chain);

    assert!(
        result.is_err(),
        "Historical snapshot should be rejected in chain_query"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support historical snapshots"),
        "Error message should explain limitation: {}",
        err_msg
    );
}

/// Test that historical snapshot_id is rejected in pattern_search
#[test]
fn test_sqlite_historical_snapshot_rejected_pattern_search() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert test node
    let node1 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    // Historical snapshot should be rejected in pattern_search
    let historical_snapshot = SnapshotId::from_lsn(111);
    let pattern = PatternQuery::default();
    let result = backend.pattern_search(historical_snapshot, node1, &pattern);

    assert!(
        result.is_err(),
        "Historical snapshot should be rejected in pattern_search"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support historical snapshots"),
        "Error message should explain limitation: {}",
        err_msg
    );
}

/// Test that historical snapshot_id is rejected in query_nodes_by_kind
#[test]
fn test_sqlite_historical_snapshot_rejected_query_nodes_by_kind() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert test node
    backend
        .insert_node(NodeSpec {
            kind: "test_kind".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    // Historical snapshot should be rejected in query_nodes_by_kind
    let historical_snapshot = SnapshotId::from_lsn(222);
    let result = backend.query_nodes_by_kind(historical_snapshot, "test_kind");

    assert!(
        result.is_err(),
        "Historical snapshot should be rejected in query_nodes_by_kind"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support historical snapshots"),
        "Error message should explain limitation: {}",
        err_msg
    );
}

/// Test that historical snapshot_id is rejected in query_nodes_by_name_pattern
#[test]
fn test_sqlite_historical_snapshot_rejected_query_nodes_by_name_pattern() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Insert test node
    backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    // Historical snapshot should be rejected in query_nodes_by_name_pattern
    let historical_snapshot = SnapshotId::from_lsn(333);
    let result = backend.query_nodes_by_name_pattern(historical_snapshot, "node*");

    assert!(
        result.is_err(),
        "Historical snapshot should be rejected in query_nodes_by_name_pattern"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not support historical snapshots"),
        "Error message should explain limitation: {}",
        err_msg
    );
}

/// Test that SnapshotId::current() works for all operations
#[test]
fn test_sqlite_current_snapshot_works_all_operations() {
    let backend = SqliteGraphBackend::in_memory().unwrap();

    // Create a simple graph
    let node1 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node1".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();
    let node2 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node2".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();
    let node3 = backend
        .insert_node(NodeSpec {
            kind: "test".to_string(),
            name: "node3".to_string(),
            file_path: None,
            data: serde_json::json!(null),
        })
        .unwrap();

    backend
        .insert_edge(sqlitegraph::EdgeSpec {
            from: node1,
            to: node2,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!(null),
        })
        .unwrap();
    backend
        .insert_edge(sqlitegraph::EdgeSpec {
            from: node2,
            to: node3,
            edge_type: "test_edge".to_string(),
            data: serde_json::json!(null),
        })
        .unwrap();

    let current = SnapshotId::current();

    // All operations should work with current snapshot
    assert!(backend.get_node(current, node1).is_ok());
    assert!(
        backend
            .neighbors(
                current,
                node1,
                NeighborQuery {
                    direction: BackendDirection::Outgoing,
                    edge_type: None,
                }
            )
            .is_ok()
    );
    assert!(backend.bfs(current, node1, 2).is_ok());
    assert!(backend.shortest_path(current, node1, node3).is_ok());
    assert!(backend.node_degree(current, node1).is_ok());
    assert!(
        backend
            .k_hop(current, node1, 2, BackendDirection::Outgoing)
            .is_ok()
    );
    assert!(
        backend
            .k_hop_filtered(
                current,
                node1,
                2,
                BackendDirection::Outgoing,
                &["test_edge"]
            )
            .is_ok()
    );
    assert!(
        backend
            .chain_query(
                current,
                node1,
                &[ChainStep {
                    direction: BackendDirection::Outgoing,
                    edge_type: None,
                }]
            )
            .is_ok()
    );
    assert!(
        backend
            .pattern_search(current, node1, &PatternQuery::default())
            .is_ok()
    );
    assert!(backend.query_nodes_by_kind(current, "test").is_ok());
    assert!(
        backend
            .query_nodes_by_name_pattern(current, "node*")
            .is_ok()
    );
}
