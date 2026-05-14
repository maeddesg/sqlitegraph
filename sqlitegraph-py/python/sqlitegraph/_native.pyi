"""Type stubs for the compiled `sqlitegraph._native` extension."""

from typing import Any

NodeId = int
EdgeId = int
VectorId = int

# ── Exceptions ───────────────────────────────────────────────────

class GraphError(Exception):
    """Base class for every exception raised by ``sqlitegraph``."""

    ...

class NotFoundError(GraphError):
    """Raised when a node, edge, or index does not exist."""

    ...

class InvalidArgumentError(GraphError):
    """Raised for malformed input, validation failures, or duplicate indexes."""

    ...

class BackendError(GraphError):
    """Raised for storage, corruption, or unsupported-operation failures."""

    ...

class Graph:
    """An embedded graph database with optional HNSW vector indexes."""

    @staticmethod
    def open(path: str) -> Graph:
        """Open or create a file-backed graph database."""
        ...

    @staticmethod
    def open_in_memory() -> Graph:
        """Create an in-memory graph (no persistence)."""
        ...

    # ── Node operations ──────────────────────────────────────────

    def add_node(
        self,
        kind: str,
        name: str,
        data: dict[str, Any] | None = None,
    ) -> NodeId:
        """Insert a node. Returns the new node's ID."""
        ...

    def get_node(self, id: NodeId) -> dict[str, Any]:
        """Fetch a node by ID. Keys: ``id``, ``kind``, ``name``, ``data``, optional ``file_path``."""
        ...

    def delete_node(self, id: NodeId) -> None:
        """Delete a node and all its incident edges."""
        ...

    def node_ids(self) -> list[NodeId]:
        """Return all node IDs in the graph."""
        ...

    def nodes_by_kind(self, kind: str) -> list[NodeId]:
        """Return node IDs matching the given kind."""
        ...

    def nodes_by_name_pattern(self, pattern: str) -> list[NodeId]:
        """Return node IDs whose name matches a SQL ``GLOB`` pattern.

        ``*`` matches any sequence; ``?`` matches a single character.
        Example: ``"Al*"`` matches ``"Alice"`` and ``"Albert"``.
        """
        ...

    def update_node(
        self,
        id: NodeId,
        kind: str,
        name: str,
        data: dict[str, Any] | None = None,
    ) -> NodeId:
        """Update an existing node in place. Returns the same ID."""
        ...

    def node_degree(self, node_id: NodeId) -> tuple[int, int]:
        """Return ``(in_degree, out_degree)`` for a node."""
        ...

    def shortest_path(self, start: NodeId, end: NodeId) -> list[NodeId] | None:
        """Shortest path as a list of node IDs, or ``None`` if no path exists."""
        ...

    # ── Edge operations ──────────────────────────────────────────

    def add_edge(
        self,
        from_id: NodeId,
        to_id: NodeId,
        edge_type: str,
        data: dict[str, Any] | None = None,
    ) -> EdgeId:
        """Insert an edge. Returns the new edge's ID."""
        ...

    def get_edge(self, id: EdgeId) -> dict[str, Any]:
        """Fetch an edge. Keys: ``id``, ``from_id``, ``to_id``, ``edge_type``, ``data``."""
        ...

    def delete_edge(self, id: EdgeId) -> None:
        """Delete an edge by ID."""
        ...

    def neighbors(
        self,
        node_id: NodeId,
        edge_type: str | None = None,
        direction: str | None = None,
    ) -> list[NodeId]:
        """Return neighbor IDs. ``direction`` is ``"outgoing"`` (default) or ``"incoming"``."""
        ...

    # ── Traversal ────────────────────────────────────────────────

    def bfs(self, start: NodeId, depth: int) -> list[NodeId]:
        """Breadth-first traversal from ``start`` up to ``depth`` hops."""
        ...

    def k_hop(
        self,
        start: NodeId,
        depth: int,
        direction: str | None = None,
    ) -> list[NodeId]:
        """Return nodes reachable within exactly ``depth`` hops."""
        ...

    # ── Graph algorithms ─────────────────────────────────────────

    def pagerank(
        self,
        damping: float | None = None,
        iterations: int | None = None,
    ) -> list[tuple[NodeId, float]]:
        """PageRank scores. Defaults: damping=0.85, iterations=20."""
        ...

    def louvain_communities(self, max_iterations: int | None = None) -> list[list[NodeId]]:
        """Louvain communities. Default: max_iterations=10."""
        ...

    def connected_components(self) -> list[list[NodeId]]:
        """Forward-reachable connected components."""
        ...

    # ── HNSW vector index ────────────────────────────────────────

    def create_hnsw_index(
        self,
        name: str,
        dimension: int,
        m: int | None = None,
        ef_construction: int | None = None,
        metric: str | None = None,
    ) -> HnswIndex:
        """Create a new HNSW vector index.

        ``metric`` is one of ``"cosine"`` (default), ``"euclidean"``, ``"dot"``.
        """
        ...

    def get_hnsw_index(self, name: str) -> HnswIndex:
        """Look up an existing HNSW index by name."""
        ...

    def list_hnsw_indexes(self) -> list[str]:
        """Return the names of all HNSW indexes attached to this graph."""
        ...

    # ── Maintenance ──────────────────────────────────────────────

    def checkpoint(self) -> None:
        """Force a WAL checkpoint (flush pending writes to the main DB file)."""
        ...

class HnswIndex:
    """A vector index attached to a :class:`Graph`."""

    def name(self) -> str:
        """Index name."""
        ...

    def vector_count(self) -> int:
        """Live count of vectors in the index."""
        ...

    def insert_vector(
        self,
        vector: list[float],
        metadata: dict[str, Any] | None = None,
    ) -> VectorId:
        """Insert one vector. Returns its ID."""
        ...

    def bulk_insert_vectors(
        self,
        items: list[tuple[list[float], dict[str, Any] | None]],
    ) -> list[VectorId]:
        """Insert many ``(vector, metadata)`` pairs in a single call."""
        ...

    def bulk_insert_numpy(self, array: Any) -> list[VectorId]:
        """Insert each row of a 2-D numpy float32 array.

        ``array`` must be a contiguous ``numpy.ndarray`` of shape
        ``(n_vectors, dimension)`` and dtype ``float32``.
        """
        ...

    def search(self, query: list[float], k: int) -> list[tuple[VectorId, float]]:
        """Return the ``k`` nearest neighbors as ``(vector_id, distance)`` tuples."""
        ...

    def get_vector(self, vector_id: VectorId) -> tuple[list[float], dict[str, Any]] | None:
        """Fetch a stored vector and its metadata by ID."""
        ...
