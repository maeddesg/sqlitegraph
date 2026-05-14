# sqlitegraph (Python) — Changelog

This file tracks releases of the `sqlitegraph` package on PyPI. The Rust
crate of the same name has its own changelog at the repository root.

## [0.1.0] — unreleased

### Added
- First Python release. Thin wrapper around the audited Rust core via PyO3
  + maturin. Single `abi3` wheel per platform; works on Python 3.10+.
- `Graph` class:
  - File-backed (`Graph.open(path)`) and in-memory (`Graph.open_in_memory()`).
  - Node CRUD (`add_node`, `get_node`, `update_node`, `delete_node`,
    `node_ids`, `nodes_by_kind`, `nodes_by_name_pattern`, `node_degree`).
  - Edge CRUD (`add_edge`, `get_edge`, `delete_edge`, `neighbors`).
  - Traversal (`bfs`, `k_hop`, `shortest_path`).
  - Algorithms (`pagerank`, `louvain_communities`, `connected_components`).
  - HNSW vector indexes (`create_hnsw_index`, `get_hnsw_index`,
    `list_hnsw_indexes`).
- `HnswIndex` class: `insert_vector`, `bulk_insert_vectors`, `search`,
  `get_vector`, `vector_count`, `name`.
- Typed exception hierarchy: `GraphError` (base), `NotFoundError`,
  `InvalidArgumentError`, `BackendError`.
- Type stubs (`_native.pyi`) and `py.typed` marker for editor support.
- 39 pytest tests covering CRUD, traversal, algorithms, HNSW, and the
  exception hierarchy.

### Notes
- Built against `sqlitegraph` (Rust) **v2.2.4**.
- The optional `inference` cargo feature pulls in `numpy` + `ndarray` for
  the experimental sparse-inference engine; it is **off by default** in
  PyPI wheels.
