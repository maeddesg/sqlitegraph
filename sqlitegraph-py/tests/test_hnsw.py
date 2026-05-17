"""Smoke tests for HNSW vector search via the Python surface."""

import pytest
import sqlitegraph
from sqlitegraph import NotFoundError


def test_create_hnsw_index():
    g = sqlitegraph.Graph.open_in_memory()
    idx = g.create_hnsw_index("emb", dimension=4)
    assert idx is not None
    assert idx.name() == "emb"


def test_insert_and_search():
    g = sqlitegraph.Graph.open_in_memory()
    idx = g.create_hnsw_index("emb", dimension=3, metric="cosine")

    id_a = idx.insert_vector([1.0, 0.0, 0.0])
    id_b = idx.insert_vector([0.0, 1.0, 0.0])
    id_c = idx.insert_vector([1.0, 0.1, 0.0])

    results = idx.search([1.0, 0.0, 0.0], k=2)
    assert len(results) == 2
    ids = [r[0] for r in results]
    assert id_a in ids
    assert id_c in ids
    # The perpendicular vector is farthest from the query.
    assert id_b not in ids


def test_vector_count():
    g = sqlitegraph.Graph.open_in_memory()
    idx = g.create_hnsw_index("emb", dimension=2)
    assert idx.vector_count() == 0
    idx.insert_vector([0.5, 0.5])
    assert idx.vector_count() == 1


def test_get_hnsw_index_roundtrip():
    g = sqlitegraph.Graph.open_in_memory()
    g.create_hnsw_index("emb", dimension=2)
    idx2 = g.get_hnsw_index("emb")
    assert idx2.name() == "emb"


def test_get_hnsw_index_missing():
    g = sqlitegraph.Graph.open_in_memory()
    with pytest.raises(NotFoundError):
        g.get_hnsw_index("does-not-exist")


def test_list_hnsw_indexes():
    g = sqlitegraph.Graph.open_in_memory()
    g.create_hnsw_index("a", dimension=2)
    g.create_hnsw_index("b", dimension=2)
    names = g.list_hnsw_indexes()
    assert set(names) >= {"a", "b"}


def test_delete_hnsw_index_removes_it():
    g = sqlitegraph.Graph.open_in_memory()
    g.create_hnsw_index("doomed", dimension=2)
    assert "doomed" in g.list_hnsw_indexes()
    g.delete_hnsw_index("doomed")
    assert "doomed" not in g.list_hnsw_indexes()
    # Subsequent get must fail.
    with pytest.raises(NotFoundError):
        g.get_hnsw_index("doomed")
