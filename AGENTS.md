# SQLiteGraph Agent Standards (AGENTS.md)

This file is for any AI agent working on the SQLiteGraph project.

## Mandatory Workflow
1. Read [CLAUDE.md](CLAUDE.md) for the "Read-Plan-Prove-Implement-Verify" cycle.
2. Verify all work with the terminal. Do not guess.
3. Use `.magellan/sqlitegraph.db` for code indexing and CFG analysis.
4. Maintain Native V3 as the production-grade storage backend.
5. Purge legacy meta-tooling (CodeMCP, SYNCORE) immediately if found.

## Verification Gate
Any PR or completion report MUST contain:
- [ ] Failing test (repro) output
- [ ] Passing test (fix) output
- [ ] `cargo bench --features v3-bench` results (if storage-related)
- [ ] `cargo clippy` and `cargo fmt` confirmation
