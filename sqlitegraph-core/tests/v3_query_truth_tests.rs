//! V3 Backend Query Truth Audit Tests
//!
//! Tests that verify V3 backend behavior is honest about its capabilities.
//! Per the truth audit principle: stubbed methods must explicitly fail
//! rather than returning fake or incomplete results.

#![cfg(feature = "native-v3")]

use sqlitegraph::{
    SqliteGraphError,
    backend::{BackendDirection, GraphBackend, NodeSpec},
    pattern::{NodeConstraint, PatternLeg, PatternQuery},
};

// Note: V3Backend is conditionally compiled and only available with native-v3 feature
// We use it through the re-export from backend module
use sqlitegraph::backend::native::v3::V3Backend;

use tempfile::TempDir;

/// Helper to create a V3-backed graph
fn create_v3_backend() -> (V3Backend, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v3.graph");
    let backend = V3Backend::create(&db_path).unwrap();
    (backend, temp_dir)
}

#[test]
fn test_v3_pattern_search_returns_unimplemented_error() {
    // BEFORE FIX: pattern_search returned fake vec![PatternMatch { nodes: vec![start] }]
    // AFTER FIX: pattern_search should return explicit Unsupported error
    let (backend, _temp_dir) = create_v3_backend();

    // Create a node to start from
    let start_id = backend
        .insert_node(NodeSpec {
            kind: "TestNode".to_string(),
            name: "test".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Create a simple pattern query
    let pattern = PatternQuery {
        root: None,
        legs: vec![PatternLeg {
            edge_type: None,
            direction: BackendDirection::Outgoing,
            constraint: Some(NodeConstraint::kind("TargetNode")),
        }],
    };

    // Attempt pattern search - should explicitly fail, not return fake results
    let result = backend.pattern_search(sqlitegraph::SnapshotId::current(), start_id, &pattern);

    // Verify we get an explicit Unsupported error, not silent fake results
    match result {
        Err(SqliteGraphError::Unsupported(msg)) => {
            // Success! Error message should be informative
            assert!(
                msg.contains("pattern_search"),
                "Error message should mention pattern_search"
            );
            assert!(
                msg.contains("V3"),
                "Error message should mention V3 backend"
            );
        }
        Ok(matches) => {
            panic!(
                "pattern_search should NOT succeed with fake results! Got: {:?} \
                   This indicates the stub is still returning fake data.",
                matches
            );
        }
        Err(other) => {
            panic!("Expected Unsupported error, got: {:?}", other);
        }
    }
}

#[test]
fn test_v3_snapshot_import_returns_unimplemented_error() {
    // BEFORE FIX: snapshot_import returned ImportMetadata with zeros (pretending success)
    // AFTER FIX: snapshot_import should return explicit Unsupported error
    let (backend, _temp_dir) = create_v3_backend();

    // Attempt snapshot import - should explicitly fail
    let result = backend.snapshot_import(_temp_dir.path());

    // Verify we get an explicit Unsupported error, not silent fake success
    match result {
        Err(SqliteGraphError::Unsupported(msg)) => {
            // Success! Error message should be informative
            assert!(
                msg.contains("snapshot_import"),
                "Error message should mention snapshot_import"
            );
            assert!(
                msg.contains("V3"),
                "Error message should mention V3 backend"
            );
        }
        Ok(metadata) => {
            panic!(
                "snapshot_import should NOT succeed with fake zeros! Got: {:?} \
                   This indicates the stub is still pretending to succeed.",
                metadata
            );
        }
        Err(other) => {
            panic!("Expected Unsupported error, got: {:?}", other);
        }
    }
}

#[test]
fn test_v3_query_nodes_by_name_pattern_substring_not_glob() {
    // This test documents the SEMANTIC_MISMATCH between V3 and SQLite:
    // - SQLite: GLOB pattern matching (wildcards: *, ?, [chars])
    // - V3: Substring matching (contains), case-sensitive
    let (backend, _temp_dir) = create_v3_backend();

    // Create nodes with specific names
    let _user = backend
        .insert_node(NodeSpec {
            kind: "User".to_string(),
            name: "SuperUser".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let _admin = backend
        .insert_node(NodeSpec {
            kind: "Admin".to_string(),
            name: "UserAdmin".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let _root = backend
        .insert_node(NodeSpec {
            kind: "Root".to_string(),
            name: "root".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // V3 uses case-sensitive substring matching - "User" matches both "SuperUser" and "UserAdmin"
    let result = backend
        .query_nodes_by_name_pattern(sqlitegraph::SnapshotId::current(), "User")
        .unwrap();

    // With case-sensitive substring matching, we expect "SuperUser" AND "UserAdmin" to match
    assert_eq!(
        result.len(),
        2,
        "V3 substring matching should find both 'SuperUser' and 'UserAdmin'"
    );

    // If this were GLOB (SQLite), pattern "User" would match exactly "User" only
    // This test documents the behavioral difference
}

#[test]
fn test_v3_query_nodes_by_kind_correct_but_slow() {
    // This test documents that query_nodes_by_kind is CORRECT but uses O(n) scan
    // vs SQLite's O(log n) indexed lookup
    let (backend, _temp_dir) = create_v3_backend();

    // Create nodes of different kinds
    let _user1 = backend
        .insert_node(NodeSpec {
            kind: "User".to_string(),
            name: "alice".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let _user2 = backend
        .insert_node(NodeSpec {
            kind: "User".to_string(),
            name: "bob".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    let _doc = backend
        .insert_node(NodeSpec {
            kind: "Document".to_string(),
            name: "test.txt".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .unwrap();

    // Query by kind - results should be correct
    let result = backend
        .query_nodes_by_kind(sqlitegraph::SnapshotId::current(), "User")
        .unwrap();

    assert_eq!(result.len(), 2, "Should find exactly 2 User nodes");

    // Results are correct; performance is the concern (documented in code comments)
}
