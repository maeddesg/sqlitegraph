# External Integrations

**Analysis Date:** 2026-02-11

## APIs & External Services

**None (self-contained)**

SQLiteGraph is designed as a fully embedded database with no external API dependencies or service integrations. All functionality is implemented in pure Rust or through embedded libraries.

## Data Storage

**Databases:**

### SQLite (Primary Backend)
- **Client:** rusqlite 0.31 with bundled libsqlite3
- **Connection:** `rusqlite::Connection` with OpenFlags
- **ORM/Client:** Direct SQL queries (no ORM layer)
- **Tables:** `entities`, `edges`, `hnsw_indexes`, `hnsw_vectors`, `hnsw_layers`, `hnsw_entry_points`, `graph_meta_history`
- **File extension:** `.db`
- **Location:** User-provided path or in-memory (`:memory:`)

### Native V2 Backend (Custom)
- **Format:** Custom binary format with clustered adjacency
- **File structure:** `FileHeader` + reserved regions + node storage + edge clusters
- **Connection:** Direct file I/O via `std::fs` and `memmap2`
- **WAL:** Write-Ahead Logging for crash recovery
- **File extension:** `.db` (same as SQLite)
- **No external database dependency**

**File Storage:**

| Data Type | Storage Location | Format |
|------------|------------------|---------|
| Graph data | User-provided `.db` path | SQLite DB or custom binary |
| HNSW vectors | SQLite `hnsw_vectors` table (SQLite backend) | BLOB |
| HNSW layers | SQLite `hnsw_layers` table (SQLite backend) | BLOB |
| Snapshot exports | JSON file (user path) | JSON |
| WAL logs | `.db-wal` (SQLite), `wal.bin` (Native V2) | Custom binary |

**Caching:**

| Cache Type | Implementation | Location |
|------------|-----------------|------------|
| Adjacency cache | LRU-K (`sqlitegraph/src/cache.rs`) | In-memory |
| Node cache | `NodeRecordCache` (native backend) | In-memory |
| Query cache | `QueryCache` (`sqlitegraph/src/query_cache.rs`) | In-memory |
| Connection pool | r2d2 pool (SQLite backend) | In-memory |

## Authentication & Identity

**None**

SQLiteGraph is an embedded database with no authentication, user management, or external identity providers. Access control is provided by the host application through file system permissions.

## Monitoring & Observability

**Error Tracking:**
- No external error tracking service
- Errors returned as `SqliteGraphError` and `NativeBackendError` enums
- Error types defined in `sqlitegraph/src/errors.rs`

**Logging:**
- log 0.4 facade (debug feature only)
- Production builds have zero logging overhead
- No log aggregation service

**Introspection:**
- `GraphIntrospection` API (`sqlitegraph/src/introspection.rs`)
- JSON-serializable state snapshots
- Cache statistics, file sizes, node/edge counts

**Progress Tracking:**
- `ProgressCallback` trait (`sqlitegraph/src/progress.rs`)
- `ConsoleProgress` for CLI output
- `NoProgress` zero-overhead implementation

## CI/CD & Deployment

**Hosting:**
- crates.io (primary package registry)
- GitHub repository: https://github.com/oldnordic/sqlitegraph

**CI Pipeline:**
- No GitHub Actions workflows detected (`.github/` directory absent)
- Manual release process based on CHANGELOG.md

**Release Process:**
- Version bumped in `Cargo.toml` files
- CHANGELOG.md updated with version notes
- Published via `cargo publish`

## Environment Configuration

**Required env vars:**
- None (configuration via command-line arguments)

**Optional env vars:**
- `PROJECT_NAME` - For development tooling (default: `sqlitegraph`)
- `DB_DIR` - Database directory for tooling (default: `.codemcp`)
- `MAGELLAN_DB` - Path to Magellan code graph database

**Secrets location:**
- No secrets storage required (embedded database)

## Webhooks & Callbacks

**Incoming:**
- None (no HTTP server)

**Outgoing:**
- None (no HTTP client)

**In-Process Pub/Sub (Native V2 Backend):**

| Event Type | Trigger | Payload |
|------------|----------|----------|
| `NodeChanged` | Node insert/update/delete | `node_id: u64` |
| `EdgeChanged` | Edge insert/delete | `from_id: u64`, `to_id: u64` |
| `KVChanged` | KV set/delete | `key_hash: u64` |
| `SnapshotCommitted` | Snapshot creation | `snapshot_id: u64` |

**Pub/Sub API:**
- `subscribe(filter)` - Subscribe with `SubscriptionFilter`
- `unsubscribe(subscriber_id)` - Remove subscription
- Channel-based delivery via `std::sync::mpsc`
- No cross-process delivery
- File: `sqlitegraph/src/backend/native/v2/pubsub/`

## Library Integrations

**Algorithm References:**

The codebase contains algorithm documentation references to external resources:

| Resource | URL | Purpose |
|----------|------|---------|
| CP-Algorithms | https://cp-algorithms.com/ | SCC and other algorithms reference |
| JGraphT | https://jgrapht.org/ | Cycle basis algorithm reference |
| petgraph docs | https://petgraph.github.io/petgraph/ | Isomorphism implementation reference |
| Wikipedia | https://en.wikipedia.org/ | Graph theory concepts |

**Development Tools (for development workflow, not runtime):**

| Tool | Purpose | Files |
|------|---------|--------|
| Magellan | Code graph indexing | `scripts/watch-magellan.sh`, `scripts/magellan-workflow.sh` |
| llmgrep | Semantic code search | Referenced in docs |
| Mirage | CFG analysis (planned) | Referenced in docs |
| splice | Precision code editing | Referenced in docs |

**Grounded Development Tools:**

Located at external repositories:
- Magellan: https://github.com/oldnordic/magellan (crates.io/crates/magellan)
- Splice: https://github.com/oldnordic/splice (crates.io/crates/splice)
- llmgrep: https://github.com/oldnordic/llmgrep (crates.io/crates/llmgrep)

## Database Schema

**SQLite Backend Schema:**

| Table | Purpose | Key Columns |
|-------|---------|--------------|
| `entities` | Node storage | id, kind, name, file_path, data |
| `edges` | Edge storage | id, from_id, to_id, edge_type, data |
| `graph_meta` | Schema version tracking | version, updated_at |
| `graph_meta_history` | Migration history | version, applied_at |
| `hnsw_indexes` | HNSW index metadata | id, name, dimension, m, ef_construction |
| `hnsw_vectors` | Vector storage | id, index_id, vector_data (BLOB) |
| `hnsw_layers` | HNSW graph layers | id, index_id, layer_level, connections |
| `hnsw_entry_points` | HNSW entry points | index_id, node_id |

**Native V2 Schema:**

| Region | Offset | Size | Purpose |
|---------|---------|-------|---------|
| Header | 0 | 256 bytes | Magic, version, metadata |
| Node region | 256 | 8MB | Node records (fixed 4KB slots) |
| Edge region | 256+8MB | Dynamic | Clustered edge storage |
| String table | End of file | Dynamic | String deduplication |
| Free space map | End of file | Dynamic | Free block tracking |
| WAL | Separate file | Dynamic | Transaction logging |

## Feature Flags

**Backend Selection:**
- `sqlite-backend` (default) - Enable SQLite storage
- `native-v2` - Enable Native V2 storage
- `all-backends` - Enable both backends

**Development Features:**
- `debug` - Enable debug/info logging
- `trace_v2_io` - Debug V2 I/O operations
- `bench-ci` - CI benchmarking mode
- `dependency-monitoring` - Enable dependency health checks

**I/O Options:**
- `v2_io_exclusive_std` - std::fs-based I/O (stable)
- `v2_io_exclusive_mmap` - Memory-mapped I/O (experimental)

---

*Integration audit: 2026-02-11*
