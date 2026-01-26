# Phase 37 Safe Code Editing Workflow

**Date:** 2026-01-22
**Purpose:** Define safe, validated workflow for Rust code changes using Magellan + Splice

---

## Tools Overview

### Magellan (`/home/feanor/.local/bin/magellan`)
- **Purpose:** Code symbol indexing and query
- **Database:** `~/.cache/magellan/sqlitegraph.db`
- **Key Commands:**
  - `magellan query --db <DB> --file <PATH>` - List symbols in a file
  - `magellan find --db <DB> --name <NAME>` - Find symbol by name
  - `magellan refs --db <DB> --name <NAME> --path <PATH>` - Show callers/callees
  - `magellan get --db <DB> --file <PATH> --symbol <NAME>` - Get symbol source code
  - `magellan get-file --db <DB> --file <PATH>` - Get all code chunks for a file
  - `magellan status --db <DB>` - Show database statistics

### Splice (`/home/feanor/.local/bin/splice`)
- **Purpose:** Span-safe refactoring for Rust using SQLiteGraph
- **Key Commands:**
  - `splice patch --file <FILE> --symbol <SYMBOL> --with <PATCH_FILE>` - Apply patch to symbol
  - `splice apply-files --glob <GLOB> --find <FIND> --replace <REPLACE>` - Pattern replacement
  - `splice log --limit N` - Show execution history
  - `splice undo` - Undo last operation from backup

---

## Validated Workflow

### Workflow 1: Pattern Replacement (imports, unused code, deprecated APIs)

**Use when:** Making the same text change across multiple files (e.g., removing unused imports, updating deprecated function names)

```bash
# Step 1: cargo check → identify warnings/issues
cargo check 2>&1 | grep "unused_import"

# Step 2: splice apply-files → apply pattern replacement
splice apply-files \
  --glob "src/**/*.rs" \
  --find "use std::collections::HashMap;" \
  --replace "" \
  --create-backup \
  --verbose

# Step 3: cargo check → verify compilation
cargo check

# Step 4: If needed, undo
splice undo
```

**Notes:**
- `--glob` uses workspace-relative patterns
- `--create-backup` enables undo via `splice undo`
- Backups stored in `~/.splice-backup/`
- `--strict` mode treats warnings as errors

### Workflow 2: Symbol-Level Refactoring

**Use when:** Modifying a specific function, struct, or method implementation

```bash
# Step 1: magellan query → find symbol location
magellan query --db ~/.cache/magellan/sqlitegraph.db \
  --file sqlitegraph/src/backend/native/graph_ops/cache.rs

# Step 2: magellan get → retrieve current symbol code
magellan get --db ~/.cache/magellan/sqlitegraph.db \
  --file sqlitegraph/src/backend/native/graph_ops/cache.rs \
  --symbol get_neighbors_optimized

# Step 3: Create patch file with replacement code
cat > /tmp/patch.rs << 'EOF'
pub fn get_neighbors_optimized(
    graph_file: &mut GraphFile,
    node_id: NativeNodeId,
    direction: Direction,
    ctx: &mut TraversalContext,
) -> NativeResult<Vec<NativeNodeId>> {
    // New implementation here
    ...
}
EOF

# Step 4: splice patch → apply change
splice patch \
  --file sqlitegraph/src/backend/native/graph_ops/cache.rs \
  --symbol get_neighbors_optimized \
  --with /tmp/patch.rs \
  --create-backup \
  --verbose

# Step 5: cargo check → verify
cargo check

# Step 6: If needed, undo
splice undo
```

**Notes:**
- Symbol must be indexed by Magellan
- `--kind` can filter: function, method, struct, impl, module, etc.
- Splice validates span boundaries to prevent malformed code

### Workflow 3: Find Callers Before Refactoring

**Use when:** Need to understand impact of changing a function

```bash
# Find all callers of a symbol
magellan refs --db ~/.cache/magellan/sqlitegraph.db \
  --name should_use_sequential_read \
  --path sqlitegraph/src/backend/native/adjacency/linear_detector.rs \
  --direction in

# Find all functions called by a symbol
magellan refs --db ~/.cache/magellan/sqlitegraph.db \
  --name bfs_generic_scalar \
  --path sqlitegraph/src/backend/native/graph_ops/bfs_implementations.rs \
  --direction out
```

---

## Splice Backup and Undo

### Backup Location
Backups are stored in: `~/.splice-backup/<operation-id>/`

### View Operations
```bash
# List recent operations
splice log --limit 10

# Get details as JSON
splice log --limit 1 --json

# Show backup manifest
cat ~/.splice-backup/<operation-id>/manifest.json
```

### Undo
```bash
# Undo the most recent operation
splice undo

# This restores files from the backup manifest
```

---

## Safety Checks

### Before Any Change

1. **Magellan index is current:**
   ```bash
   magellan status --db ~/.cache/magellan/sqlitegraph.db
   # Should show current file/symbol counts
   ```

2. **You know the exact symbol location:**
   ```bash
   magellan find --db ~/.cache/magellan/sqlitegraph.db --name <SYMBOL>
   ```

3. **You've read the current code:**
   ```bash
   magellan get --db ~/.cache/magellan/sqlitegraph.db --file <FILE> --symbol <SYMBOL>
   # Or use Read tool directly
   ```

4. **You have a backup plan:**
   - Always use `--create-backup`
   - Know that `splice undo` can revert

### After Any Change

1. **Compilation check:**
   ```bash
   cargo check
   ```

2. **Clippy check:**
   ```bash
   cargo clippy --all-targets --all-features -D warnings
   ```

3. **Run affected tests:**
   ```bash
   cargo test -p sqlitegraph <test_name>
   ```

4. **Verify with Magellan:**
   ```bash
   magellan query --db ~/.cache/magellan/sqlitegraph.db --file <FILE>
   # Confirm symbol structure is correct
   ```

---

## Known Limitations

### Splice `apply-files`

- **Workspace root detection:** Requires proper project structure (Cargo.toml for Rust projects)
- **Glob patterns:** Must be relative to workspace root
- **Pattern matching:** Exact string match, not regex

### Splice `patch`

- **Symbol must exist in Magellan index:** If symbol not found, re-index first
- **Span boundaries:** Replacement must be valid Rust code
- **Cannot add new top-level items:** Only modify existing symbols

### Magellan

- **Requires database:** Run `magellan watch` or ensure index is current
- **Symbol resolution:** Uses AST parsing, may not handle some macro edge cases
- **Dynamic dispatch:** May not track trait method calls perfectly

---

## Phase 37 Issues to Fix

From bug analysis report (`docs/bug-analysis-report.md`), the following issues need to be addressed using this workflow:

### Critical (Must Fix)

1. **BUG-4.1:** Slice index overflow in `SequentialClusterReader::extract_neighbors()`
   - File: `sqlitegraph/src/backend/native/adjacency/sequential_cluster_reader.rs`
   - Missing bounds validation on `byte_offset + cluster_size`

2. **BUG-5.2:** `node_cluster_index` never populated in BFS implementations
   - Files: `bfs_implementations.rs`, `mod.rs`
   - Must add mapping population after `observe_with_cluster()`

### High Priority

3. **BUG-3.2:** Node read failures silently use `(0, 0)` fallback
   - Pollutes cluster offset tracking

4. **BUG-9.1:** Non-standard `RefCell<u32>` in static context

5. **BUG-1.2:** MMAP depth counter `RefCell` can panic on reentrant access

---

## Execution Plan for Phase 37-07

### Task 1: Add bounds validation to `extract_neighbors()`
- Use `splice patch` to add validation
- Add test for malformed data

### Task 2: Populate `node_cluster_index` in all BFS implementations
- Use `splice patch` for each of 4 BFS functions
- Add lines after `observe_with_cluster()`:
  ```rust
  let cluster_index = ctx.detector.cluster_offsets().len().saturating_sub(1);
  ctx.node_cluster_index.insert(current_node, cluster_index);
  ```

### Task 3: Fix node read fallback handling
- Add logging for fallback cases
- Consider returning error instead of silent `(0, 0)`

### Task 4: Address `RefCell` issues
- Evaluate thread safety requirements
- Replace with thread-safe alternative if needed

### Task 5: Remove/modify `prefetch_clusters_from()` calls
- Prevent L1 short-circuit of sequential read
- Use `splice patch` to modify or remove

---

## Validation Checklist

Before committing any change:

- [ ] Magellan index reflects current code
- [ ] Symbol location confirmed with `magellan find`
- [ ] Current code read and understood
- [ ] Backup created with `--create-backup`
- [ ] `cargo check` passes
- [ ] `cargo clippy` passes
- [ ] Affected tests pass
- [ ] No new warnings introduced

---

**This workflow ensures all Rust changes go through Magellan → Splice → cargo fmt → clippy → tests**
