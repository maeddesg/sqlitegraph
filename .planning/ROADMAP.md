# Roadmap: SQLiteGraph

## Overview

SQLiteGraph roadmap. Phases 1-57 completed across milestones v0.2-v1.3.0.

## Milestones

- ✅ **v1.0 MVP** - Phases 1-10 (shipped 2026-01-17)
- ✅ **v1.1 ACID & Reliability** - Phases 11-22 (shipped 2026-01-20)
- ✅ **v1.2 Benchmark Infrastructure** - Phases 23-24 (shipped 2026-01-21)
- ✅ **v1.3 Chain Traversal Performance** - Phases 25-29 (shipped 2026-01-21)
- ✅ **v1.4 Sequential I/O Optimization** - Phases 30-32 (shipped 2026-01-21)
- ✅ **v1.6 Chain Locality** - Phases 33-36 (shipped 2026-01-21)
- ✅ **v1.13 Pub/Sub** - Phases 37-44 (shipped 2026-01-26)
- ✅ **v1.3.0 Graph Algorithms Library** - Phases 45-57 (shipped 2026-02-03) — *35 algorithms for CFG analysis, program slicing, security*

## Next Milestone

**Status:** Phase 59 - Test Suite Recovery

**Phase 59:** Test Suite Recovery — Fix broken test modules, enable CI/CD, unlock Phase 58 test verification

**Plans:** 5 plans in 4 waves

Plans:
- [ ] 59-01-PLAN.md — Fix V2WALConfig struct initialization errors
- [ ] 59-02-PLAN.md — Fix GraphEntityCreate import errors
- [ ] 59-03-PLAN.md — Fix natural_loops_from_exit import errors
- [ ] 59-04-PLAN.md — Fix KvStore/KvValue import errors (Phase 58 tests)
- [ ] 59-05-PLAN.md — Verify test suite compiles and runs

See [v1.4.0 Roadmap](.planning/milestones/v1.4.0-ROADMAP.md) for details.

---

## Archive

See [milestones/](.planning/milestones/) directory for complete phase details:
- [v0.2-v1.13 Archive](.planning/milestones/)
- [v1.3.0 Graph Algorithms Library](.planning/milestones/v1.3.0-ROADMAP.md) — 35 algorithms, 13 phases, 36 plans
- [v1.4.0 Pub/Sub Enhancements](.planning/milestones/v1.4.0-ROADMAP.md) — Pattern subscriptions, KV queries
