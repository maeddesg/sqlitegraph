# PR: Fix "entity not found" errors from entity_ids() method

## Issue Description

When `GraphBackend::entity_ids()` is called after nodes have been deleted, it returns entity IDs that no longer have corresponding nodes. This causes "entity not found" errors to be printed to stderr, even though the calling code handles missing nodes gracefully using `.ok()`.

## Severity

**Low** - This is a noisy warning issue, not a data corruption bug. Calling code can (and does) handle missing nodes correctly with `.ok()` filtering.

## Reproduction Steps

1. Create a sqlitegraph database with nodes
2. Delete some nodes
3. Call `entity_ids()` on the backend
4. Observe "entity not found: entity N" errors printed to stderr

```rust
let backend = GraphBackend::open(&db_path)?;
// After deletions, some entity IDs are stale
let ids = backend.entity_ids()?; // Returns stale IDs
for id in ids {
    let node = backend.get_node(id); // Prints "entity not found" errors
}
```

## Root Cause

The `entity_ids()` method returns IDs from an internal tracking table (`_sqlitegraph_entities`), but when nodes are deleted, their entries in this table are not cleaned up. This leaves stale entity IDs that don't correspond to actual nodes.

## Expected Behavior

**Option A:** `entity_ids()` should only return IDs for entities that still have nodes.

**Option B:** Provide a separate method like `entity_ids_validated()` that filters out stale IDs, and document `entity_ids()` as returning potentially stale IDs.

## Current Workaround

Calling code must filter out missing nodes:

```rust
let ids = backend.entity_ids()?;
let valid_nodes: Vec<_> = ids.iter()
    .filter_map(|id| backend.get_node(*id).ok())  // Filter missing nodes
    .collect();
```

This is the pattern used in Magellan after discovering this issue.

## Impact on Downstream Projects

- **Magellan v1.4 Phase 17:** This issue became more visible after implementing file-scoped counting, which calls `entity_ids()` more frequently (on every reconcile instead of just on deletes).
- **Any project using sqlitegraph with deletions:** Projects that delete nodes and then query `entity_ids()` will see these warnings.

## Suggested Fix

1. **In `GraphBackend::entity_ids()`:** Filter out stale entity IDs before returning
2. **Or:** Add cleanup to the deletion path to remove stale entity IDs from the tracking table
3. **Or:** Document that `entity_ids()` may return stale IDs and provide `entity_ids_validated()` alternative

## Testing

Write test that:
1. Creates nodes
2. Deletes some nodes
3. Calls `entity_ids()`
4. Verifies only valid IDs are returned (no "entity not found" errors)

## Related

- Magellan issue: Phase 17 exposure (file-scoped counting)
- sqlitegraph internal table: `_sqlitegraph_entities`

**Reported by:** @feanor (Magellan project)
**Date:** 2026-01-22
**sqlitegraph version:** v1.0.0
