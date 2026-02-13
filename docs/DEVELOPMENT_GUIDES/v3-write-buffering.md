# V3 Write Buffering Architecture

**Last Updated:** 2026-02-12  
**Version:** v1.6.0  
**Status:** Design Document

## Problem Statement

Current V3 implementation has synchronous disk I/O on every write:
- Each node insert triggers immediate disk write
- fsync() on every page operation
- No batching or coalescing

**Result:** 138x slower than SQLite for 10K node inserts

## Revised Solution: Transaction Batching (Minimal Fix)

Based on analysis, the simplest effective fix is transaction batching:

```
┌─────────────────────────────────────────────┐
│         Transaction Batching                │
├─────────────────────────────────────────────┤
│                                              │
│  BEFORE (138× slower):                      │
│    insert_node() → write → fsync            │
│    insert_node() → write → fsync            │
│    insert_node() → write → fsync            │
│                                              │
│  AFTER (target 3-5× slower):                │
│    begin_batch()                            │
│    insert_node() → mark dirty (in-mem)      │
│    insert_node() → mark dirty (in-mem)      │
│    insert_node() → mark dirty (in-mem)      │
│    commit_batch() → write all → fsync once  │
│                                              │
└─────────────────────────────────────────────┘
```

### Key Insight

**SQLite comparison issue:**
- SQLite benchmarks were run with **implicit transactions** (amortized)
- V3 benchmarks were run with **per-insert durability** (conservative)
- This is not an apples-to-apples comparison

**The fix:** Add explicit transaction batching so V3 can match SQLite's amortized commit model.

### Implementation: WriteBatch

```rust
/// A write batch that buffers page mutations
pub struct WriteBatch {
    /// Dirty pages accumulated in this batch
    dirty_pages: HashMap<u64, IndexPage>,
    /// Whether this batch has been committed
    committed: bool,
}

impl WriteBatch {
    /// Stage a page for writing (in-memory only)
    pub fn write_page(&mut self, page: IndexPage);
    
    /// Commit all pages in a single operation
    pub fn commit(self, db_path: &Path) -> Result<()>;
}

/// BTreeManager integration
impl BTreeManager {
    /// Start a new write batch
    pub fn begin_batch(&mut self) -> WriteBatch;
    
    /// Commit a batch (single fsync for all pages)
    pub fn commit_batch(&mut self, batch: WriteBatch) -> Result<()>;
}
```

### Durability Models

| Mode | Behavior | Use Case |
|------|----------|----------|
| **Autocommit** (current) | fsync per insert | Maximum durability, slow |
| **Explicit Batch** (new) | fsync at commit | Bulk inserts, fast |
| **Deferred** (future) | Async background flush | Mixed workloads |

### Expected Performance

**Before:**
- 10K inserts with autocommit: 4.3s (138× slower than SQLite)

**After (with batching):**
- 10K inserts in batch: ~50-100ms (2-3× slower than SQLite)
- Improvement: **40-80× faster**

### Testing Strategy

1. **Correctness:** Batched writes == individual writes (same result)
2. **Durability:** Data survives crash after commit
3. **Performance:** Batch much faster than individual
4. **SQLite Comparison:** Compare batched V3 vs batched SQLite

### Safety

- Pages are still checksummed
- WAL still provides crash recovery
- Batch commit is atomic (all pages or none)
- Graceful degradation: if batch fails, no partial writes

## API Design

```rust
/// Configuration for write buffering
pub struct WriteBufferConfig {
    /// Max dirty pages before forced flush (default: 64)
    pub buffer_capacity: usize,
    /// Pages per batch flush (default: 16)
    pub batch_size: usize,
    /// Auto-flush interval in ms (default: 10)
    pub flush_interval_ms: u64,
    /// Enable background thread (default: true)
    pub background_flush: bool,
}

/// Thread-safe write buffer
pub struct WriteBuffer {
    dirty_pages: RwLock<HashMap<u64, IndexPage>>,
    flush_tx: Sender<FlushCommand>,
    flush_rx: Receiver<FlushResult>,
    config: WriteBufferConfig,
}

impl WriteBuffer {
    /// Queue a page for writing (non-blocking)
    pub fn write_page(&self, page: IndexPage) -> Result<()>;
    
    /// Force immediate flush (blocking)
    pub fn flush(&self) -> Result<()>;
    
    /// Read page (from buffer or disk)
    pub fn read_page(&self, page_id: u64) -> Result<IndexPage>;
}
```

## Durability Guarantees

| Configuration | Durability | Performance |
|--------------|------------|-------------|
| `flush_interval_ms = 0` | Immediate (fsync every write) | Slow |
| `flush_interval_ms = 10` | 10ms window (may lose last 10ms) | Fast |
| `buffer_capacity = 1` | Immediate (no buffering) | Slow |
| `buffer_capacity = 64` | Buffered (flush when full) | Fast |
| `background_flush = false` | Synchronous writes | Slow |
| `background_flush = true` | Asynchronous writes | Fast |

**Default:** Balance durability and performance
- `flush_interval_ms = 10` (10ms window acceptable for most apps)
- `buffer_capacity = 64` (reasonable memory usage)
- `background_flush = true` (async for performance)

## Testing Strategy

1. **Unit Tests:**
   - Buffer fills correctly
   - Flush triggers at capacity
   - Read returns buffered or disk page
   - No data loss on flush

2. **Integration Tests:**
   - Concurrent writes/reads
   - Crash recovery (WAL replay)
   - Performance benchmarks

3. **Stress Tests:**
   - 100K inserts with various configs
   - Memory usage validation

## Implementation Phases

### Phase 1: Write Buffer Core
- WriteBuffer struct with HashMap storage
- Basic write/read operations
- Synchronous flush

### Phase 2: Batching
- Batch collection logic
- Batch write to disk
- Single fsync per batch

### Phase 3: Background Thread
- FlushWorker thread
- Channel communication
- Auto-flush on interval

### Phase 4: Integration
- Integrate with BTreeManager
- Config options in V3Backend
- Performance validation

## Expected Performance Improvement

**Before:**
- 10K inserts: 4.3 seconds (138x slower than SQLite)

**After (target):**
- 10K inserts: ~50-100ms (2-3x slower than SQLite)
- Improvement: 40-80x faster

## Safety Notes

1. **Crash Recovery:** WAL ensures durability even with buffering
2. **Data Integrity:** Checksums on all pages
3. **Memory Limits:** Bounded buffer prevents OOM
4. **Graceful Shutdown:** Flush all pending on drop
