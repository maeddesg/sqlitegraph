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

## Examples

The [`examples/`](./examples/) directory contains runnable scripts:

| Example | What it shows |
|---------|---------------|
| [`01_basic_crud.py`](./examples/01_basic_crud.py) | Nodes, edges, update, delete, query by kind/pattern, degrees |
| [`02_graph_algorithms.py`](./examples/02_graph_algorithms.py) | BFS, k-hop, shortest path, PageRank, Louvain communities, connected components |
| [`03_vector_search.py`](./examples/03_vector_search.py) | HNSW index creation, insert, search, bulk insert, index listing |
| [`04_social_network.py`](./examples/04_social_network.py) | Realistic network: influencers (PageRank), communities, connection paths, mutual follows |
| [`05_file_backed.py`](./examples/05_file_backed.py) | Persistent `Graph.open(path)`, checkpoint, reopen, cleanup |

Run any example from the repo root:

```bash
cd sqlitegraph-py
source .venv/bin/activate
python examples/01_basic_crud.py
```
