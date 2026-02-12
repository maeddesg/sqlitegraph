# Project State

**Last Updated**: 2026-02-12

## Current Milestone

**v1.3.0: Graph Algorithms Library** (SHIPPED 2026-02-03)

## Current Phase

**Phase 58: Core Technical Debt** (3 of 5 plans complete)

### Progress

```
Phase 58: [=========================================----] 80%
  58-01: [COMPLETED] bincode 1.3 to 2.0 migration
  58-02: [COMPLETED] WAL snapshot isolation
  58-03: [COMPLETED] Node deletion with edge cleanup
  58-04: [COMPLETED] Transaction rollback for KV store
  58-05: [COMPLETED] Deadlock detection enhancement
  58-06: [PENDING]
```

## Accumulated Context

### Roadmap Evolution

- Phase 58 added: Core Technical Debt (2026-02-11)

### Key Decisions

- Technical debt cleanup prioritized after algorithms library completion
- Focus on correctness over new features
- **2026-02-11**: Migrated from bincode 1.3 to 2.0 with custom BincodeError wrapper to handle separate EncodeError/DecodeError types
- **2026-02-12**: Added commit_lsn field to TransactionCommit for snapshot isolation. TransactionCommit records now track LSN at commit time. Added snapshot_id filtering to WALReadFilter. SnapshotId can query TxRangeIndex for max_committed_lsn.
- **2026-02-12**: Implemented resource-specific deadlock detection with resource_wait_graph (ResourceId -> HashSet<TransactionId>) and tx_waiting_for (TransactionId -> HashSet<ResourceId>) mappings. Added LockTypeValidator with can_upgrade() and has_conflict() for multi-granularity locking (IS, IX, S, X).

### Performance Metrics

| Phase | Plan | Tasks | Duration |
|--------|-------|-------|----------|
| 58 | 58-01 | 6 | ~9 minutes |
| 58 | 58-03 | 6 | ~14 minutes |
| 58 | 58-02 | 4 | ~3 minutes |
| 58 | 58-05 | 6 | ~10 minutes |

## Session History

See git log for full history.

**Last Session**: Completed 58-05 Deadlock detection enhancement (2026-02-12)
