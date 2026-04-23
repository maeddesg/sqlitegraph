# SQLiteGraph Engineering Standards

## Project Overview

SQLiteGraph is an embedded graph database with dual backend support:
- **V2 Backend**: SQLite-based (mature, ACID transactions, production-ready)
- **V3 Backend**: Native B+Tree index (experimental, unlimited capacity, HNSW vector search)

Published on crates.io as `sqlitegraph` crate. See README.md for user-facing documentation.

## The Golden Rule
> **NEVER write code based on assumptions. ALWAYS read the source first.**

## Code Graph Workflow
Use `.magellan/sqlitegraph.db` before editing hotpaths.

1. Check the index: `magellan status --db .magellan/sqlitegraph.db`
2. Find symbols: `magellan find --db .magellan/sqlitegraph.db --name <symbol>`
3. Find callers and references: `magellan refs --db .magellan/sqlitegraph.db --name <symbol>`
4. Inspect file symbols: `magellan query --db .magellan/sqlitegraph.db --file <path>`
5. Search text with the DB: `llmgrep search --db .magellan/sqlitegraph.db "<term>"`
6. Before changing hotpaths, try:
   `mirage paths --db .magellan/sqlitegraph.db <from> <to>`
   `mirage blast-zone --db .magellan/sqlitegraph.db --use-call-graph <file:line>`

Note: `llmgrep lookup` is supported (this project uses Native V3 for performance).

## The AI Command Cycle: Read-Plan-Prove-Implement-Verify

1. **READ**: Analyze source, schema, and current benchmarks. Use `magellan` and `llmgrep`.
2. **PLAN**: For any task involving >1 file, write a plan to `docs/standards/plans/YYYY-MM-DD-<topic>.md` and get user approval before touching code.
3. **PROVE (TDD)**: Create a failing test in `tests/` or a reproduction script.
4. **IMPLEMENT**: Write code to pass the test. Standard: No `unwrap()`, strict type safety.
5. **VERIFY**: Run full test suite and `cargo bench --features v3-bench`.

## Hard Gates
- **Performance**: Any storage change MUST be benchmarked with `v3-bench`.
- **Durability**: V3 WAL operations MUST have recovery tests.
- **Verification**: No completion claims without raw terminal output evidence.
- **Purge**: Delete any CodeMCP/SYNCORE artifacts on sight.

## Subagent Handover Protocol

When dispatching subagents on this project, require them to use the `sqlitegraph-subagent` skill. Every subagent must:

1. Query `.magellan/sqlitegraph.db` before modifying code (see Code Graph Workflow above).
2. Produce a **Handover Summary** block when finishing, including:
   - Task description and completion status
   - Files modified with rationale
   - Graph artifacts discovered (symbols, references, CFG insights)
   - Open questions or blockers for the next subagent
   - Suggested next steps

This ensures sequential subagents build on verified graph state rather than stale assumptions.

## Rust Standards
- Maximum 1,000 LOC per module (split into submodules if larger).
- No `unwrap()` in production; use `?` or `.expect("detailed reason")`.
- Document all public APIs with `# Examples` and `# Errors`.

## Parallel Execution Safety

**CRITICAL:** Never use `std::thread::available_parallelism().unwrap().get()` directly - returns ALL CPU cores and can crash systems.

**Safe pattern:**
```rust
let num_cpus = std::cmp::min(
    std::thread::available_parallelism().unwrap().get(),
    4  // Maximum 4 threads for safety
);
```

**Testing:** Always run tests sequentially: `cargo test -- --test-threads=1`
**Benchmarks:** Limit parallelism to prevent system overload

## Feature Verification Workflow

Use `.claude/skills/verify-feature/run.sh <feature-name>` to verify features are actually wired (not just declared).

Checks:
- Feature is exported (declaration)
- Feature is instantiated (integration)
- Data flow complete (output is read)
- No hardcoded bypasses in I/O code

Prevents "declared but not working" implementations.

## Thread Safety Verification

Use thread sanitizer to detect data races:

```bash
cargo clean
RUSTFLAGS="-Z sanitizer=thread" cargo test --lib --features native-v3 --target x86_64-unknown-linux-gnu
```

Expect zero warnings. If data races found, fix before marking work complete.
