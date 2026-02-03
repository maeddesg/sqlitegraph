# Contributing to SQLiteGraph

**Last Updated:** 2026-02-03
**Version:** v1.4.2

This document provides guidelines for contributing to SQLiteGraph.

---

## Table of Contents

1. [Development Setup](#development-setup)
2. [Code Standards](#code-standards)
3. [Development Workflow](#development-workflow)
4. [Testing Requirements](#testing-requirements)
5. [Documentation Requirements](#documentation-requirements)
6. [Submitting Changes](#submitting-changes)

---

## Development Setup

### Prerequisites

- Rust 1.75 or later
- Git
- SQLite 3 (for SQLite backend development)

### Clone and Build

```bash
# Clone repository
git clone https://github.com/yourusername/sqlitegraph.git
cd sqlitegraph

# Build workspace
cargo build --workspace

# Run tests
cargo test --workspace

# Run with Native V2 feature
cargo build --workspace --features native-v2
```

### Development Tools

```bash
# Install development tools
cargo install cargo-watch
cargo install cargo-edit
cargo install flamegraph

# Optional: Install tarpaulin for coverage
cargo install cargo-tarpaulin
```

### IDE Setup

**Recommended: VS Code with rust-analyzer**

```json
// .vscode/settings.json
{
    "rust-analyzer.cargo.features": "all",
    "rust-analyzer.checkOnSave.command": "clippy",
    "rust-analyzer.cargo.loadOutDirsFromCheck": true
}
```

---

## Code Standards

### Rust Style

Follow standard Rust style:

```bash
# Format code
cargo fmt --all

# Check formatting
cargo fmt --all -- --check
```

### Linting

```bash
# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Auto-fix some issues
cargo clippy --fix --allow-dirty --allow-staged
```

### Code Quality Rules

1. **No unwrap() in production code**
   ```rust
   // Bad
   let node = graph.get_node(id).unwrap();

   // Good
   let node = graph.get_node(id)?;
   // or
   let node = graph.get_node(id).expect("node should exist");
   ```

2. **Proper error handling**
   ```rust
   use sqlitegraph::SqliteGraphError;

   pub fn insert_node(&self, spec: NodeSpec) -> Result<u64, SqliteGraphError> {
       if spec.kind.is_empty() {
           return Err(SqliteGraphError::ValidationError("kind cannot be empty".into()));
       }
       // ...
   }
   ```

3. **Document public APIs**
   ```rust
   /// Inserts a new node into the graph.
   ///
   /// # Arguments
   ///
   /// * `spec` - Node specification with kind, name, and data
   ///
   /// # Returns
   ///
   /// The ID of the newly created node.
   ///
   /// # Errors
   ///
   /// Returns `ValidationError` if kind or name is empty.
   ///
   /// # Examples
   ///
   /// ```rust
   /// let spec = NodeSpec {
   ///     kind: "User".to_string(),
   ///     name: "Alice".to_string(),
   ///     file_path: None,
   ///     data: json!({"age": 30}),
   /// };
   /// let id = graph.insert_node(spec)?;
   /// ```
   pub fn insert_node(&self, spec: NodeSpec) -> Result<u64, SqliteGraphError>;
   ```

4. **Module size limit**
   - Maximum 300 LOC per module (600 with justification)
   - Split large modules into submodules

5. **No state artifacts in src/**
   - No test databases in source directories
   - Use `tempfile::TempDir` for test data

---

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/issue-description
```

### 2. Make Changes

Following the TDD workflow:

```rust
// 1. Write failing test
#[test]
fn test_new_feature() {
    let graph = create_test_graph().unwrap();
    let result = graph.new_feature();
    assert!(result.is_ok());
}

// 2. Run test - should FAIL
// cargo test test_new_feature

// 3. Implement feature
impl GraphBackend for MyBackend {
    fn new_feature(&self) -> Result<()> {
        // implementation
    }
}

// 4. Run test - should PASS
// cargo test test_new_feature
```

### 3. Run Full Test Suite

```bash
# All tests
cargo test --workspace --features native-v2

# Integration tests
cargo test --test api_ergonomics_tests

# Documentation tests
cargo test --doc

# Clippy
cargo clippy --all-targets --all-features -- -D warnings

# Format check
cargo fmt --all -- --check
```

### 4. Run Benchmarks (if applicable)

```bash
# Run benchmarks
cargo bench

# Check for regressions
cargo bench -- --baseline main
```

### 5. Update Documentation

- Update relevant sections in MANUAL.md or ARCHITECTURE.md
- Add examples to API.md
- Update CHANGELOG.md with your changes

---

## Testing Requirements

### Test Coverage

New features must have:

1. **Unit tests** for the specific functionality
2. **Integration tests** using the public API
3. **Error case tests** for all error paths
4. **Documentation tests** for public APIs

### Test Organization

```
sqlitegraph/tests/
├── helpers/              # Add helpers here if needed
└── your_feature_tests.rs # Integration tests

sqlitegraph/src/your_module/
└── tests.rs              # Unit tests (or use #[cfg(test)])
```

### Test Naming

```rust
// Descriptive test names
fn test_insert_node_creates_node_with_valid_id() {
    // ...
}

// Edge cases
fn test_insert_node_fails_with_empty_kind() {
    // ...
}

// Regression tests
fn test_phase_44_event_emission_on_commit() {
    // ...
}
```

### Concurrent Tests

For code involving concurrency:

```rust
#[test]
fn test_concurrent_snapshot_reads() {
    let (graph, _, _, _temp_dir) = create_simple_v2_graph().unwrap();
    let graph = Arc::new(graph);

    let handles: Vec<_> = (0..10)
        .map(|_| {
            let g = graph.clone();
            thread::spawn(move || {
                g.snapshot().unwrap();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}
```

---

## Documentation Requirements

### What to Document

1. **Public APIs** - All `pub` items need rustdoc comments
2. **New features** - Add to MANUAL.md or create section
3. **Architecture changes** - Update ARCHITECTURE.md
4. **Breaking changes** - Update CHANGELOG.md

### Rustdoc Style

```rust
/// Brief one-line summary.
///
/// Longer description if needed. Explain what the function does,
/// why it exists, and any important details.
///
/// # Arguments
///
/// * `arg1` - Description of argument
/// * `arg2` - Description of argument
///
/// # Returns
///
/// Description of return value.
///
/// # Errors
///
/// When and why this function returns errors.
///
/// # Examples
///
/// ```
/// use sqlitegraph::{GraphConfig, open_graph};
///
/// let config = GraphConfig::native();
/// let graph = open_graph("test.db", &config)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn example_function(arg1: Type1, arg2: Type2) -> Result<ReturnType, Error> {
    // ...
}
```

### Manual Documentation

For new features visible to users, add to MANUAL.md:

```markdown
## N. Your Feature Name

### Overview

Brief description of what the feature does.

### Usage

```rust
use sqlitegraph::your_feature;

let result = your_feature()?;
```

### Configuration

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| param1    | u32  | 100     | Description |

### Limitations

- Known limitation 1
- Known limitation 2
```

---

## Submitting Changes

### Commit Messages

Follow conventional commit format:

```
type(scope): brief description

Extended description (optional).

Refs: #issue_number
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `test`: Test only
- `refactor`: Code change that doesn't change behavior
- `perf`: Performance improvement
- `chore`: Maintenance task

**Examples:**

```
feat(pubsub): add subscribe method to GraphBackend trait

Implements pub/sub event notification system with four event types:
NodeChanged, EdgeChanged, KVChanged, SnapshotCommitted.

Refs: #123

fix(wal): prevent corruption on concurrent checkpoint

Adds mutex around checkpoint operation to prevent concurrent
checkpoint attempts which could corrupt the WAL file.

Refs: #124
```

### Pull Request Process

1. **Update your branch**

```bash
git checkout main
git pull origin main
git checkout your-branch
git rebase main
```

2. **Push and create PR**

```bash
git push origin your-branch
# Create PR on GitHub
```

3. **PR Description Template**

```markdown
## Summary
Brief description of changes.

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Performance improvement
- [ ] Documentation update
- [ ] Refactoring

## Testing
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Manual testing completed

## Documentation
- [ ] API documentation updated
- [ ] MANUAL.md updated (if user-visible)
- [ ] CHANGELOG.md updated

## Checklist
- [ ] Code follows style guidelines
- [ ] All tests pass
- [ ] No clippy warnings
- [ ] Documentation is accurate
```

### Review Process

1. Automated checks must pass (tests, clippy, format)
2. Code review by maintainer
3. Address review feedback
4. Squash and merge when approved

---

## Feature Development

### Adding a New Graph Algorithm

1. Create file in `src/algo/your_algorithm.rs`
2. Implement algorithm accepting `&dyn GraphBackend`
3. Add tests in `tests/algo_tests.rs`
4. Add benchmark in `benches/your_algorithm.rs`
5. Update `src/algo/mod.rs` exports
6. Add to MANUAL.md Section 4
7. Add CLI command if applicable

### Adding to Native V2 Backend

1. Understand the V2 format (see ARCHITECTURE.md)
2. Update WAL records if adding new operations
3. Add recovery tests
4. Add migration tests (V1 ↔ V2)
5. Update introspection API
6. Document invariants

### Adding CLI Command

1. Add command enum in `sqlitegraph-cli/src/main.rs`
2. Implement command handler
3. Add usage documentation
4. Add integration test
5. Update MANUAL.md Section 11

---

## Getting Help

### Questions?

- Check existing documentation first
- Check GitHub issues for similar discussions
- Read ARCHITECTURE.md for system understanding
- Read TESTS.md for testing patterns

### Reporting Bugs

Include:
- Minimal reproduction case
- Rust version (`rustc --version`)
- Backend used (SQLite or Native V2)
- Error messages and stack traces
- Expected vs actual behavior

### Feature Requests

Include:
- Use case description
- Proposed API (if applicable)
- Alternative approaches considered
- Whether you're willing to implement it

---

## Code Review Guidelines

### For Reviewers

1. Be constructive and specific
2. Explain reasoning for suggestions
3. Approve if code is good enough, not perfect
4. Test suggestions before proposing

### For Contributors

1. Respond to all feedback
2. Push commits to address feedback
3. Ask for clarification if needed
4. Mark conversations as resolved

---

## License

By contributing, you agree that your contributions will be licensed under the GPL-3.0-or-later license.

---

## Thanks

Contributions of any kind are appreciated! Whether it's bug reports, feature requests, documentation improvements, or code changes, they all make SQLiteGraph better.
