# Magellan + Splice Workflow Improvement Suggestions

**Purpose:** Ideas for enhancing the Magellan + Splice workflow without losing precision or safety
**Target Audience:** Tool developers, workflow designers, future agents

---

## Category 1: Magelian Enhancements

### S1.1: Symbol Change Impact Analysis

**Current Limitation:** `magellan refs` shows callers but not the nature of the dependency

**Suggestion:** Add impact scoring to `magellan refs`

```bash
magellan refs --db <DB> --name <SYMBOL> --path <PATH> --impact
```

**Output:**
```
Symbol: bfs_generic_scalar
Impact Score: 8/10 (High)

Callers (4):
  ├─ native_bfs (public API) [CRITICAL]
  ├─ native_bfs_with_cpu_profile (public API) [CRITICAL]
  ├─ native_bfs_with_telemetry (diagnostic) [MEDIUM]
  └─ test_bfs_generic_scalar (test) [LOW]

Risk Summary:
- Breaking change would affect 2 public APIs
- 1 diagnostic function would need update
- Test coverage: 1 direct test
```

**Why:** Helps assess refactoring risk before making changes

---

### S1.2: Symbol Dependency Graph Visualization

**Current Limitation:** Cannot visualize full call chain

**Suggestion:** Add graph output option

```bash
magellan deps --db <DB> --name <SYMBOL> --format dot --output deps.dot
dot -Tpng deps.dot -o deps.png
```

**Why:** Visual understanding helps prevent unintended side effects

---

### S1.3: Symbol Fingerprinting for Change Detection

**Current Limitation:** Cannot detect if code was already modified

**Suggestion:** Hash symbol AST and store in database

```bash
magellan fingerprint --db <DB> --symbol <SYMBOL> --before > /tmp/before.hash
# ... make changes ...
magellan fingerprint --db <DB> --symbol <SYMBOL> --after > /tmp/after.hash
diff /tmp/before.hash /tmp/after.hash
```

**Why:** Prevents overwriting manual edits or concurrent changes

---

### S1.4: Cross-Reference Symbol Names

**Current Limitation:** Finding symbols by fuzzy match not supported

**Suggestion:** Add fuzzy search

```bash
# Find all symbols with "cluster" in name
magellan search --db <DB> --pattern "*cluster*"

# Output matches with similarity score
```

**Why:** Helps when symbol name is partially remembered

---

### S1.5: Symbol Metadata Annotations

**Current Limitation:** Cannot mark symbols as unstable, deprecated, etc.

**Suggestion:** Add label command

```bash
magellan label --db <DB> --add "unstable:37-05-fix" --symbol <SYMBOL>
magellan label --db <DB> --list --show-code
```

**Why:** Tracks which symbols are pending changes, experimental, etc.

---

## Category 2: Splice Enhancements

### S2.1: Dry-Run Mode with Diff Preview

**Current Limitation:** Cannot see exact changes before applying

**Suggestion:** Add `--dry-run` flag

```bash
splice patch \
  --file src/file.rs \
  --symbol function_name \
  --with /tmp/patch.rs \
  --dry-run \
  --format unified
```

**Output:** Unified diff showing exactly what will change

**Why:** Reduces risk of unintended modifications

---

### S2.2: Incremental Patch Validation

**Current Limitation:** All-or-nothing validation; cannot see intermediate errors

**Suggestion:** Add step-by-step validation mode

```bash
splice patch \
  --file src/file.rs \
  --symbol function_name \
  --with /tmp/patch.rs \
  --validate-steps
```

**Output:**
```
Step 1: Parse patch file... OK
Step 2: Locate symbol in source... OK
Step 3: Calculate span boundaries... OK
Step 4: Validate Rust syntax... OK
Step 5: Check type compatibility... OK
Step 6: Verify imports exist... FAILED
  Missing import: std::collections::HashMap
Step 7: Apply patch... ABORTED
```

**Why:** Faster feedback on what's wrong with the patch

---

### S2.3: Automatic Import Management

**Current Limitation:** Adding new types requires manual import updates

**Suggestion:** Auto-add imports when symbols used in patch

```bash
splice patch \
  --file src/file.rs \
  --symbol function_name \
  --with /tmp/patch.rs \
  --auto-imports
```

**Behavior:** Scans patch for unrecognized types, adds use statements

**Why:** Reduces manual import management

---

### S2.4: Batch Operations with Rollback Grouping

**Current Limitation:** Cannot apply multiple patches as atomic unit

**Suggestion:** Add transaction/plan mode

```bash
splice plan create --name "fix-bfs-mapping"

splice plan add --name "fix-bfs-mapping" \
  --file src/bfs.rs --symbol bfs_generic_scalar --with patch1.rs

splice plan add --name "fix-bfs-mapping" \
  --file src/bfs.rs --symbol bfs_pointer --with patch2.rs

splice plan execute --name "fix-bfs-mapping" \
  --atomic --create-backup
```

**Behavior:** Either all patches apply, or all roll back

**Why:** Safer for multi-file refactors

---

### S2.5: Conflict Detection with Concurrent Edits

**Current Limitation:** No warning if file was modified since last magellan index

**Suggestion:** Check file modification time before patching

```bash
splice patch \
  --file src/file.rs \
  --symbol function_name \
  --with /tmp/patch.rs \
  --check-mtime
```

**Behavior:** Aborts if source file mtime > database mtime

**Why:** Prevents overwriting manual edits

---

## Category 3: Validation Enhancements

### S3.1: Pre-Commit Test Selection

**Current Limitation:** Running all tests is slow for large changesets

**Suggestion:** Smart test selection based on changed symbols

```bash
# Only run tests that call modified symbols
cargo test --affected-by bfs_generic_scalar

# Implemented via:
# 1. Get modified symbols from splice log
# 2. magellan refs to find test functions
# 3. Run only those tests
```

**Why:** Faster feedback loop

---

### S3.2: Semantic Validation Beyond Syntax

**Current Limitation:** Splice validates Rust syntax but not semantic correctness

**Suggestion:** Add semantic checks

```bash
splice patch \
  --file src/file.rs \
  --symbol function_name \
  --with /tmp/patch.rs \
  --semantic-check
```

**Checks:**
- All referenced symbols exist in scope
- Return type matches expected signature
- Generic bounds are satisfied
- Trait methods are actually implemented

**Why:** Catches type errors before runtime

---

### S3.3: Compilation Cache Integration

**Current Limitation:** `cargo check` is slow even for small changes

**Suggestion:** Use `check` cache from incremental compilation

```bash
# Splice could integrate with cargo's dep graph
splice patch ... --use-cargo-cache
```

**Why:** Faster validation for large projects

---

## Category 4: Workflow Integration

### S4.1: Git Integration

**Current Limitation:** No git awareness in workflow

**Suggestion:** Add git-aware commands

```bash
# Check if symbol is modified in git working tree
magellan status-git --db <DB> --symbol <SYMBOL>

# Create git commit after successful patch
splice patch ... --git-commit "refactor: update bfs mapping"

# Show git diff between current and patched version
splice patch ... --git-diff
```

**Why:** Aligns code changes with version control

---

### S4.2: Issue Tracker Integration

**Current Limitation:** No link to GitHub issues

**Suggestion:** Tag operations with issue numbers

```bash
splice patch \
  --file src/file.rs \
  --symbol function_name \
  --with /tmp/patch.rs \
  --metadata '{"issue": "https://github.com/.../37"}'
```

**Why:** Tracks which changes address which issues

---

### S4.3: CI/CD Integration

**Current Limitation:** No automated validation in CI

**Suggestion:** CI mode that produces machine-readable output

```bash
splice patch ... --ci-mode --output junit.xml
```

**Output:** JUnit XML for CI systems

**Why:** Enables automated enforcement of workflow

---

## Category 5: Safety Improvements

### S5.1: Symbol Contract Verification

**Current Limitation:** Cannot verify that refactored code maintains same behavior

**Suggestion:** Add contract-based testing

```bash
# Define contract before change
magellan contract create --symbol bfs_generic_scalar \
  --input "GraphFile, NativeNodeId, u32" \
  --output "Vec<NativeNodeId>" \
  --invariants "visits all nodes up to depth, no duplicates"

# Verify contract still holds after patch
splice patch ... --verify-contract bfs_generic_scalar
```

**Why:** Ensures refactoring doesn't change semantics

---

### S5.2: Dead Code Detection

**Current Limitation:** Cannot detect if refactored code leaves dead symbols

**Suggestion:** Add dead code analysis

```bash
# After patch, check for newly dead code
splice patch ...
magellan dead-code --db <DB> --since <operation-id>
```

**Output:** Lists symbols that became unreachable

**Why:** Keeps codebase clean

---

### S5.3: Runtime Invariant Checking

**Current Limitation:** Cannot catch state drift bugs at runtime

**Suggestion:** Add invariant assertions to TraversalContext

```rust
impl TraversalContext {
    fn verify_invariants(&self) {
        // If cluster_buffer is Some, offsets must match
        if let Some(ref buffer) = self.cluster_buffer {
            assert_eq!(
                buffer.len(),
                self.cluster_buffer_offsets.iter().map(|(_, s)| *s as usize).sum(),
                "cluster_buffer size doesn't match offsets"
            );
        }
    }
}

// Call after each mutation
ctx.verify_invariants();
```

**Why:** Catches state drift immediately

---

## Category 6: Performance & Tooling

### S6.1: Parallel Magellan Indexing

**Current Limitation:** Indexing large codebase is slow

**Suggestion:** Parallel indexing

```bash
magellan watch --root . --db <DB> --threads 8
```

**Why:** Faster indexing for large projects

---

### S6.2: Incremental Index Updates

**Current Limitation:** Full re-index required after changes

**Suggestion:** Incremental updates

```bash
magellan update --db <DB> --file src/file.rs
```

**Behavior:** Only re-indexes changed file and its dependencies

**Why:** Faster updates during active development

---

### S6.3: Fuzzy Symbol Matching for Refactoring

**Current Limitation:** Cannot batch-rename similar symbols

**Suggestion:** Add rename command with confirmation

```bash
magellan rename --db <DB> --from-pattern "*_cluster_*" --to-pattern "*_node_*"
```

**Behavior:** Shows preview, asks for confirmation per symbol

**Why:** Safer batch refactoring

---

## Category 7: Documentation & Learning

### S7.1: Symbol Documentation Integration

**Current Limitation:** Cannot see docs alongside code

**Suggestion:** Add doc extraction

```bash
magellan get --db <DB> --symbol bfs_generic_scalar --with-docs
```

**Output:** Source code + rustdoc comments

**Why:** Better understanding before editing

---

### S7.2: Example-Based Patch Creation

**Current Limitation:** Creating patches manually is error-prone

**Suggestion:** Generate patch template from example

```bash
# Given an example of what the code should look like
splice generate-from-example \
  --example-file examples/good_bfs.rs \
  --target-file src/bfs.rs \
  --target-symbol bfs_generic_scalar \
  --output /tmp/suggested_patch.rs
```

**Why:** Reduces manual patch creation errors

---

### S7.3: Interactive Tutorial Mode

**Current Limitation:** Steep learning curve for new users

**Suggestion:** Add tutorial mode

```bash
splice tutorial --interactive
```

**Behavior:** Guides through simple edit with explanations

**Why:** Faster onboarding for agents/LLMs

---

## Priority Ranking

### High Priority (High Impact, Low Risk)

1. **S2.1: Dry-Run Mode** - Critical for safety
2. **S1.1: Impact Analysis** - Critical for risk assessment
3. **S2.4: Batch Operations** - Critical for multi-file changes
4. **S5.1: Contract Verification** - Critical for correctness

### Medium Priority (Medium Impact, Medium Risk)

5. **S2.2: Incremental Validation** - Faster debugging
6. **S2.3: Auto Imports** - UX improvement
7. **S4.1: Git Integration** - VCS alignment
8. **S3.1: Smart Test Selection** - Faster feedback

### Low Priority (Nice to Have)

9. **S1.2: Dependency Graph** - Visualization
10. **S1.4: Fuzzy Search** - Convenience
11. **S6.1: Parallel Indexing** - Performance
12. **S7.3: Tutorial Mode** - Learning

---

## Anti-Patterns to Avoid

### A1: Don't Add AI Code Generation

**Why:** Generated code may be incorrect; defeats purpose of precise editing

**Alternative:** Use tools as-is, verify manually

### A2: Don't Add Automatic Refactoring

**Why:** Risk of unintended changes across codebase

**Alternative:** Explicit per-symbol edits with verification

### A3: Don't Remove Backup Requirement

**Why:** Undo capability is critical for safety

**Alternative:** Keep `--create-backup` mandatory or default

### A4: Don't Add Async/Auto-Apply Modes

**Why:** Removes human verification step

**Alternative:** Always require explicit confirmation

---

## Implementation Notes

### For Tool Developers

1. **Preserve precision:** Every enhancement must maintain exact span tracking
2. **Fail fast:** Validation errors should abort before any writes
3. **Log everything:** Enable forensic analysis of failures
4. **Test thoroughly:** Add tests for each enhancement
5. **Document changes:** Update this document with each new feature

### For LLM/Agent Users

1. **Always use current tools:** Don't wait for enhancements
2. **Read docs before editing:** Understand what you're changing
3. **Validate after every change:** Catch errors early
4. **Undo when in doubt:** Better safe than sorry
5. **Report issues:** Help improve the tools

---

**Version:** 1.0
**Last Updated:** 2026-01-22
**Status:** Open for feedback and discussion
