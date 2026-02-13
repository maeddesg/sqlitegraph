//! Simple Cypher-like query parser
//!
//! Supports basic patterns:
//! - MATCH (n:Label) RETURN n.name
//! - MATCH (n:Label {key: "value"}) RETURN n
//! - MATCH (a)-[:REL]->(b) RETURN a, b

use serde_json::Value;
use sqlitegraph::backend::GraphBackend;
use sqlitegraph::graph::GraphEntity;
use sqlitegraph::snapshot::SnapshotId;

#[derive(Debug)]
pub enum Query {
    Match {
        pattern: Pattern,
        returns: Vec<String>,
    },
}

#[derive(Debug)]
pub enum Pattern {
    Node(NodePattern),
    Edge(NodePattern, String, NodePattern), // from, rel_type, to
}

#[derive(Debug)]
pub struct NodePattern {
    pub var: String,
    pub label: Option<String>,
    pub props: Vec<(String, String)>,
}

impl NodePattern {
    fn matches(&self, node: &GraphEntity) -> bool {
        // Check label (kind)
        if let Some(ref label) = self.label {
            if node.kind != *label {
                return false;
            }
        }
        // Check properties
        for (key, value) in &self.props {
            match node.data.get(key) {
                Some(v) if v.as_str() == Some(value) => continue,
                _ => return false,
            }
        }
        true
    }
}

/// Parse a simple Cypher-like query
pub fn parse(query: &str) -> anyhow::Result<Query> {
    let query = query.trim();

    if query.to_uppercase().starts_with("MATCH ") {
        parse_match(query)
    } else {
        anyhow::bail!("Only MATCH queries are supported")
    }
}

fn parse_match(query: &str) -> anyhow::Result<Query> {
    // Remove MATCH keyword
    let rest = query[6..].trim();

    // Find RETURN clause
    let return_pos = rest.to_uppercase().find(" RETURN ");
    let (pattern_str, returns) = if let Some(pos) = return_pos {
        let pattern_part = &rest[..pos];
        let return_part = &rest[pos + 8..];
        let returns: Vec<String> = return_part.split(',').map(|s| s.trim().to_string()).collect();
        (pattern_part.trim(), returns)
    } else {
        (rest, vec!["*".to_string()])
    };

    let pattern = parse_pattern(pattern_str)?;

    Ok(Query::Match { pattern, returns })
}

fn parse_pattern(s: &str) -> anyhow::Result<Pattern> {
    let s = s.trim();

    // Check for edge pattern: (a)-[:REL]->(b)
    if s.contains("-") && s.contains("->") {
        parse_edge_pattern(s)
    } else if s.starts_with('(') && s.ends_with(')') {
        // Node pattern
        let inner = &s[1..s.len() - 1];
        let node = parse_node(inner)?;
        Ok(Pattern::Node(node))
    } else {
        anyhow::bail!("Invalid pattern syntax")
    }
}

fn parse_edge_pattern(_s: &str) -> anyhow::Result<Pattern> {
    anyhow::bail!("Edge patterns not yet implemented in parser")
}

fn parse_node(s: &str) -> anyhow::Result<NodePattern> {
    let s = s.trim();

    // Parse variable name
    let var_end = s.find(|c: char| c == ':' || c == '{' || c.is_whitespace());
    let var = if let Some(end) = var_end {
        s[..end].trim().to_string()
    } else {
        s.to_string()
    };

    // Parse label if present
    let label = if let Some(colon_pos) = s.find(':') {
        let after_colon = &s[colon_pos + 1..];
        let label_end = after_colon.find(|c: char| c == '{' || c.is_whitespace());
        if let Some(end) = label_end {
            Some(after_colon[..end].trim().to_string())
        } else {
            Some(after_colon.trim().to_string())
        }
    } else {
        None
    };

    // Parse properties if present
    let mut props = Vec::new();
    if let Some(open_brace) = s.find('{') {
        if let Some(close_brace) = s.rfind('}') {
            let props_str = &s[open_brace + 1..close_brace];
            // Simple key: "value" parsing
            for part in props_str.split(',') {
                let part = part.trim();
                if let Some(colon_pos) = part.find(':') {
                    let key = part[..colon_pos].trim().to_string();
                    let value = part[colon_pos + 1..]
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                    props.push((key, value));
                }
            }
        }
    }

    Ok(NodePattern { var, label, props })
}

/// Execute a query against the backend
pub fn execute(backend: &dyn GraphBackend, query: &Query) -> anyhow::Result<Value> {
    match query {
        Query::Match { pattern, returns } => execute_match(backend, pattern, returns),
    }
}

fn execute_match(
    backend: &dyn GraphBackend,
    pattern: &Pattern,
    returns: &[String],
) -> anyhow::Result<Value> {
    match pattern {
        Pattern::Node(node_pat) => {
            let snapshot = SnapshotId::current();
            let node_ids = backend.entity_ids()?;

            let mut results = Vec::new();
            for id in node_ids.iter().take(1000) {
                // Limit to 1000 results
                if let Ok(node) = backend.get_node(snapshot, *id) {
                    if node_pat.matches(&node) {
                        let mut obj = serde_json::Map::new();

                        for ret in returns {
                            if ret == "*" || *ret == node_pat.var {
                                obj.insert(
                                    node_pat.var.clone(),
                                    serde_json::json!({
                                        "id": node.id,
                                        "kind": node.kind,
                                        "name": node.name,
                                        "data": node.data,
                                    }),
                                );
                            } else if ret.starts_with(&format!("{}.", node_pat.var)) {
                                let field = &ret[node_pat.var.len() + 1..];
                                let value = match field {
                                    "id" => serde_json::json!(node.id),
                                    "kind" => serde_json::json!(node.kind),
                                    "name" => serde_json::json!(node.name),
                                    _ => node.data.get(field).cloned().unwrap_or(Value::Null),
                                };
                                obj.insert(ret.clone(), value);
                            }
                        }

                        if !obj.is_empty() {
                            results.push(Value::Object(obj));
                        }
                    }
                }
            }

            Ok(serde_json::json!({
                "results": results,
                "count": results.len(),
            }))
        }
        Pattern::Edge(_, _, _) => {
            anyhow::bail!("Edge pattern queries not yet implemented")
        }
    }
}
