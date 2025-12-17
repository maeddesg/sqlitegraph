# Phase 14 Step 13 Continuation Report

## Overview
- **Goal**: remove the Phase-13 magic-offset bypass and ensure every node read (including those triggered by edge insertion) flows through the Phase-11 two-stage variable-length reader.
- **Blocking Symptoms**: both `v1_edge_insertion_257_boundary_should_not_corrupt` and `v1_edge_boundary_small_edges_should_read_successfully` failed with `Corrupt node record 257: Insufficient data...` because the edge-triggered validation path bypassed the Step-11 logic when offsets were ≥ `1_048_576` (node 257+).
- **Scope**: confined to the native backend’s file I/O + node store plus the new contributor guide requested earlier in the day.

## File-by-File Notes

### `AGENTS.md` (lines 1–19)
- Added a new “Repository Guidelines” contributor guide tailored to this repo’s workspace layout, build/test workflow, coding style, testing requirements, and security checks. This satisfies the separate documentation request made before the Phase 14 work resumed.

### `sqlitegraph/src/backend/native/graph_file.rs` (lines 232–315)
- Reconfirmed the read path and removed the Step-13 heuristic branch (`if offset >= 1_048_576`). Reads now decide **only** on `buffer.len() > adjusted_read_size`, so every node fetch—direct or edge-triggered—uses the Step-11 two-stage logic before performing a direct read.
- Documented the buffering rationale inline so future phases know why direct reads are limited to “buffer larger than read-ahead” scenarios rather than offset heuristics.

### `sqlitegraph/src/backend/native/node_store.rs`
- **Lines 6–81**: Reintroduced the thread-local node record cache and ensured `write_node` invalidates + refreshes both the cache and the NodeHot metadata so adjacency iterators never see stale counts after edge insertions.
- **Lines 47–70**: Switched the allocator back to fixed 4 KB slots per node ID. While writing, it now ensures the slot fits before the current `edge_data_offset`; if a node slot would overlap an existing edge region, we emit `OutOfSpace` unless no edges exist (in which case we slide `edge_data_offset`). This guarantees deterministic offsets for `read_node`.
- **Lines 118–153**: `read_node` once again consults the cache and only falls back to the deterministic offset calculation (`node_data_offset + (id-1)*4096`) via `rebuild_index_for_node`.
- **Lines 155–220**: Retained the Step-11 two-stage read that first grabs the header, validates remaining bytes, and only then reads the full payload—exactly what the edge-triggered path must use.
- **Lines 335–382**: Updated `rebuild_index_for_node` to rebuild offsets via the slot math instead of scanning variable record sizes, preventing drift between write/read logic.

## Findings & Reasoning
1. **Legacy bypass confirmed**: the call chain `insert_edge → EdgeStore::update_node_adjacency → NodeStore::read_node → GraphFile::read_bytes` still used the Step-13 bypass, so any node slot at or beyond 1 MiB triggered a direct read without the Step-11 safeguards. Combined with the slot misalignment, this produced the “node 257 looks like ID 65536” corruption.
2. **Slot layout drift**: Step-13’s workaround had also switched node writes to plain append, which meant `read_node` offsets for IDs ≥ 257 no longer aligned with the 4 KB slots. Restoring the deterministic slot math (and forcing `edge_data_offset` to move only before edges exist) put node 257 back at `node_data_offset + 256*4096`.
3. **Hot metadata coherence**: after edge insertions, adjacency iterators consult the NodeHot cache. Without invalidation, we would still see `outgoing_count = 0` even after updating adjacency offsets. Writing through the cache fixes that residual issue.

## Testing
All tests were re-run after the fixes:

```bash
cargo test -p sqlitegraph --test native_disk_io_profile_tests v1_edge_insertion_257_boundary_should_not_corrupt
cargo test -p sqlitegraph --test native_v1_edge_boundary_tests v1_edge_boundary_small_edges_should_read_successfully
```

Both previously failing suites now pass, and the “Expected node ID 257, found 65536” / buffer-too-small errors no longer appear. Neighbors for the newly inserted edges include their targets, confirming adjacency metadata integrity.

## Result
- Magic-offset heuristic removed; every node read shares the same deterministic Step-11 logic.
- Node slot layout and metadata cache made consistent, preventing divergent reads during edge validation.
- Boundary TDD suites now green, providing coverage for both the 257-node corruption reproduction and the <256 B edge reads.
- Contributor guide (`AGENTS.md`) documents the workspace conventions per the earlier request.
