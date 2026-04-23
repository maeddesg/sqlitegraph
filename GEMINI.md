# SQLiteGraph AI Standards (Gemini)

Refer to [CLAUDE.md](CLAUDE.md) as the primary source of truth for engineering standards. This file contains Gemini-specific reminders.

## Gemini-Specific Directives
- **Subagent Delegation**: For tasks involving >3 files, always dispatch a specialized subagent via the `generalist` or `codebase_investigator` tools.
- **Evidence Over Assertion**: Never say "Fixed" without providing the terminal output of `cargo test` and `cargo bench`.
- **V3 Native Backend**: This is the performance target. If `v3-bench` performance regresses, the change is invalid.

## Code Graph Manual
Refer to the `Code Graph Workflow` in `CLAUDE.md` for `magellan`, `llmgrep`, and `mirage` commands. Always use `.magellan/sqlitegraph.db` as the database path.
