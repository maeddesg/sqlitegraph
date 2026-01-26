# SQLiteGraph

## What This Is

SQLiteGraph is an embedded graph database library in Rust with dual backend support (SQLite and Native V2), HNSW vector search, and CLI interface. It provides high-performance graph operations with MVCC concurrency, WAL-based durability, and vector similarity search capabilities.

## Core Value

**Feature parity, performance, and reliability equally.** Native V2 must match or exceed SQLite backend capabilities while maintaining rock-solid MVCC correctness and achieving best-in-class embedded graph database performance.

## Current Milestone: v1.2 Benchmark Infrastructure

**Goal:** Fix broken benchmark harness to get complete performance numbers for honest public comparison.

**Target features:**
- Fix HNSW API mismatch (2 argument requirement)
- Fix Native V2 temp-file init pattern
- Run full benchmark suite for complete numbers
- Update documentation with honest performance claims

**Started:** 2026-01-20

---

## Validated

### v1.1 Shipped

**Shipped:** 2026-01-20

v1.1 ACID & Reliability is **COMPLETE** with all requirements satisfied. The milestone achieved full ACID transaction correctness for Native V2 backend with comprehensive reliability improvements.

<details>
<summary>v1.0 Production capabilities (shipped 2026-01-17)</summary>

- ✓ **Dual backend architecture** — GraphBackend trait with SQLite and Native V2 implementations
- ✓ **Core graph operations** — insert_node, get_node, neighbors, insert_edge, bfs
- ✓ **MVCC snapshot system** — ArcSwap<Arc<SnapshotState>> for concurrent reads
- ✓ **WAL for Native V2** — Write-Ahead Logging with recovery/replayer modules
- ✓ **HNSW vector search** — Hierarchical navigable small world index with persistence
- ✓ **Graph algorithms** — BFS, k-hop, shortest path, PageRank, Betweenness, Louvain, Label Propagation
- ✓ **CLI interface** — Command-line tool with backend selection
- ✓ **Configuration system** — Builder pattern for GraphConfig, SqliteConfig, NativeConfig
- ✓ **Error handling** — thiserror-based SqliteGraphError with comprehensive variants
- ✓ **Introspection APIs** — GraphIntrospection for LLM tooling
- ✓ **Progress tracking** — ProgressCallback for long-running operations

</details>

<details>
<summary>v1.1 ACID & Reliability (shipped 2026-01-20)</summary>

**ACID Transaction Guarantees:**
- ✓ **Atomicity** — Complete rollback for all operations (including node deletion)
- ✓ **Consistency** — Cluster overlap validation, checkpoint state validation, constraint enforcement
- ✓ **Isolation** — Concurrent write support, proper transaction isolation levels, deadlock detection
- ✓ **Durability** — All checkpoint strategies functional, WAL replay completeness

**Technical Debt (Resolved):**
- ✓ **HNSW multi-layer** — O(log N) search with exponential distribution
- ✓ **Checkpoint strategies** — All 3 strategies implemented (transaction-count, size-based, time-based)
- ✓ **Node deletion WAL replay** — Complete rollback with before-image capture
- ✓ **Cluster overlap validation** — Re-enabled with sequencing support
- ✓ **Checkpoint validation** — Fixed and enabled
- ✓ **Schema version size** — Migrated to 4-byte field with format version bump

**Security & Safety (Resolved):**
- ✓ **Unsafe transmute audit** — All 19 sites replaced with Arc<RwLock<GraphFile>>
- ✓ **Deadlock detection** — Implemented in transaction coordinator
- ✓ **Input sanitization** — JsonLimits with size/depth limits

**Performance & Structure (Resolved):**
- ✓ **Large file refactoring** — All 5 large files split
- ✓ **Clone operations** — All 263 clone() calls audited
- ✓ **Connection pooling** — Implemented with r2d2 (4-5x throughput improvement)

**Missing Features (Resolved):**
- ✓ **Concurrent write support** — Multi-writer with deadlock detection
- ✓ **Graph file migration** — Automated V2→V3 migration
- ✓ **Backup/Restore API** — Complete API implemented
- ✓ **Node deletion WAL replay** — Complete crash recovery consistency

**Test Coverage (Resolved):**
- ✓ **WAL edge cases** — All rollback scenarios tested
- ✓ **Cluster overlap validation** — Re-enabled and passing
- ✓ **Checkpoint transitions** — All validation enabled
- ✓ **HNSW multi-layer** — Comprehensive tests passing
- ✓ **Unsafe blocks** — Miri testing for all replaced transmutes

**Scaling Limits (Resolved):**
- ✓ **Checkpoint file size** — Multi-file checkpointing implemented
- ✓ **Dirty block tracking** — Overflow strategy with >50K support
- ✓ **WAL transaction coordinator** — ID bounds with cleanup
- ✓ **HNSW index size** — Documented disk-based options

**Dependencies (Resolved):**
- ✓ **rusqlite 0.31** — Monitoring documented, healthy
- ✓ **bincode 1.3** — 2.0 migration plan documented

</details>

### Active

**v1.2 Benchmark Infrastructure** (started 2026-01-20)

Focus: Fix broken benchmark harness to enable honest public performance comparison.

Run `/gsd:plan-phase 23` to start execution.

### Out of Scope

- **Breaking API changes** — Must maintain backward compatibility with existing databases and APIs
- **New external integrations** — Focus remains on embedded standalone database
- **Web services or network protocol** — In-process embedded database only
- **Alternative storage backends** — SQLite and Native V2 only

## Context

**Current codebase state (v1.1 shipped):**

- **Architecture:** Layered design with Public API → Backend Abstraction → Storage (SQLite/Native) → Infrastructure
- **File organization:** Modular structure in `sqlitegraph/src/` with backend/, graph/, hnsw/, pattern_engine/, config/
- **v1.1 achievements:**
  - Full ACID transaction correctness (Atomicity, Consistency, Isolation, Durability)
  - Transaction coordinator with deadlock detection and victim selection
  - All 19 unsafe transmute sites replaced with Arc<RwLock<GraphFile>>
  - All 5 large files refactored into focused submodules
  - Connection pooling with 4-5x throughput improvement
  - Multi-file checkpointing for >1GB databases
  - HNSW multi-layer with O(log N) search (100% recall)
  - 126 tests passing, comprehensive test suite
  - 83,865 LOC Rust

**Deferred items (v1.2 candidates):**
- HNSW-10: Layer persistence (requires database schema migration)
- Rollback state persistence (in-memory acceptable for v1.1)
- Cluster API persistence sync (architectural improvement)

**Technical environment:**
- Rust 2024 (library) / Rust 2021 (CLI), MSRV 1.70.0
- Dependencies: rusqlite 0.31, thiserror, serde, parking_lot, memmap2, criterion, r2d2
- Cross-platform: Linux, macOS, Windows
- No external services or cloud dependencies

## Constraints

- **Backward compatibility:** Must maintain compatibility with existing databases and APIs — no breaking changes to file formats or public interfaces
- **Timeline:** No deadline — systematic completion until everything is done
- **Performance:** Must meet or exceed current performance characteristics for all operations
- **Safety:** Eliminated all unsafe transmute sites; miri tests validate safety
- **Testing:** All code paths have test coverage per CONTRIBUTING.md

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Dual backend architecture | Leverages SQLite's maturity while enabling high-performance native path | ✓ Shipped (v1.0) |
| MVCC with ArcSwap | Lock-free snapshot updates for concurrent readers | ✓ Shipped (v1.0) |
| HNSW disk persistence | Simple design evolved to include persistence | ✓ Shipped (v1.0) |
| WAL for Native V2 | Enables durability and crash recovery | ✓ Shipped (v1.1) |
| No breaking changes | Existing users and databases must continue working | ✓ Enforced |
| Split large files | Reduces fragility, improves maintainability | ✓ Complete (v1.1) |
| Arc<RwLock<GraphFile>> | Replace unsafe transmute with safe shared ownership | ✓ Complete (v1.1) |
| Transaction coordinator | Deadlock detection with wait-for graph and victim selection | ✓ Complete (v1.1) |
| Connection pooling (r2d2) | Reduce SQLite connection overhead for concurrent access | ✓ Complete (v1.1) |
| Multi-file checkpoint | Support databases >1GB | ✓ Complete (v1.1) |

---
*Last updated: 2026-01-20 for v1.2 Benchmark Infrastructure milestone*
