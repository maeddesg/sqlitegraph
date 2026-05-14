"""sqlitegraph — embedded graph database with HNSW vector search.

Python bindings to the `sqlitegraph` Rust crate. The compiled extension
lives at `sqlitegraph._native`; this module re-exports its public surface
under stable Python names.
"""

from ._native import (
    BackendError,
    Graph,
    GraphError,
    HnswIndex,
    InvalidArgumentError,
    NotFoundError,
)

__all__ = [
    "BackendError",
    "Graph",
    "GraphError",
    "HnswIndex",
    "InvalidArgumentError",
    "NotFoundError",
]
__version__ = "0.1.0"
