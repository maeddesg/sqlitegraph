//! Tests for the Cypher-inspired query language parser and executor.

use sqlitegraph::backend::{EdgeSpec, NodeSpec};
use sqlitegraph::cypher::{self, Pattern};
use sqlitegraph::index::add_label;
use sqlitegraph::{GraphBackend, SqliteGraph, SqliteGraphBackend};

/// Build a small test graph: main -> helper -> util (CALLS edges)
/// plus file -(CONTAINS)-> main
fn build_test_graph() -> SqliteGraphBackend {
    let graph = SqliteGraph::open_in_memory().expect("open in-memory");
    let backend = SqliteGraphBackend::from_graph(graph);

    let main_id = backend
        .insert_node(NodeSpec {
            kind: "Function".into(),
            name: "main".into(),
            file_path: None,
            data: serde_json::json!({"lang": "rust"}),
        })
        .expect("insert main");

    let helper_id = backend
        .insert_node(NodeSpec {
            kind: "Function".into(),
            name: "helper".into(),
            file_path: None,
            data: serde_json::json!({"lang": "rust"}),
        })
        .expect("insert helper");

    let util_id = backend
        .insert_node(NodeSpec {
            kind: "Function".into(),
            name: "util".into(),
            file_path: None,
            data: serde_json::json!({"lang": "python"}),
        })
        .expect("insert util");

    let file_id = backend
        .insert_node(NodeSpec {
            kind: "File".into(),
            name: "main.rs".into(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .expect("insert file");

    // Register labels for pattern matching (match_triples uses graph_labels table)
    add_label(backend.graph(), main_id, "Function").expect("label main");
    add_label(backend.graph(), helper_id, "Function").expect("label helper");
    add_label(backend.graph(), util_id, "Function").expect("label util");
    add_label(backend.graph(), file_id, "File").expect("label file");

    backend
        .insert_edge(EdgeSpec {
            from: main_id,
            to: helper_id,
            edge_type: "CALLS".into(),
            data: serde_json::json!({}),
        })
        .expect("insert edge main->helper");

    backend
        .insert_edge(EdgeSpec {
            from: helper_id,
            to: util_id,
            edge_type: "CALLS".into(),
            data: serde_json::json!({}),
        })
        .expect("insert edge helper->util");

    backend
        .insert_edge(EdgeSpec {
            from: file_id,
            to: main_id,
            edge_type: "CONTAINS".into(),
            data: serde_json::json!({}),
        })
        .expect("insert edge file->main");

    backend
}

// ════════════════════════════════════════════════════════════════
// Phase 1: Bug fix tests
// ════════════════════════════════════════════════════════════════

// ── Parser tests (existing) ─────────────────────────────────

#[test]
fn test_parse_node_pattern_no_label() {
    let query = cypher::parse("MATCH (n) RETURN n").expect("parse");
    match &query.pattern {
        Pattern::Node(np) => {
            assert_eq!(np.var, "n");
            assert!(np.label.is_none());
        }
        _ => panic!("expected node pattern"),
    }
    assert_eq!(query.returns, &["n".to_string()]);
}

#[test]
fn test_parse_node_pattern_with_label() {
    let query = cypher::parse("MATCH (n:Function) RETURN n.name").expect("parse");
    match &query.pattern {
        Pattern::Node(np) => {
            assert_eq!(np.var, "n");
            assert_eq!(np.label.as_deref(), Some("Function"));
        }
        _ => panic!("expected node pattern"),
    }
}

#[test]
fn test_parse_node_pattern_with_props() {
    let query = cypher::parse(r#"MATCH (n:Function {lang: "rust"}) RETURN n"#).expect("parse");
    match &query.pattern {
        Pattern::Node(np) => {
            assert_eq!(np.var, "n");
            assert_eq!(np.label.as_deref(), Some("Function"));
            assert_eq!(np.props, vec![("lang".to_string(), "rust".to_string())]);
        }
        _ => panic!("expected node pattern"),
    }
}

#[test]
fn test_parse_edge_pattern_basic() {
    let query = cypher::parse("MATCH (a)-[:CALLS]->(b) RETURN a, b").expect("parse");
    match &query.pattern {
        Pattern::Edge(from, rel, to) => {
            assert_eq!(from.var, "a");
            assert_eq!(rel, "CALLS");
            assert_eq!(to.var, "b");
        }
        _ => panic!("expected edge pattern"),
    }
    assert_eq!(query.returns, &["a".to_string(), "b".to_string()]);
}

#[test]
fn test_parse_edge_pattern_with_labels() {
    let query =
        cypher::parse("MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a.name").expect("parse");
    match &query.pattern {
        Pattern::Edge(from, rel, to) => {
            assert_eq!(from.label.as_deref(), Some("Function"));
            assert_eq!(rel, "CALLS");
            assert_eq!(to.label.as_deref(), Some("Function"));
        }
        _ => panic!("expected edge pattern"),
    }
}

#[test]
fn test_parse_where_clause() {
    let query =
        cypher::parse(r#"MATCH (n:Function) WHERE n.lang = "rust" RETURN n.name"#).expect("parse");
    assert_eq!(query.where_groups.len(), 1);
    assert_eq!(query.where_groups[0].len(), 1);
    assert_eq!(query.where_groups[0][0].var, "n");
    assert_eq!(query.where_groups[0][0].field, "lang");
    assert_eq!(query.where_groups[0][0].value, "rust");
}

#[test]
fn test_parse_limit() {
    let query = cypher::parse("MATCH (n:Function) RETURN n.name LIMIT 10").expect("parse");
    assert_eq!(query.limit, Some(10));
}

#[test]
fn test_parse_where_and_limit() {
    let query =
        cypher::parse(r#"MATCH (a)-[:CALLS]->(b) WHERE b.lang = "rust" RETURN a.name LIMIT 5"#)
            .expect("parse");
    assert!(matches!(query.pattern, Pattern::Edge(_, _, _)));
    assert_eq!(query.where_groups.len(), 1);
    assert_eq!(query.where_groups[0].len(), 1);
    assert_eq!(query.limit, Some(5));
    assert_eq!(query.returns, &["a.name".to_string()]);
}

#[test]
fn test_parse_no_return_defaults_to_star() {
    let query = cypher::parse("MATCH (n:Function)").expect("parse");
    assert_eq!(query.returns, &["*".to_string()]);
}

#[test]
fn test_parse_rejects_unsupported_statement() {
    // Legacy guard: queries that aren't MATCH or CREATE should be rejected.
    // CREATE is now supported (see Phase 3 tests below), so this test uses
    // a genuinely unsupported keyword.
    let result = cypher::parse("DROP TABLE users");
    assert!(result.is_err());
}

// ── Bug fix: WHERE AND splitting ─────────────────────────────

#[test]
fn test_parse_where_multiple_and() {
    let query =
        cypher::parse(r#"MATCH (n:Function) WHERE n.lang = "rust" AND n.name = "main" RETURN n"#)
            .expect("parse");
    // Pure AND: a single OR-group containing two AND-joined predicates.
    assert_eq!(query.where_groups.len(), 1);
    assert_eq!(query.where_groups[0].len(), 2);
    assert_eq!(query.where_groups[0][0].var, "n");
    assert_eq!(query.where_groups[0][0].field, "lang");
    assert_eq!(query.where_groups[0][0].value, "rust");
    assert_eq!(query.where_groups[0][1].var, "n");
    assert_eq!(query.where_groups[0][1].field, "name");
    assert_eq!(query.where_groups[0][1].value, "main");
}

#[test]
fn test_execute_where_and() {
    let backend = build_test_graph();
    let query =
        cypher::parse(r#"MATCH (n:Function) WHERE n.lang = "rust" AND n.name = "main" RETURN n"#)
            .expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");

    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    // Only "main" is both rust and named main
    assert_eq!(results.len(), 1);
}

// ── Bug fix: LIMIT applies after filtering ───────────────────

#[test]
fn test_limit_after_filter() {
    let backend = build_test_graph();
    // Without limit we get 3 functions. With LIMIT 2 we should get exactly 2.
    let query = cypher::parse("MATCH (n:Function) RETURN n.name LIMIT 2").expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 2);

    // With LIMIT 10 we should get all 3 (not capped at 10 candidates)
    let query2 = cypher::parse("MATCH (n:Function) RETURN n.name LIMIT 10").expect("parse");
    let result2 = cypher::execute(&backend, &query2).expect("execute");
    let results2 = result2
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    assert_eq!(results2.len(), 3);
}

// ── Bug fix: Bidirectional edges ─────────────────────────────

#[test]
fn test_parse_backward_edge() {
    let query = cypher::parse("MATCH (a)<-[:CALLS]-(b) RETURN a, b").expect("parse");
    match &query.pattern {
        Pattern::Edge(_from, rel, _to) => {
            // Backward: (a)<-[:CALLS]-(b) means "b calls a", so from=b, to=a
            // but with direction=Incoming
            assert_eq!(rel, "CALLS");
        }
        _ => panic!("expected edge pattern"),
    }
    assert_eq!(query.direction, cypher::EdgeDirection::Incoming);
}

#[test]
fn test_execute_backward_edge() {
    let backend = build_test_graph();
    // util <-[:CALLS]- helper means "who calls util" = helper
    let query = cypher::parse("MATCH (a)<-[:CALLS]-(b) RETURN a.name, b.name").expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    // main->helper, helper->util. Backward CALLS: helper receives from main, util receives from helper = 2
    assert_eq!(results.len(), 2);
}

#[test]
fn test_parse_undirected_edge() {
    let query = cypher::parse("MATCH (a)-[:CALLS]-(b) RETURN a, b").expect("parse");
    assert_eq!(query.direction, cypher::EdgeDirection::Both);
}

// ════════════════════════════════════════════════════════════════
// Phase 2: Multi-hop and variable-depth traversal
// ════════════════════════════════════════════════════════════════

#[test]
fn test_parse_multi_hop() {
    let query = cypher::parse("MATCH (a)-[:CALLS]->(b)-[:CALLS]->(c) RETURN a, c").expect("parse");
    match &query.pattern {
        Pattern::MultiHop(legs) => {
            assert_eq!(legs.len(), 2);
            assert_eq!(legs[0].rel_type, "CALLS");
            assert_eq!(legs[1].rel_type, "CALLS");
        }
        _ => panic!("expected multi-hop pattern, got {:?}", query.pattern),
    }
}

#[test]
fn test_execute_multi_hop() {
    let backend = build_test_graph();
    // main->helper->util: two consecutive CALLS
    let query = cypher::parse("MATCH (a)-[:CALLS]->(b)-[:CALLS]->(c) RETURN a.name, c.name")
        .expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    // Only path: main->helper->util
    assert_eq!(results.len(), 1);
}

#[test]
fn test_parse_variable_depth() {
    let query = cypher::parse("MATCH (a)-[:CALLS*1..2]->(b) RETURN a, b").expect("parse");
    match &query.pattern {
        Pattern::VariableDepth {
            rel_type,
            min_hops,
            max_hops,
        } => {
            assert_eq!(rel_type, "CALLS");
            assert_eq!(*min_hops, 1);
            assert_eq!(*max_hops, 2);
        }
        _ => panic!("expected variable-depth pattern"),
    }
}

#[test]
fn test_execute_variable_depth() {
    let backend = build_test_graph();
    // CALLS*1..2 from main: 1-hop = helper, 2-hop = util
    let query =
        cypher::parse("MATCH (a:Function {name: \"main\"})-[:CALLS*1..2]->(b) RETURN b.name")
            .expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    assert!(results.len() >= 2); // helper (1-hop) + util (2-hop)
}

// ════════════════════════════════════════════════════════════════
// Phase 3: Write operations
// ════════════════════════════════════════════════════════════════

#[test]
fn test_parse_create_node() {
    let query = cypher::parse(r#"CREATE (n:Function {lang: "rust"})"#).expect("parse");
    assert!(matches!(
        query.statement,
        cypher::Statement::CreateNode { .. }
    ));
}

#[test]
fn test_execute_create_node() {
    let backend = build_test_graph();
    let query = cypher::parse(r#"CREATE (n:TestNode {key: "val"})"#).expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    // Should return the created node ID
    assert!(result.get("id").is_some());
    let id = result.get("id").unwrap().as_i64().expect("id is i64");
    assert!(id > 0);
}

#[test]
fn test_parse_create_edge() {
    let query = cypher::parse("CREATE (1)-[:RELATES]->(2)").expect("parse");
    assert!(matches!(
        query.statement,
        cypher::Statement::CreateEdge { .. }
    ));
}

#[test]
fn test_execute_create_edge() {
    let backend = build_test_graph();
    let query = cypher::parse("CREATE (1)-[:TEST_REL]->(2)").expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    assert!(result.get("id").is_some());
}

#[test]
fn test_parse_delete() {
    let query = cypher::parse("MATCH (n) WHERE n.name = \"util\" DELETE n").expect("parse");
    assert!(matches!(query.statement, cypher::Statement::Delete { .. }));
}

#[test]
fn test_execute_delete() {
    let backend = build_test_graph();
    let query = cypher::parse(r#"MATCH (n) WHERE n.name = "util" DELETE n"#).expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    assert_eq!(result.get("deleted").unwrap().as_u64().unwrap(), 1);
}

#[test]
fn test_parse_set() {
    let query =
        cypher::parse(r#"MATCH (n) WHERE n.name = "main" SET n.lang = "cpp""#).expect("parse");
    assert!(matches!(query.statement, cypher::Statement::Set { .. }));
}

#[test]
fn test_execute_set() {
    let backend = build_test_graph();
    let query =
        cypher::parse(r#"MATCH (n) WHERE n.name = "main" SET n.lang = "cpp""#).expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    assert_eq!(result.get("updated").unwrap().as_u64().unwrap(), 1);
}

// ════════════════════════════════════════════════════════════════
// Phase 4: Name pattern and advanced WHERE
// ════════════════════════════════════════════════════════════════

#[test]
fn test_parse_where_regex() {
    let query = cypher::parse(r#"MATCH (n) WHERE n.name =~ "ma.*" RETURN n"#).expect("parse");
    assert_eq!(query.where_groups.len(), 1);
    assert_eq!(query.where_groups[0].len(), 1);
    assert_eq!(query.where_groups[0][0].field, "name");
    assert_eq!(query.where_groups[0][0].operator, cypher::WhereOp::Regex);
    assert_eq!(query.where_groups[0][0].value, "ma.*");
}

#[test]
fn test_execute_where_regex() {
    let backend = build_test_graph();
    let query =
        cypher::parse(r#"MATCH (n:Function) WHERE n.name =~ "ma.*" RETURN n.name"#).expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    // Should match "main" only
    assert_eq!(results.len(), 1);
}

#[test]
fn test_parse_where_numeric_comparison() {
    let query = cypher::parse(r#"MATCH (n) WHERE n.count > 5 RETURN n"#).expect("parse");
    assert_eq!(
        query.where_groups[0][0].operator,
        cypher::WhereOp::GreaterThan
    );
    assert_eq!(query.where_groups[0][0].value, "5");
}

#[test]
fn test_parse_where_or() {
    let query = cypher::parse(r#"MATCH (n) WHERE n.name = "main" OR n.name = "util" RETURN n"#)
        .expect("parse");
    // Pure OR: two OR-groups, each containing one predicate.
    assert_eq!(query.where_groups.len(), 2);
    assert_eq!(query.where_groups[0].len(), 1);
    assert_eq!(query.where_groups[1].len(), 1);
    assert_eq!(query.where_groups[0][0].value, "main");
    assert_eq!(query.where_groups[1][0].value, "util");
}

// ── Mixed AND/OR precedence (OR binds looser than AND) ───────

#[test]
fn test_parse_where_and_or_precedence() {
    // `a AND b OR c` → (a AND b) OR c  →  [[a, b], [c]]
    let query = cypher::parse(
        r#"MATCH (n) WHERE n.lang = "rust" AND n.name = "main" OR n.name = "util" RETURN n"#,
    )
    .expect("parse");
    assert_eq!(query.where_groups.len(), 2);
    assert_eq!(query.where_groups[0].len(), 2);
    assert_eq!(query.where_groups[0][0].value, "rust");
    assert_eq!(query.where_groups[0][1].value, "main");
    assert_eq!(query.where_groups[1].len(), 1);
    assert_eq!(query.where_groups[1][0].value, "util");
}

#[test]
fn test_parse_where_or_and_precedence() {
    // `a OR b AND c` → a OR (b AND c)  →  [[a], [b, c]]
    let query = cypher::parse(
        r#"MATCH (n) WHERE n.name = "util" OR n.lang = "rust" AND n.name = "main" RETURN n"#,
    )
    .expect("parse");
    assert_eq!(query.where_groups.len(), 2);
    assert_eq!(query.where_groups[0].len(), 1);
    assert_eq!(query.where_groups[0][0].value, "util");
    assert_eq!(query.where_groups[1].len(), 2);
    assert_eq!(query.where_groups[1][0].value, "rust");
    assert_eq!(query.where_groups[1][1].value, "main");
}

#[test]
fn test_execute_where_and_or_precedence() {
    let backend = build_test_graph();
    // Graph: main(rust), helper(rust), util(python), main.rs(File).
    // Predicate: (lang = "rust" AND name = "main") OR name = "util"
    // Matches: main (first group), util (second group). Total 2.
    let query = cypher::parse(
        r#"MATCH (n:Function) WHERE n.lang = "rust" AND n.name = "main" OR n.name = "util" RETURN n.name"#,
    )
    .expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    let results = result.get("results").expect("results").as_array().expect("array");
    assert_eq!(results.len(), 2);
    let names: Vec<&str> = results
        .iter()
        .filter_map(|r| r.get("n.name").and_then(|v| v.as_str()))
        .collect();
    assert!(names.contains(&"main"));
    assert!(names.contains(&"util"));
}

// ── Star patterns: comma-separated legs sharing a root variable ──

/// Build a star test graph.
///
/// Layout (ids assigned in insert order):
/// - `root` (Hub, id 1) — central node
/// - `a` (Thing, id 2), `b` (Thing, id 3) — both OWNed
/// - `c` (Thing, id 4) — LIKED only
///
/// Edges:
/// - root -OWNS-> a
/// - root -OWNS-> b
/// - root -LIKES-> c
/// - root -TAGS-> a
fn build_star_graph() -> SqliteGraphBackend {
    let graph = SqliteGraph::open_in_memory().expect("open in-memory");
    let backend = SqliteGraphBackend::from_graph(graph);

    let root = backend
        .insert_node(NodeSpec {
            kind: "Hub".into(),
            name: "root".into(),
            file_path: None,
            data: serde_json::json!({}),
        })
        .expect("insert root");
    let a = backend
        .insert_node(NodeSpec {
            kind: "Thing".into(),
            name: "a".into(),
            file_path: None,
            data: serde_json::json!({"colour": "red"}),
        })
        .expect("insert a");
    let b = backend
        .insert_node(NodeSpec {
            kind: "Thing".into(),
            name: "b".into(),
            file_path: None,
            data: serde_json::json!({"colour": "blue"}),
        })
        .expect("insert b");
    let c = backend
        .insert_node(NodeSpec {
            kind: "Thing".into(),
            name: "c".into(),
            file_path: None,
            data: serde_json::json!({"colour": "green"}),
        })
        .expect("insert c");

    add_label(backend.graph(), root, "Hub").expect("label root");
    add_label(backend.graph(), a, "Thing").expect("label a");
    add_label(backend.graph(), b, "Thing").expect("label b");
    add_label(backend.graph(), c, "Thing").expect("label c");

    for (from, to, kind) in &[
        (root, a, "OWNS"),
        (root, b, "OWNS"),
        (root, c, "LIKES"),
        (root, a, "TAGS"),
    ] {
        backend
            .insert_edge(EdgeSpec {
                from: *from,
                to: *to,
                edge_type: (*kind).into(),
                data: serde_json::json!({}),
            })
            .expect("insert edge");
    }

    backend
}

#[test]
fn test_parse_star_two_legs() {
    let query = cypher::parse("MATCH (r)-[:OWNS]->(x), (r)-[:LIKES]->(y) RETURN r, x, y")
        .expect("parse star");
    match &query.pattern {
        Pattern::Star { legs } => {
            assert_eq!(legs.len(), 2);
            assert_eq!(legs[0].rel_type, "OWNS");
            assert_eq!(legs[0].from.var, "r");
            assert_eq!(legs[0].to.var, "x");
            assert_eq!(legs[1].rel_type, "LIKES");
            assert_eq!(legs[1].from.var, "r");
            assert_eq!(legs[1].to.var, "y");
        }
        other => panic!("expected Pattern::Star, got {other:?}"),
    }
}

#[test]
fn test_parse_star_three_legs() {
    let query =
        cypher::parse("MATCH (r)-[:OWNS]->(x), (r)-[:LIKES]->(y), (r)-[:TAGS]->(z) RETURN r")
            .expect("parse three-leg star");
    match &query.pattern {
        Pattern::Star { legs } => {
            assert_eq!(legs.len(), 3);
        }
        other => panic!("expected Pattern::Star, got {other:?}"),
    }
}

#[test]
fn test_parse_star_root_var_mismatch_rejected() {
    // Legs that don't share a root variable aren't true stars — for now we
    // require all legs to start from the same var.
    let result = cypher::parse("MATCH (a)-[:X]->(b), (c)-[:Y]->(d) RETURN a, b, c, d");
    assert!(
        result.is_err(),
        "expected parse error for non-shared root, got {result:?}"
    );
}

#[test]
fn test_execute_star_two_legs() {
    let backend = build_star_graph();
    // OWNS → {a, b}; LIKES → {c}. Cartesian product: (root, a, c), (root, b, c).
    let query = cypher::parse("MATCH (r)-[:OWNS]->(x), (r)-[:LIKES]->(y) RETURN r.name, x.name, y.name")
        .expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    let results = result
        .get("results")
        .expect("results")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 2);

    // Each result must have r=root, y=c, and x ∈ {a, b}.
    let mut xs: Vec<&str> = results
        .iter()
        .filter_map(|r| r.get("x.name").and_then(|v| v.as_str()))
        .collect();
    xs.sort();
    assert_eq!(xs, vec!["a", "b"]);
    for row in results {
        assert_eq!(row.get("r.name").and_then(|v| v.as_str()), Some("root"));
        assert_eq!(row.get("y.name").and_then(|v| v.as_str()), Some("c"));
    }
}

#[test]
fn test_execute_star_with_where() {
    let backend = build_star_graph();
    // OWNS → {a, b}; restrict x.colour = "red" → only a survives. LIKES → c.
    let query = cypher::parse(
        r#"MATCH (r)-[:OWNS]->(x), (r)-[:LIKES]->(y) WHERE x.colour = "red" RETURN x.name, y.name"#,
    )
    .expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    let results = result
        .get("results")
        .expect("results")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("x.name").and_then(|v| v.as_str()),
        Some("a")
    );
    assert_eq!(
        results[0].get("y.name").and_then(|v| v.as_str()),
        Some("c")
    );
}

#[test]
fn test_execute_star_three_legs_shared_root() {
    let backend = build_star_graph();
    // OWNS → {a, b}; LIKES → {c}; TAGS → {a}.
    // Cartesian: 2 × 1 × 1 = 2 rows. Each has y=c, z=a, x ∈ {a, b}.
    let query =
        cypher::parse("MATCH (r)-[:OWNS]->(x), (r)-[:LIKES]->(y), (r)-[:TAGS]->(z) RETURN x.name, y.name, z.name")
            .expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");
    let results = result
        .get("results")
        .expect("results")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 2);
    for row in results {
        assert_eq!(row.get("y.name").and_then(|v| v.as_str()), Some("c"));
        assert_eq!(row.get("z.name").and_then(|v| v.as_str()), Some("a"));
    }
}

// ── Existing executor tests ─────────────────────────────────

#[test]
fn test_execute_node_pattern() {
    let backend = build_test_graph();
    let query = cypher::parse("MATCH (n:Function) RETURN n.name").expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");

    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 3);
}

#[test]
fn test_execute_node_pattern_with_prop_filter() {
    let backend = build_test_graph();
    let query = cypher::parse(r#"MATCH (n:Function {lang: "rust"}) RETURN n.name"#).expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");

    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 2);
}

#[test]
fn test_execute_edge_pattern() {
    let backend = build_test_graph();
    let query = cypher::parse("MATCH (a)-[:CALLS]->(b) RETURN a, b").expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");

    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 2);
}

#[test]
fn test_execute_edge_pattern_with_label_filter() {
    let backend = build_test_graph();
    let query = cypher::parse("MATCH (a:Function)-[:CALLS]->(b:Function) RETURN *").expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");

    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 2);
}

#[test]
fn test_execute_with_where() {
    let backend = build_test_graph();
    let query = cypher::parse(r#"MATCH (n:Function) WHERE n.lang = "python" RETURN n.name"#)
        .expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");

    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_execute_with_limit() {
    let backend = build_test_graph();
    let query = cypher::parse("MATCH (n:Function) RETURN n.name LIMIT 2").expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");

    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 2);
}

#[test]
fn test_execute_edge_with_where() {
    let backend = build_test_graph();
    let query = cypher::parse(r#"MATCH (a)-[:CALLS]->(b) WHERE b.lang = "rust" RETURN a.name"#)
        .expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");

    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_execute_contains_edge() {
    let backend = build_test_graph();
    let query = cypher::parse("MATCH (f)-[:CONTAINS]->(n) RETURN f, n").expect("parse");
    let result = cypher::execute(&backend, &query).expect("execute");

    let results = result
        .get("results")
        .expect("results key")
        .as_array()
        .expect("array");
    assert_eq!(results.len(), 1);
}
