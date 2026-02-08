# Bug Report: node_count Not Updated When Process Crashes

**Date:** 2026-02-08
**Severity:** High - Data Loss during abnormal termination
**Component:** `backend/native/`, glibc thread-local storage
**Version:** sqlitegraph 1.5.2

## Summary

When the process crashes due to `tcache_thread_shutdown: unaligned tcache chunk detected` error during shutdown, the Drop implementation may not complete, causing the header to not be written with the final `node_count`. This results in data being written to disk but appearing as 0 counts when the database is reopened.

## Symptoms

1. `tcache_thread_shutdown: unaligned tcache chunk detected` error appears during shutdown
2. Database shows `node_count=0` in header after crash
3. Node data IS present on disk at the expected offsets
4. After clean shutdown (no crash), `node_count` is correctly updated

## Root Cause

The `tcache_thread_shutdown` crash occurs in glibc during thread-local storage cleanup, which happens AFTER Rust Drop implementations run. However, if the crash is severe enough, it can prevent file operations from completing, causing the header write to be incomplete.

The crash is related to tree-sitter parser thread-local storage and occurs when the process exits, even during graceful shutdown with SIGINT.

## Reproduction Steps

```bash
# This issue is intermittent and depends on timing
# It typically occurs when the process is interrupted during active watching

# Create a database and start watching
magellan watch --root /tmp/test_dir --db /tmp/test.db --debounce-ms 200 &
WATCHER_PID=$!

# Add some files quickly
echo "pub fn test1() {}" > /tmp/test_dir/test1.rs
echo "pub fn test2() {}" > /tmp/test_dir/test2.rs

# Interrupt immediately (increases chance of crash)
sleep 1
kill -INT $WATCHER_PID

# Check database - may show 0 counts if crash occurred
magellan status --db /tmp/test.db
```

## Evidence

### Case 1: Successful Shutdown (node_count updated correctly)
```
=== Database header ===
00000010: 0000 0000 0000 0002 0000 0000 0000 0000
node_count = 2 (big-endian)

=== Status ===
files: 1
symbols: 1
references: 0
```

### Case 2: Crash During Shutdown (node_count = 0)
```
=== Database header ===
00000010: 0000 0000 0000 0000 0000 0000 0000 0000
node_count = 0 (big-endian)

=== Status ===
files: 0
symbols: 0
references: 0
```

## Workaround

There is no direct workaround in application code since the crash occurs in glibc during thread cleanup. However:

1. **Avoid SIGTERM** - Use SIGINT (Ctrl+C) instead of `kill` to allow graceful shutdown
2. **Wait before exit** - Give the process time to clean up after SIGINT
3. **Manual recovery** - If data exists but header shows 0, manually patch the header

### Manual Header Recovery

```bash
# If node_count is 0 but data exists, manually patch the header
# This example sets node_count to 1 (adjust value as needed)
printf "\x01\x00\x00\x00\x00\x00\x00\x00" | dd of=/path/to/db.db bs=1 count=8 seek=16 conv=notrunc
```

## Related Issues

1. **`tcache_thread_shutdown` crash** - This is a glibc memory corruption issue during thread-local storage cleanup
2. **Tree-sitter parser cleanup** - The crash is related to tree-sitter's use of thread-local storage

## Potential Fixes

1. **Explicit sync before crash** - Ensure header is written before thread cleanup begins
2. **Signal handler improvement** - Allow more time for cleanup after SIGINT/SIGTERM
3. **Tree-sitter cleanup** - Ensure parsers are cleaned up before glibc TLS cleanup

## Resolution

This is NOT a bug in the sqlitegraph header write logic. The header is correctly updated during normal operation. The issue is that process crashes during cleanup prevent the final header write from completing.

The Drop implementation at `mod.rs:217-222` correctly calls `write_header()` and `sync()`, but if the process crashes during glibc cleanup, these operations may not complete.

## Recommended Action

Focus on fixing the `tcache_thread_shutdown` crash (tree-sitter TLS cleanup issue) rather than modifying the header write logic.

