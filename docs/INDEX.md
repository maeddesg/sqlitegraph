# SQLiteGraph Documentation Index

**Last Updated:** 2026-01-26
**Version:** v1.2.0

This index provides navigation for all SQLiteGraph documentation.

---

## Quick Links

| Document | Purpose | Audience |
|----------|---------|----------|
| [README.md](../README.md) | Project overview, quick start | Users |
| [MANUAL.md](../MANUAL.md) | Comprehensive usage guide | Users |
| [API.md](../API.md) | Quick API reference | Users |
| [CHANGELOG.md](../CHANGELOG.md) | Version history | All |
| [ARCHITECTURE.md](ARCHITECTURE.md) | System architecture | Developers |
| [TESTING.md](TESTING.md) | Testing guide | Developers |
| [DEBUGGING.md](DEBUGGING.md) | Debugging and profiling | Developers |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contribution guidelines | Developers |

---

## For Users

### Getting Started

1. **[README.md](../README.md)** - Start here
   - Features overview
   - Installation
   - Quick start examples
   - Performance benchmarks

2. **[MANUAL.md](../MANUAL.md)** - Comprehensive guide
   - Backend selection (SQLite vs Native V2)
   - Core operations
   - Graph algorithms
   - Vector search (HNSW)
   - Pub/Sub events
   - CLI usage

3. **[API.md](../API.md)** - API reference
   - Quick type reference
   - Method signatures
   - Feature flags

### User Guides by Topic

| Topic | Document | Section |
|-------|----------|---------|
| **Quick Start** | README.md | Quick Start |
| **Backend Selection** | README.md | Backend Selection Guide |
| **CLI Usage** | MANUAL.md | CLI Usage |
| **Graph Algorithms** | MANUAL.md | Graph Algorithms |
| **Vector Search** | MANUAL.md | HNSW Vector Search |
| **Pub/Sub Events** | MANUAL.md | Section 14: Pub/Sub Events |
| **Error Handling** | MANUAL.md | Error Handling |
| **Troubleshooting** | MANUAL.md | Troubleshooting |

---

## For Developers

### Core Developer Documentation

1. **[ARCHITECTURE.md](ARCHITECTURE.md)** - System architecture
   - High-level overview
   - Directory structure
   - Backend architecture
   - Data flow
   - Design decisions

2. **[TESTING.md](TESTING.md)** - Testing guide
   - Test structure
   - Test utilities
   - Running tests
   - Writing tests
   - Benchmarking

3. **[DEBUGGING.md](DEBUGGING.md)** - Debugging guide
   - Debug builds
   - Logging
   - Introspection APIs
   - Profiling tools
   - Common issues

4. **[CONTRIBUTING.md](CONTRIBUTING.md)** - Contribution guidelines
   - Development workflow
   - Code standards
   - Submitting changes

### Development Guides by Topic

| Topic | Document | Section |
|-------|----------|---------|
| **Architecture** | ARCHITECTURE.md | High-Level Overview |
| **Backends** | ARCHITECTURE.md | Backend Architecture |
| **Data Flow** | ARCHITECTURE.md | Data Flow |
| **Testing** | TESTING.md | Running Tests |
| **Writing Tests** | TESTING.md | Writing Tests |
| **Benchmarking** | TESTING.md | Benchmarking |
| **Debug Builds** | DEBUGGING.md | Debug Builds |
| **Profiling** | DEBUGGING.md | Profiling |
| **Logging** | DEBUGGING.md | Logging |

---

## Historical Documentation

The `docs/` directory contains historical phase reports and research documents. These are organized by development phase and provide insight into design decisions and implementation history.

### Architecture & Design (Phase 0-9)

| Document | Phase | Description |
|----------|-------|-------------|
| `phase1_architecture_overview.md` | 1 | Initial architecture design |
| `phase1_backend_abstractions.md` | 1 | Backend trait design |
| `phase1_modularization_plan.md` | 1 | Module organization |
| `phase1_native_backend_file_format.md` | 1 | V1 file format design |
| `phase8_backend_selection_plan.md` | 8 | Dual backend decision |
| `phase9_public_api_freeze.md` | 9 | API stabilization |

### Performance (Phase 10-14)

| Document | Phase | Description |
|----------|-------|-------------|
| `phase10_performance_tuning.md` | 10 | Performance optimization work |
| `phase13_cpu_tuning_plan.md` | 13 | CPU-aware optimizations |
| `phase14_kernel_redesign_plan.md` | 14 | V2 format redesign |

### Bug Reports & Fixes

| Document | Description |
|----------|-------------|
| `BFS_NODE_257_CORRUPTION_FIX_REPORT.md` | BFS traversal bug fix |
| `CLUSTER_OFFSET_CORRUPTION_FIX_REPORT.md` | Cluster allocation bug |
| `NODE_SLOT_UNINITIALIZED_FIX_REPORT.md` | Node slot bug |
| `PHASE_HEADER_CORRUPTION_FIX_REPORT.md` | Header corruption fix |
| `TRANSACTION_BEGIN_NODE_CORRUPTION_FIX_REPORT.md` | Transaction bug fix |

### V2 Format

| Document | Description |
|----------|-------------|
| `V2_FIELD_MAPPING_TABLE.md` | V1 to V2 field mapping |
| `V2_INVARIANTS_MAP.md` | V2 format invariants |
| `V2_TEST_CLOSURE.md` | V2 testing completion |

### Verification & Completion

| Document | Description |
|----------|-------------|
| `FINAL_V2_VERIFICATION_REPORT.md` | V2 format verification |
| `PROJECT_HEALTH_REPORT.md` | Project health assessment |
| `COMPREHENSIVE_CODEBASE_INVESTIGATION.md` | Codebase analysis |

---

## Planning Documents

The `.planning/` directory contains project planning artifacts (not checked into git due to `.gitignore`).

- **ROADMAP.md** - Current roadmap and phase status
- **STATE.md** - Current project state
- **PROJECT.md** - Project overview and context
- **REQUIREMENTS.md** - Requirements traceability
- **phases/** - Phase-by-phase plans and summaries

---

## External Resources

- **[docs.rs/sqlitegraph](https://docs.rs/sqlitegraph)** - Full rustdoc API documentation
- **[crates.io/crates/sqlitegraph](https://crates.io/crates/sqlitegraph)** - Crate information
- **[GitHub Repository](https://github.com/yourusername/sqlitegraph)** - Source code

---

## Internal Documentation

The `docs/internal/` directory contains historical development artifacts, phase reports, bug analysis, and research documents. These are kept for reference but are not part of the user-facing or developer-facing documentation.

### Internal Docs by Category

| Category | Location | Description |
|----------|----------|-------------|
| **Architecture & Design** | `docs/internal/phase*_*.md` | Phase-by-phase architecture decisions |
| **Bug Reports & Fixes** | `docs/internal/*REPORT.md`, `docs/internal/*CORRUPTION*.md` | Bug fix reports and root cause analysis |
| **V2 Format** | `docs/internal/V2_*.md` | V2 format specifications and validation |
| **Performance** | `docs/internal/*PERFORMANCE*.md`, `docs/internal/*BENCH*.md` | Performance analysis and benchmarks |
| **Research** | `docs/internal/*RESEARCH*.md`, `docs/internal/*INVESTIGATION*.md` | Technical research and investigations |
| **Completion Reports** | `docs/internal/*COMPLETION*.md`, `docs/internal/*PROGRESS*.md` | Phase completion reports |

### Note on Internal Docs

These documents are **historical artifacts** from the development process. They provide insight into:
- Design decisions and trade-offs
- Bug discovery and resolution
- Performance optimization journey
- Development methodology

For current understanding of the system, prefer the **Developer Documentation** listed above.

---

## Document Conventions

### Code Blocks

```rust
// Rust code blocks show examples
let graph = SqliteGraph::open_in_memory()?;
```

```bash
# Bash blocks show commands
cargo test --workspace
```

### Tables

| Column 1 | Column 2 | Description |
|----------|----------|-------------|
| Value A  | Value B  | Tables show comparisons |

### Callouts

> **Note:** Important information that doesn't fit in flow.

> **Warning:** Cautionary information about potential pitfalls.

### Status Indicators

| Indicator | Meaning |
|-----------|---------|
| ✅ | Feature complete/implemented |
| 🚧 | Work in progress |
| ❌ | Not supported/known limitation |
| 📋 | Planned for future |

---

## Contributing to Documentation

Documentation improvements are welcome. Please:

1. Check existing docs before adding new content
2. Follow the style guide in CONTRIBUTING.md
3. Test code examples before submitting
4. Update the "Last Updated" date when modifying
5. Add entries to this index for new documents

---

## Changelog

See [CHANGELOG.md](../CHANGELOG.md) for detailed version history.

| Version | Date | Major Changes |
|---------|------|---------------|
| v1.2.0 | 2026-01-26 | Pub/Sub event system |
| v1.1.0 | 2025-XX-XX | ACID transaction correctness |
| v1.0.0 | 2025-XX-XX | Native V2 backend, initial release |
