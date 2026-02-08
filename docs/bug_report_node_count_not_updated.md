# Bug Report: node_count Reset to 0 by Multiple GraphFile Instances

**Date:** 2026-02-08
**Severity:** High - Data corruption due to concurrent access
**Status:** FIXED
**Component:** `backend/native/graph_file/mod.rs`
**Version:** sqlitegraph 1.5.3 (fixed in this version)

## Summary

When multiple `GraphFile` instances access the same database file (e.g., main thread and watcher thread), the Drop implementation blindly writes the in-memory header to disk. The second instance (which never wrote any nodes) has `node_count=0` and overwrites the correct data from the first instance.

## Symptoms

1. Database shows `node_count=0` in header after process exits
2. Node data IS present on disk at the expected offsets
3. Status commands report 0 files/symbols despite data being present
4. Issue occurs even without crashes (during normal shutdown)

## Root Cause

**Two separate bugs:**

1. **Missing sync in write_header()** - `GraphFile::write_header()` only called `flush()` which writes to OS buffer, not `sync_all()` which guarantees data reaches disk. This meant header updates could be lost if the process exited before OS flush.

2. **Multiple GraphFile Drop corruption** - The Drop impl writes the in-memory header without checking if it's stale. When a second GraphFile instance is opened (e.g., by watcher thread for pub/sub), it reads the header from disk (which may be outdated) and writes it back on Drop, corrupting the file.

### Code Path

```rust
// In watcher thread (magellan src/indexer.rs:522)
let backend = NativeGraphBackend::open(&db_path)?;  // Opens second GraphFile

// When watcher thread exits, Drop runs:
impl Drop for GraphFile {
    fn drop(&mut self) {
        let _ = self.write_header();  // Writes stale header with node_count=0!
        let _ = self.sync();
    }
}
```

## Fix Applied

1. **Added sync_all() to write_header()** - Ensures header reaches disk before Drop
2. **Added guard to Drop impl** - Skips header write if `node_count=0`, preventing read-only instances from corrupting data

```rust
impl Drop for GraphFile {
    fn drop(&mut self) {
        // Don't overwrite if this instance never wrote any nodes
        if self.persistent_header.node_count == 0 {
            return;
        }
        let _ = self.write_header();
        let _ = self.sync();
    }
}
```

## Testing

```bash
# Before fix:
# node_count=0 after crash, data present but unreadable

# After fix:
magellan watch --root /tmp/test --db /tmp/test.db --debounce-ms 100
# Ctrl+C to exit (even with tcache crash)
magellan status --db /tmp/test.db
# Now correctly shows: files: 1, symbols: 1
```

## Remaining Issues

1. **tcache_thread_shutdown crash** - This is a separate tree-sitter library issue (GitHub #3359)
   - Occurs during glibc TLS cleanup
   - Happens AFTER Rust Drop completes
   - Does not cause data loss with the fixes applied

2. **Heuristic fix** - The Drop guard only checks `node_count==0`, which is a heuristic.
   - Better solutions would use file locking, dirty flags, or shared GraphFile instances

## Related

- Original analysis incorrectly attributed issue to Drop not completing due to crash
- Actual issue was multiple GraphFile instances corrupting each other's data
- Commit: f08bd92 "fix: prevent multiple GraphFile instances from corrupting header on Drop"
