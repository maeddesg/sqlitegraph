# LLM/Agent Workflow: Safe Code Editing with Magellan + Splice

**Purpose:** This document teaches LLMs and AI agents how to safely modify code using Magellan and Splice tools.

**Target Audience:** AI agents (Claude, GPT, etc.) that need to edit code

**Supported Languages:** Rust, Python, C, C++, Java, JavaScript, TypeScript

**Critical Rule:** ALL code changes MUST go through Magellan → Splice → validation. NEVER edit files directly.

---

## Tool Locations

```bash
Magellan: /home/feanor/.local/bin/magellan
Splice:   /home/feanor/.local/bin/splice
Database: ~/.cache/magellan/sqlitegraph.db
Backups:  ~/.splice-backup/
```

---

## Overview

### Why This Workflow Exists

SQLiteGraph is a **deterministic storage engine**. Incorrect edits can cause:
- Data corruption
- Read amplification
- Race conditions
- ACID violations

### Critical: Splice Uses LSP Validation

**Splice will NOT apply patches with errors.** The LSP (Language Server Protocol) validates all patches:
- **Syntax errors:** Patch rejected
- **Type errors:** Patch rejected
- **Missing imports:** Patch rejected
- **Wrong signatures:** Patch rejected
- **Undefined symbols:** Patch rejected

This means: **Patch files must be complete, correct Rust code before Splice will apply them.**

### The Workflow

```
┌─────────────────┐
│ 1. Magellan     │ ← Find symbols, understand code structure
├─────────────────┤
│ 2. Read Code    │ ← Verify current implementation
├─────────────────┤
│ 3. Create Patch │ ← Prepare replacement code
├─────────────────┤
│ 4. Splice Apply │ ← Apply with backup creation
├─────────────────┤
│ 5. Validate     │ ← cargo check, clippy, tests
├─────────────────┤
│ 6. Undo if fail │ ← Restore from backup
└─────────────────┘
```

---

## Magellan: Code Understanding Tool

### What Magellan Does

- **Indexes** all Rust symbols (functions, structs, methods, modules)
- **Tracks** references between symbols (who calls whom)
- **Provides** source code chunks for any symbol
- **Enables** precise symbol location before editing

### Key Commands

```bash
# Show all symbols in a file
magellan query --db ~/.cache/magellan/sqlitegraph.db \
  --file sqlitegraph/src/backend/native/graph_ops/cache.rs

# Find a specific symbol by name
magellan find --db ~/.cache/magellan/sqlitegraph.db \
  --name get_neighbors_optimized

# Get source code for a specific symbol
magellan get --db ~/.cache/magellan/sqlitegraph.db \
  --file sqlitegraph/src/backend/native/graph_ops/cache.rs \
  --symbol get_neighbors_optimized

# Find all callers of a symbol (who calls this?)
magellan refs --db ~/.cache/magellan/sqlitegraph.db \
  --name bfs_generic_scalar \
  --path sqlitegraph/src/backend/native/graph_ops/bfs_implementations.rs \
  --direction in

# Find all functions called by a symbol (what does this call?)
magellan refs --db ~/.cache/magellan/sqlitegraph.db \
  --name bfs_generic_scalar \
  --path sqlitegraph/src/backend/native/graph_ops/bfs_implementations.rs \
  --direction out

# Check database status (is index current?)
magellan status --db ~/.cache/magellan/sqlitegraph.db
```

### Expected Output Format

```
$ magellan query --db ~/.cache/magellan/sqlitegraph.db --file src/file.rs
/path/to/file.rs:
  Line   10: Function     new [fn]
  Line   25: Function     process [fn]
  Line   40: Struct       MyStruct [struct]
```

```
$ magellan find --db ~/.cache/magellan/sqlitegraph.db --name process
Found "process":
  File:     /path/to/file.rs
  Kind:     Function [fn]
  Location: Line 25, Column 0
  Node ID:  123
```

---

## Splice: Safe Code Editing Tool

### What Splice Does

- **Applies** changes to specific symbol spans (not whole files)
- **Validates** Rust syntax before applying
- **Creates** automatic backups for undo
- **Tracks** all operations in a log

### Two Modes of Operation

#### Mode 1: Symbol-Level Patch (`splice patch`)

**Use when:** Modifying a specific function, method, or struct

```bash
splice patch \
  --file <path/to/file.rs> \
  --symbol <symbol_name> \
  --with </path/to/patch.rs> \
  --create-backup \
  --verbose
```

**Parameters:**
- `--file`: Path to the Rust source file (workspace-relative or absolute)
- `--symbol`: Exact name of the symbol to patch
- `--with`: Path to file containing replacement code
- `--create-backup`: Creates backup for undo (REQUIRED)
- `--verbose`: Shows detailed operation info
- `--kind`: Optional symbol kind filter (function, method, struct, impl, module)

**Patch file format:** `/tmp/patch.rs`
```rust
pub fn get_neighbors_optimized(
    graph_file: &mut GraphFile,
    node_id: NativeNodeId,
    direction: Direction,
    ctx: &mut TraversalContext,
) -> NativeResult<Vec<NativeNodeId>> {
    // New implementation here
    let result = /* ... */;
    Ok(result)
}
```

#### Mode 2: Pattern Replacement (`splice apply-files`)

**Use when:** Making the same text change across multiple files

```bash
splice apply-files \
  --glob "src/**/*.rs" \
  --find "use std::collections::HashMap;" \
  --replace "" \
  --create-backup \
  --verbose
```

**Parameters:**
- `--glob`: Glob pattern for files to match
- `--find`: Exact text pattern to find
- `--replace`: Replacement text (empty string = delete)
- `--create-backup`: Creates backup for undo (REQUIRED)

### Other Commands

```bash
# View operation history
splice log --limit 10

# Get details as JSON
splice log --limit 1 --json

# Undo the most recent operation
splice undo
```

---

## Complete Step-by-Step Workflows

### Workflow A: Modify a Single Function

**Scenario:** Add logging to `bfs_generic_scalar()` function

```bash
# Step 1: Find the symbol location
magellan find --db ~/.cache/magellan/sqlitegraph.db \
  --name bfs_generic_scalar

# Output shows: File: sqlitegraph/src/backend/native/graph_ops/bfs_implementations.rs
#                  Location: Line 14

# Step 2: Read the current implementation
magellan get --db ~/.cache/magellan/sqlitegraph.db \
  --file sqlitegraph/src/backend/native/graph_ops/bfs_implementations.rs \
  --symbol bfs_generic_scalar

# Step 3: Read the full file to understand context (use Read tool)
# This gives you surrounding code, imports, etc.

# Step 4: Create patch file with modified function
cat > /tmp/bfs_patch.rs << 'EOF'
pub fn bfs_generic_scalar(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    // ADDED: Log entry for debugging
    log::debug!("bfs_generic_scalar: start={}, depth={}", start, depth);

    if depth == 0 {
        return Ok(vec![start]);
    }

    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    let mut result = Vec::new();

    // ... rest of implementation
    Ok(result)
}
EOF

# Step 5: Apply the patch
splice patch \
  --file sqlitegraph/src/backend/native/graph_ops/bfs_implementations.rs \
  --symbol bfs_generic_scalar \
  --with /tmp/bfs_patch.rs \
  --create-backup \
  --verbose

# Step 6: Verify compilation
cargo check

# Step 7: If compilation fails, undo
splice undo

# Step 8: Run affected tests
cargo test -p sqlitegraph bfs_generic_scalar
```

### Workflow B: Add Code After a Specific Line

**Scenario:** Add `node_cluster_index.insert()` after `observe_with_cluster()`

```bash
# Step 1: Find where the code is
magellan find --db ~/.cache/magellan/sqlitegraph.db \
  --name observe_with_cluster

# Step 2: Get the caller function that needs modification
magellan refs --db ~/.cache/magellan/sqlitegraph.db \
  --name observe_with_cluster \
  --path sqlitegraph/src/backend/native/adjacency/linear_detector.rs \
  --direction out

# Step 3: Read the full BFS function to see current code
# (Use Read tool on the file)

# Step 4: Create patch with the full function including new lines
cat > /tmp/bfs_add_mapping.rs << 'EOF'
pub fn bfs_generic_scalar(
    graph_file: &mut GraphFile,
    start: NativeNodeId,
    depth: u32,
) -> Result<Vec<NativeNodeId>, NativeBackendError> {
    if depth == 0 {
        return Ok(vec![start]);
    }

    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    let mut result = Vec::new();
    let mut ctx = TraversalContext::new();

    visited.insert(start);
    queue.push_back((start, 0));

    while let Some((current_node, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }

        let degree = AdjacencyHelpers::outgoing_degree(graph_file, current_node)?;

        // Extract cluster metadata
        let (cluster_offset, cluster_size) = match graph_file.read_node_at(current_node) {
            Ok(node_record) => (
                node_record.outgoing_cluster_offset,
                node_record.outgoing_cluster_size,
            ),
            Err(_) => (0, 0),
        };

        // Observe for pattern detection with cluster metadata
        let _pattern = ctx.detector.observe_with_cluster(current_node, degree, cluster_offset, cluster_size);

        // ADDED: Populate node_cluster_index mapping
        let cluster_index = ctx.detector.cluster_offsets().len().saturating_sub(1);
        ctx.node_cluster_index.insert(current_node, cluster_index);

        // ... rest of function
    }

    Ok(result)
}
EOF

# Step 5: Apply patch
splice patch \
  --file sqlitegraph/src/backend/native/graph_ops/bfs_implementations.rs \
  --symbol bfs_generic_scalar \
  --with /tmp/bfs_add_mapping.rs \
  --create-backup

# Step 6: Verify
cargo check
cargo test -p sqlitegraph bfs
```

### Workflow C: Remove Unused Code Across Files

**Scenario:** Remove unused import across all files

```bash
# Step 1: Identify the pattern
cargo check 2>&1 | grep "unused import"

# Step 2: Apply pattern replacement
splice apply-files \
  --glob "src/**/*.rs" \
  --find "use std::collections::HashMap;\n" \
  --replace "" \
  --create-backup

# Step 3: Verify
cargo check
```

### Workflow D: Find All Callers Before Refactoring

**Scenario:** Understand impact of changing `should_use_sequential_read()`

```bash
# Step 1: Find the symbol
magellan find --db ~/.cache/magellan/sqlitegraph.db \
  --name should_use_sequential_read

# Step 2: Find all callers
magellan refs --db ~/.cache/magellan/sqlitegraph.db \
  --name should_use_sequential_read \
  --path sqlitegraph/src/backend/native/adjacency/linear_detector.rs \
  --direction in

# Step 3: For each caller, read the code to understand usage
# This tells you what changes might break

# Step 4: Plan changes accordingly
```

---

## Validation Commands

### After Every Change

```bash
# 1. Compilation check
cargo check

# 2. Linter check (more strict)
cargo clippy --all-targets --all-features -D warnings

# 3. Run affected tests
cargo test -p sqlitegraph <test_name>

# 4. Verify symbol still exists
magellan find --db ~/.cache/magellan/sqlitegraph.db --name <symbol>

# 5. Run full test suite if critical change
cargo test --all
```

---

## Error Recovery

### Undo Failed Changes

```bash
# View what happened
splice log --limit 1

# Undo immediately after
splice undo

# Files are restored from ~/.splice-backup/<operation-id>/
```

### When Splice Cannot Find Symbol

```bash
# 1. Verify Magellan database is current
magellan status --db ~/.cache/magellan/sqlitegraph.db

# 2. If not current, re-index
magellan watch --root . --db ~/.cache/magellan/sqlitegraph.db \
  --scan-initial --debounce-ms 100 &
# Wait for indexing, then kill

# 3. Try find again
magellan find --db ~/.cache/magellan/sqlitegraph.db --name <symbol>
```

### When Patch Fails Validation

**Error: `splice patch` fails with no changes applied**

**Cause:** The LSP (Language Server Protocol) found syntax or semantic errors in the patch file. Splice ONLY accepts syntactically and semantically correct Rust code.

**Common LSP Errors:**
- Syntax error in patch (missing braces, invalid Rust)
- Type mismatch in patch
- Missing imports in patch
- Wrong function signature in patch
- Undefined symbols in patch

**Solution:**
1. Check patch file syntax: `rustc /tmp/patch.rs`
2. Verify exact symbol name with `magellan find`
3. Read current code again with `magellan get`
4. Fix patch and retry

---

## Rules for LLMs/Agents

### MANDATORY Rules

1. **NEVER edit Rust files directly** - Always use Splice
2. **ALWAYS use `--create-backup`** - Enables undo capability
3. **VERIFY symbol exists before patching** - Use `magellan find`
4. **READ current code first** - Use `magellan get` or Read tool
5. **VALIDATE after every change** - `cargo check`
6. **UNDO if validation fails** - `splice undo`

### Forbidden Behaviors

- ❌ Using Write tool directly for .rs files
- ❌ Using Edit tool directly for .rs files
- ❌ Guessing symbol locations
- ❌ Modifying code without reading it first
- ❌ Skipping validation steps
- ❌ Creating "quick fixes" without backup

### Allowed Direct Edits

Only for non-code files:
- Markdown documentation (.md)
- Configuration files (.toml, .yaml, .json)
- Plain text scripts (.sh)

For any source code files (.rs, .py, .c, .cpp, .java, .js, .ts), MUST use Splice.

---

## Example Conversation Prompts

### For "Fix this bug" requests

```
1. Use magellan find to locate the buggy function
2. Use magellan get to read current implementation
3. Identify the bug
4. Create patch file with fix
5. Use splice patch to apply
6. Run cargo check to verify
7. If fails, use splice undo and reassess
```

### For "Refactor this code" requests

```
1. Use magellan refs to find all callers
2. Understand impact scope
3. Use magellan get to read current code
4. Create patch with refactored version
5. Use splice patch to apply
6. Run cargo check and clippy
7. Run tests to verify behavior unchanged
```

### For "Add this feature" requests

```
1. Identify where to add code (magellan query)
2. Find integration points (magellan refs)
3. Read surrounding code for context
4. Create patch adding new code
5. Apply with splice patch
6. Write tests for new functionality
7. Verify all tests pass
```

---

## Quick Reference Card

```bash
# FIND symbol
magellan find --db ~/.cache/magellan/sqlitegraph.db --name <NAME>

# GET symbol code
magellan get --db ~/.cache/magellan/sqlitegraph.db --file <FILE> --symbol <NAME>

# FIND callers
magellan refs --db ~/.cache/magellan/sqlitegraph.db --name <NAME> --path <FILE> --direction in

# PATCH symbol
splice patch --file <FILE> --symbol <NAME> --with <PATCH> --create-backup

# PATTERN replace
splice apply-files --glob "<GLOB>" --find "<FIND>" --replace "<REPLACE>" --create-backup

# VALIDATE
cargo check && cargo clippy --all-targets --all-features -D warnings

# UNDO
splice undo
```

---

**Version:** 1.0
**Last Updated:** 2026-01-22
**For:** SQLiteGraph v1.7 Gap Closure Phase
