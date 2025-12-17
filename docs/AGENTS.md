# Repository Guidelines

## Project Structure & Module Organization
The workspace root (`Cargo.toml`) wires two crates: `sqlitegraph/` (core backend) and `sqlitegraph-cli/` (thin CLI). Core code lives under `sqlitegraph/src/` with modules capped at ~300 LOC; deterministic integration tests sit in `sqlitegraph/tests/`. Workspace-level specs (pipeline, DSL, reasoning) live in `tests/`, while operational docs/manuals are under `docs/` and `manual.md`. Sample databases (`example_sqlite.db`, `example_native.db`) plus `scripts/` support benchmarking and safety tooling.

## Build, Test, and Development Commands
Use `cargo fmt --all` before reviews, then `cargo clippy --workspace --all-features -- -D warnings` to keep lint passes fail-fast. `cargo test --workspace` executes every crate, and focused suites such as `cargo test --test pipeline_tests` or `cargo test -p sqlitegraph --test perf_gate_tests` target regressions faster. Run curated demos with `cargo run --example basic_usage` or CLI smoke tests via `cargo run -p sqlitegraph-cli -- --command status`. Deterministic benches are available through `cargo bench` (Criterion writes to `target/criterion`).

## Coding Style & Naming Conventions
Rustfmt defaults (4-space indent, trailing commas) are mandatory; never hand-edit formatting. Prefer `snake_case` for modules/functions, `CamelCase` for types, and `SCREAMING_SNAKE_CASE` for consts. Keep modules small, document invariants inline, and avoid nondeterministic iteration (always sort before exposing collections). Commit only deterministic changes; reproducible seeds are required for generators and tests.

## Testing Guidelines
Rely on real SQLite storage—no mocks or fakes. Integration files under `sqlitegraph/tests/` cover algorithms, migrations, and perf gates, while workspace `tests/*.rs` exercise public ergonomics. Add targeted cases near existing suites (e.g., `tests/dsl_tests.rs`) and name new files `*_tests.rs`. CI expects `cargo test --workspace` and `cargo doc --workspace --no-deps` to succeed before review.

## Commit & Pull Request Guidelines
Follow the linear history model: short-lived branches, fast-forward merges. Commit messages stay under ~50 characters, imperative (“fix mvcc snapshot”) and reference an issue when applicable. PRs must describe motivation, enumerate test commands run, and attach benchmark diffs whenever touching performance-sensitive code (`sqlitegraph_bench.json`).

## Security & Safety Checks
Consult `SECURITY.md` plus `manual.md` for invariants. Always run `sqlitegraph --command safety-check --strict --db <path>` before publishing storage changes, and use `sqlitegraph --command migrate --dry-run` prior to touching schema files. When perf tuning, document `bench_gates::check_thresholds` output and update `sqlitegraph_bench.json` only alongside justification.
