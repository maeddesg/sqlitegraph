# sqlitegraph

Python bindings to the [`sqlitegraph`](https://crates.io/crates/sqlitegraph)
embedded graph database. Storage, graph algorithms, and HNSW vector search
run in a reviewed Rust core; this package is the Pythonic surface.

> Alpha — API subject to change before 1.0.

## Install

    pip install sqlitegraph

## Quick start

```python
from sqlitegraph import Graph

g = Graph.open_in_memory()
alice = g.add_node(kind="User", name="Alice", data={"age": 30})
order = g.add_node(kind="Order", name="Order-123")
g.add_edge(alice, order, "placed")

print(g.neighbors(alice))
```

A polished README with vector-search and code-metadata examples will land
alongside the first PyPI release.
