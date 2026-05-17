//! CLI query module — delegates to core cypher module.
//!
//! This module provides the CLI-specific interface for parsing and executing
//! Cypher-inspired queries. The actual parser and executor live in
//! `sqlitegraph::cypher`.

use serde_json::Value;
use sqlitegraph::backend::SqliteGraphBackend;

/// Parse and execute a Cypher-inspired query string.
///
/// Delegates to `sqlitegraph::cypher::parse` and `sqlitegraph::cypher::execute`.
/// Returns a JSON value with `{"results": [...], "count": N}`.
pub fn run(backend: &SqliteGraphBackend, query_str: &str) -> anyhow::Result<Value> {
    let query =
        sqlitegraph::cypher::parse(query_str).map_err(|e| anyhow::anyhow!("parse error: {e}"))?;
    let result = sqlitegraph::cypher::execute(backend, &query)
        .map_err(|e| anyhow::anyhow!("execution error: {e}"))?;
    Ok(result)
}
