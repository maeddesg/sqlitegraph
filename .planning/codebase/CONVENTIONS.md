# Coding Conventions

**Analysis Date:** 2025-02-11

## Naming Patterns

**Files:**
- `snake_case.rs` for all Rust source files
- `mod.rs` for module directories (re-exports children)
- `lib.rs` for crate root (`sqlitegraph/src/lib.rs`)
- `main.rs` for binary entry points
- Test files: `<module>_tests.rs` or `_<feature>_tests.rs` or descriptive names like `mvcc_baseline_tests.rs`

**Functions:**
- `snake_case` for all function and method names
- Builder-style methods: `with_<field>()` for chainable configuration (e.g., `with_overwrite()`, `with_state()`)
- Predicate functions: `is_<state>()`, `has_<property>()`, `contains_<item>()`
- Getter methods: Direct property access or `get_<item>()`

**Variables:**
- `snake_case` for all variables
- Short names in concise scopes: `id`, `i`, `j`, `n`
- Descriptive names in larger scopes: `node_id`, `entity_ids`, `adjacency_map`

**Types:**
- `PascalCase` for structs, enums, and type aliases
- Newtype wrappers: `PascalCase` wrapping single field (e.g., `NodeId(pub i64)`, `Label(pub String)`)
- Trait names: `PascalCase` (e.g., `GraphBackend`, `ProgressCallback`)
- Type parameters: `'a` for lifetimes, `T` for generic types

**Constants:**
- `SCREAMING_SNAKE_CASE` for compile-time constants
- Prefix SQL constants: `_<purpose>_SQL` (e.g., `OUTGOING_FILTER_SQL`, `INCOMING_FILTER_SQL`)

## Code Style

**Formatting:**
- Standard `rustfmt` formatting (no explicit config in repo - uses defaults)
- Line length: No strict limit enforced, but generally under 100-120 characters
- Indentation: 4 spaces (Rust standard)

**Linting:**
- `#![allow(dead_code)]` present at top of `lib.rs` - indicates work-in-progress code is tolerated
- Compiler warnings generally fixed before commits
- No custom `clippy` lint configuration detected

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
- Re-exports in `lib.rs` for public API
- Feature-gated items: `#[cfg(feature = "native-v2")]` imports

**Grouping:**
- Blank line between std lib, external crates, and internal imports
- Related imports grouped together (e.g., multiple `use crate::` items)

## Error Handling

**Patterns:**
- All public functions return `Result<T, SqliteGraphError>`
- `SqliteGraphError` enum with derived `thiserror::Error`
- Error variants use specific constructors: `.connection()`, `.query()`, `.not_found()`, etc.

```rust
// Error creation pattern
pub fn connection<T: Into<String>>(msg: T) -> Self {
    SqliteGraphError::ConnectionError(msg.into())
}
```

**Context propagation:**
```rust
.map_err(|e| SqliteGraphError::query(e.to_string()))?
```

**Unwrap usage:**
- `.expect()` in tests with descriptive messages
- `.unwrap()` avoided in production code
- `?` operator for error propagation everywhere else

## Logging

**Framework:** `log` crate with feature-gated debug output

**Patterns:**
- `log::debug!()` for development traces
- `debug` feature flag enables verbose output
- Release builds have zero-overhead (feature not enabled)
- No structured logging detected (plain string messages)

**When to log:**
- Significant state transitions (WAL operations, snapshot creation)
- Performance-relevant events (cache hits/misses)
- Errors already surfaced via Result, no redundant error logging

## Comments

**Module-level docs:**
- `//!` style for module documentation
- Comprehensive doc comments at top of every module file
- Include examples in `lib.rs` for public API

**Function docs:**
- `///` triple-slash for public items
- Document arguments with `# Arguments` sections
- Document returns with `# Returns` sections
- Document errors with `# Errors` sections
- Include `# Example` sections for non-trivial usage

**When to Comment:**
- Invariants documented in struct-level docs (e.g., MVCC memory ordering guarantees)
- "Why" comments for non-obvious decisions
- "Phase XX" comments indicate implementation phases in development
- Algorithm explanations inline for complex logic

**JSDoc/TSDoc:**
- Not applicable (Rust project)

## Function Design

**Size:** No strict limit but generally functions under 50 lines preferred

**Parameters:**
- Few parameters: use structs for 3+ related parameters
- Builder pattern for configuration (e.g., `GraphConfig`, `NativeConfig`)
- Reference passing: `&self` for read-only, `&mut self` for mutation
- Lifetime annotations on borrowed data (`'a` common for query objects)

**Return Values:**
- `Result<T, E>` for fallible operations
- `Option<T>` for absent values (not errors)
- `Vec<T>` for collections (not iterators) in public API
- Tuple returns for multiple related values

## Module Design

**Exports:**
- Public API re-exported in `lib.rs`
- Internal modules marked `mod` (not `pub mod`)
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
└── <feature>.rs     # Feature-specific modules
```

## Struct and Enum Conventions

**Structs:**
- Field-level `pub` for data-carrying structs (e.g., `GraphEntity`, `NodeSpec`)
- Builder structs: `with_<field>()` methods return `Self`
- Derive macros: `Debug`, `Clone`, `Copy` (newtype wrappers), `PartialEq`, `Eq`, `Hash`

**Enums:**
- PascalCase variants
- Dataful variants: `VariantName(fields)`
- Error enums derive `thiserror::Error`

**Newtype Pattern:**
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub i64);

impl From<i64> for NodeId { ... }
impl fmt::Display for NodeId { ... }
```

## Trait Conventions

**Naming:**
- Descriptive names: `GraphBackend`, `ProgressCallback`, `SnapshotState`
- Methods: verb phrases (`insert_node`, `get_entity`, `neighbors`)

**Required vs Provided:**
- All methods in traits are public API
- Default implementations provided where sensible
- Documentation required for all trait items

## Documentation Style

**Module Headers:**
```rust
//! Feature/Module description
//!
//! # Section Heading
//!
//! - [`Item1`] - Description
//! - [`Item2`] - Description
//!
//! # Example
//!
//! ```rust,ignore
//! use ...
//! ```
```

**Code Examples:**
- `rust` for compile-tested examples
- `rust,ignore` for pseudo-code or incomplete examples
- `no_run` for examples that shouldn't execute

## Testing Conventions

**Test Organization:**
- Unit tests in `sqlitegraph/tests/` directory
- Integration tests in `tests/` directory
- Test helpers as private functions in test files

**Test Naming:**
- `test_<feature>_scenario` for specific tests
- `test_<feature>_<variant>` for related tests
- Descriptive names preferred over brief ones

---

*Convention analysis: 2025-02-11*
