# Coding Conventions

**Analysis Date:** 2025-02-12

## Naming Patterns

**Files:**
- `snake_case.rs` for all Rust source files
- `mod.rs` for module directories (re-exports children)
- `lib.rs` for crate root (`sqlitegraph/src/lib.rs`)
- `main.rs` for binary entry points
- Test files: `<module>_tests.rs`, `_<feature>_tests.rs`, descriptive names like `snapshot_invariants_tests.rs`, `header_architecture_regression_tests.rs`

**Functions:**
- `snake_case` for all function and method names
- Builder-style methods: `with_<field>()` for chainable configuration (e.g., `with_overwrite()`, `with_wal_mode()`, `with_cpu_profile()`)
- Predicate functions: `is_<state>()`, `has_<property>()`, `contains_<item>()`
- Getter methods: Direct property access or `get_<item>()`
- Public API functions use descriptive verbs: `insert_node`, `bulk_insert_entities`, `neighbors_of`

**Variables:**
- `snake_case` for all variables
- Short names in concise scopes: `id`, `i`, `j`, `n`
- Descriptive names in larger scopes: `node_id`, `entity_ids`, `adjacency_map`

**Types:**
- `PascalCase` for structs, enums, and type aliases
- Newtype wrappers: `PascalCase` wrapping single field (e.g., `NodeId(pub i64)`, `Label(pub String)`, `EdgeId(pub i64)`)
- Trait names: `PascalCase` (e.g., `GraphBackend`, `ProgressCallback`, `VectorStorage`)
- Type aliases: `SnapshotId`, `NativeResult`, `NativeNodeId`

**Constants:**
- `SCREAMING_SNAKE_CASE` for compile-time constants
- Examples: `HEADER_SIZE`, `NODE_SLOT_SIZE`, `V2_MAGIC`, `EDGE_SLOT_SIZE`
- Prefix SQL constants: `_<purpose>_SQL` (e.g., `OUTGOING_FILTER_SQL`)

## Code Style

**Formatting:**
- Standard `rustfmt` formatting (no explicit config - uses defaults)
- Line length: No strict limit, but generally under 100-120 characters
- Indentation: 4 spaces (Rust standard)

**Linting:**
- `#![allow(dead_code)]` present at top of `lib.rs` - indicates work-in-progress code tolerated
- Compiler warnings generally fixed before commits
- No custom `clippy` lint configuration detected

**Attributes:**
- `#[derive(Debug, Clone, Copy)]` for small copy types
- `#[derive(Debug, Clone)]` for types with heap data
- `#[cfg(test)]` for test-only code
- `#[cfg(feature = "...")]` for feature-gated implementations
- `#[serde(rename = "...")]` for field name mapping

**Brace Style:**
- Opening braces on same line for functions, structs, match arms
- Closing brace on separate line
- Single-line expressions omit braces where idiomatic

## Import Organization

**Order:**
1. Standard library imports (`use std::...`)
2. External crate imports (`use rusqlite::...`, `use serde::...`)
3. Internal crate imports (`use crate::...`)
4. Module declarations and re-exports

**Path Aliases:**
- `crate::<module>` for internal absolute paths
- External crates imported by root name
- Re-exports in `lib.rs` for clean public API
- Feature-gated items: `#[cfg(feature = "native-v2")]` imports

**Grouping:**
- Blank line between std lib, external crates, and internal imports
- Related imports grouped together (e.g., multiple `use crate::` items)

**Example:**
```rust
// Standard library
use std::collections::HashMap;
use std::sync::Arc;

// Third-party
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tempfile::TempDir;

// Crate
use crate::{
    backend::GraphBackend,
    errors::SqliteGraphError,
    graph::SqliteGraph,
};
```

## Error Handling

**Patterns:**
- All public functions return `Result<T, SqliteGraphError>`
- Centralized error enum in `sqlitegraph/src/errors.rs`
- Uses `thiserror` crate for error derivation

**Error Type Structure:**
```rust
#[derive(Debug, Error)]
pub enum SqliteGraphError {
    #[error("connection error: {0}")]
    ConnectionError(String),
    #[error("schema error: {0}")]
    SchemaError(String),
    #[error("query error: {0}")]
    QueryError(String),
    #[error("entity not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("fault injected: {0}")]
    FaultInjected(String),
    #[error("native backend error: {0}")]
    NativeError(#[from] crate::backend::native::types::NativeBackendError),
}
```

**Constructor Methods:**
- Each error variant has constructor: `.connection()`, `.schema()`, `.query()`, `.not_found()`
- Generic `Into<String>` for flexible message types

**Propagation:**
- `?` operator for error propagation
- `#[from]` attribute for automatic conversion
- Explicit `map_err()` for context addition: `.map_err(|e| SqliteGraphError::query(e.to_string()))?`

**Unwrap Usage:**
- `.unwrap()` and `.expect()` used only in tests with descriptive messages
- Production code returns proper `Result` types
- Temporary files use `.expect()` with descriptive messages like `"Failed to create temp dir"`

## Logging

**Framework:** `log` crate with feature-gated debug output

**Levels:**
- `log::debug!()` for development traces
- Debug/tracing controlled via `debug` feature
- Release builds have zero-overhead (logging disabled)
- No structured logging detected (plain string messages)

**Patterns:**
- Significant state transitions (WAL operations, snapshot creation)
- Performance-relevant events (cache hits/misses)
- Errors already surfaced via Result, no redundant error logging
- `println!()` used only in test/benchmark code

## Comments

**Module-level docs:**
- `//!` style for module documentation
- Comprehensive doc comments at top of every module file
- Include architecture, invariants, guarantees, and usage examples

**Function docs:**
- `///` triple-slash for public items
- Document arguments with `# Arguments` sections
- Document returns with `# Returns` sections
- Document errors with `# Errors` sections
- Include `# Example` code blocks where applicable

**Inline Comments:**
- Used for complex algorithm explanations
- Memory ordering guarantees documented (e.g., in `mvcc.rs`)
- Invariants marked with explicit comments
- "Why" comments for non-obvious decisions
- "Phase XX" comments indicate implementation phases in development

**Documentation Style:**
```rust
//! Module/Feature description
//!
//! # Section Heading
//!
//! - [`Item1`] - Description
//! - [`Item2`] - Description
//!
//! # Example
//!
//! ```rust,ignore
//! use crate_name::Item;
//! let item = Item::new();
//! ```
```

**Code Examples:**
- `rust` for compile-tested examples
- `rust,ignore` for pseudo-code or incomplete examples
- `no_run` for examples that shouldn't execute

## Function Design

**Size:**
- No strict limit but generally functions under 50 lines preferred
- Complex algorithms split into helper functions
- Large test functions (100+ lines) acceptable for invariants testing

**Parameters:**
- Few parameters: use structs for 3+ related parameters
- Builder pattern for configuration (e.g., `GraphConfig`, `HnswConfig`, `SnapshotExportConfig`)
- Fluent method chaining: `.with_wal_mode().with_cache_size(1000).with_overwrite(true)`
- Reference passing: `&self` for read-only, `&mut self` for mutation
- Lifetime annotations on borrowed data (`'a` common for query objects)
- Slice parameters for collections: `&[&str]`, `&[NodeId]`

**Return Values:**
- `Result<T, E>` for fallible operations
- `Option<T>` for absent values (not errors)
- `Vec<T>` for collections (not iterators) in public API
- Tuple returns for multiple related values: `(usize, usize)` for `(in_degree, out_degree)`
- Custom result types for complex returns: `CycleBasisResult`, `PartitionResult`, `SnapshotExportResult`

## Module Design

**Exports:**
- Public API re-exported in `lib.rs`
- Internal modules marked `mod` (not `pub mod`)
- Feature-gated exports: `#[cfg(feature = "native-v2")] pub use ...;`
- Test-only modules: `pub mod <name> // Public for tests`

**Barrel Files:**
- `mod.rs` re-exports commonly used items
- Selective re-exports for public API surface
- Private items kept private in submodules

**Visibility:**
- Default private, `pub` for API
- `pub(crate)` for crate-wide internal sharing
- Test helpers: private functions in test files

**Module Organization:**
```
sqlitegraph/src/
├── lib.rs           # Public API re-exports
├── backend.rs        # Backend trait and routing
├── graph/mod.rs     # Core graph with submodules
├── algo/mod.rs      # Algorithms library
├── mvcc.rs          # MVCC snapshot system
├── cache.rs         # LRU-K adjacency cache
├── hnsw/mod.rs     # Vector search
└── <feature>.rs     # Feature-specific modules
```

## Struct and Enum Conventions

**Structs:**
- Field-level `pub` for data-carrying structs (e.g., `GraphEntity`, `NodeSpec`, `EdgeSpec`)
- Builder structs: `with_<field>()` methods return `Self`
- Derive macros: `Debug`, `Clone`, `Copy` (newtype wrappers), `PartialEq`, `Eq`, `Hash`
- `#[serde(...)]` attributes for serialization

**Enums:**
- `PascalCase` variants
- Dataful variants: `VariantName(fields)`
- Error enums derive `thiserror::Error`

**Newtype Pattern:**
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub i64);

impl From<i64> for NodeId {
    fn from(value: i64) -> Self {
        NodeId(value)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
```

## Trait Conventions

**Naming:**
- Descriptive names: `GraphBackend`, `ProgressCallback`, `SnapshotState`, `VectorStorage`
- Methods: verb phrases (`insert_node`, `get_entity`, `neighbors`)

**Required vs Provided:**
- All methods in traits are public API
- Default implementations provided where sensible
- Documentation required for all trait items

## Serialization Patterns

**Serde:**
- `#[derive(Serialize, Deserialize)]` from `serde` for data types
- `#[serde(rename = "...")]` for field name mapping
- JSON serialization via `serde_json::to_string()` and `json!()` macro

**Binary Formats:**
- `bincode` for compact binary encoding
- `binrw` crate for native backend file formats (`#[derive(BinRead, BinWrite)]`)
- `bytemuck` for zero-copy casts in memory-mapped I/O (`#[derive(Pod, Zeroable)]`)

## Concurrency Patterns

**Interior Mutability:**
- `RefCell` for single-threaded interior mutability (`SqliteGraph`)
- `ArcSwap` for lock-free atomic updates (`SnapshotManager`)
- `parking_lot` for efficient mutexes and RwLocks

**Thread Safety:**
- Types explicitly marked as NOT thread-safe when applicable
- `Sync` not implemented for graph with `RefCell`
- Use `GraphSnapshot` for concurrent reads
- Writes must be serialized

**Atomic Operations:**
- `ArcSwap` provides Acquire/Release memory ordering
- Documented in `mvcc.rs` with happens-before guarantees
- Comments explain memory ordering: `// Load uses Acquire ordering`, `// Store uses Release ordering`

## Unsafe Code Guidelines

**Usage:**
- Minimal unsafe code (primarily in FFI/low-level I/O)
- `unsafe` blocks clearly documented with safety invariants
- Memory-mapped I/O in backend modules uses unsafe
- Comments with `// SAFETY:` explaining why unsafe code is correct

**Documentation:**
- Safety comments required: `// SAFETY: ...`
- Explains why unsafe code is correct
- Documents preconditions that must hold
- Documents behavior with `bytemuck` zero-copy operations

---

*Convention analysis: 2025-02-12*
