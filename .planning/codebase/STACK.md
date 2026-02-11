# Technology Stack

**Analysis Date:** 2026-02-11

## Languages

**Primary:**
- Rust 2024 Edition (Rust 1.93.0) - Core library and all storage backends

**Secondary:**
- Python 3 - Benchmarking scripts in `scripts/`
- Shell (bash) - Development workflow scripts

## Runtime

**Environment:**
- Linux x86_64 (kernel 6.18+ for optimal performance)
- Cross-platform: Linux, macOS, Windows

**Package Manager:**
- Cargo 1.93.0
- Lockfile: `Cargo.lock` (present)
- Workspace: 2-member workspace (`sqlitegraph`, `sqlitegraph-cli`)

**Cargo Configuration:**
- Build target dir: `target/` (`.cargo/config.toml`)
- Resolver: "3" (workspace resolver)

## Frameworks

**Core:**
- sqlitegraph (v1.6.0) - Main graph database library
- sqlitegraph-cli (v1.6.0) - Command-line interface

**Testing:**
- Criterion 0.5 - Statistical benchmarking framework
- assert_cmd 2.1 - CLI testing
- tempfile 3.23 - Test file isolation

**Build/Dev:**
- clap 4.5 - CLI argument parsing (derive feature)
- anyhow 1.0 - Error handling for CLI

## Key Dependencies

**Critical:**

| Package | Version | Purpose |
|---------|----------|---------|
| rusqlite | 0.31 | SQLite backend with bundled libsqlite3 |
| petgraph | 0.6 | Graph algorithms library (SCC, dominators, etc.) |
| rayon | 1.10 | Parallel data processing |

**Infrastructure:**

| Package | Version | Purpose |
|---------|----------|---------|
| r2d2 | 0.8 | Connection pooling for SQLite backend |
| r2d2_sqlite | 0.24 | SQLite adapter for r2d2 |
| arc-swap | 1.7 | Lock-free atomic updates (MVCC) |
| parking_lot | 0.12 | Fast mutexes and RwLocks |
| ahash | 0.8 | Fast non-cryptographic hashing |

**Serialization:**

| Package | Version | Purpose |
|---------|----------|---------|
| serde | 1.0 | Framework for data serialization |
| serde_json | 1.0 | JSON support |
| bincode | 1.3 | Binary serialization |
| binrw | 0.13 | Binary read/write for native format |
| bytemuck | 1.13 | Zero-copy byte casting |

**I/O:**

| Package | Version | Purpose |
|---------|----------|---------|
| memmap2 | 0.9 | Memory-mapped file I/O for native backend |
| rand | 0.8 | Random number generation |

**Error Handling:**
- thiserror 1.0 - Derive macros for error types

**Logging:**
- log 0.4 - Logging facade (debug feature disabled in release)

## Configuration

**Environment:**
- No .env file required for core library
- CLI uses command-line arguments (clap)
- Backend selection via feature flags

**Key configs required:**
- `Cargo.toml` - Feature flags for backend selection
- `.cargo/config.toml` - Build directory configuration

**Build:**
- Workspace root `Cargo.toml` with shared profiles
- Custom profiles: `release`, `bench`, `test`

**Profile Configuration:**

```toml
[profile.release]
opt-level = 3
codegen-units = 1
lto = "thin"
panic = "abort"

[profile.bench]
inherits = "release"
opt-level = 3
codegen-units = 1
lto = "thin"
debug = true  # For flamegraph profiling

[profile.test]
opt-level = 2  # Faster test execution
```

## Platform Requirements

**Development:**
- Rust 1.93.0+ (rustc, cargo)
- SQLite CLI (for manual database inspection)
- Python 3+ (for benchmark scripts)

**Production:**
- Linux x86_64 (primary target)
- macOS, Windows (cross-platform support)
- No external runtime dependencies for native backend
- libsqlite3 embedded (bundled feature)

**Target Platforms:**
- `x86_64-unknown-linux-gnu` (primary)
- `aarch64-unknown-linux-gnu` (ARM support)
- `x86_64-apple-darwin` (macOS)
- `x86_64-pc-windows-msvc` (Windows)

---

*Stack analysis: 2026-02-11*
