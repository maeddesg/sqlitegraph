//! SQLite-specific backend implementation for GraphBackend trait.
//!
//! This module provides the concrete SQLite backend implementation that wraps
//! SqliteGraph and implements the GraphBackend trait. It includes the main
//! SqliteGraphBackend type and all supporting types needed for SQLite operations.

// Module organization
mod impl_;
pub mod types;

#[cfg(all(test, feature = "native-v2"))]
mod kv_tests;

#[cfg(test)]
mod pubsub_tests;

// Re-export the main backend implementation
pub use impl_::SqliteGraphBackend;

// Re-export public types for external use
#[allow(unused_imports)] // Future extension points for SQLite backend API
pub use types::{BackendDirection, EdgeSpec, NeighborQuery, NodeSpec};
