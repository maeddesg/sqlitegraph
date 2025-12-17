# Phase 23.2 – Three-Way Performance Measurement Report

**Workloads Measured:** BFS chain 100 nodes, k-hop star 100 nodes

**Raw Results + Ratios:**
- BFS chain 100: sqlite 5.94 ms, native FAILED (Invalid node ID error), in-mem 2.67 µs (2,225× I/O overhead)
- k-hop star 100: sqlite 5.58 ms, native FAILED (Invalid node ID error), in-mem 3.57 µs (1,562× I/O overhead)

**Conclusion:**
- Native backend could not be measured due to Invalid node ID errors in Phase 23.1 fix
- Disk I/O shows ~1,500-2,200× overhead vs CPU ceiling for measured workloads
- SQLite and in-memory results confirm persistent I/O remains dominant performance gap