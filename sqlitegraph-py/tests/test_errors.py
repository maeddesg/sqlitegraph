"""Tests for the public exception hierarchy."""

import pytest
import sqlitegraph
from sqlitegraph import (
    BackendError,
    GraphError,
    InvalidArgumentError,
    NotFoundError,
)


def _g():
    return sqlitegraph.Graph.open_in_memory()


def test_exception_classes_exposed():
    assert issubclass(NotFoundError, GraphError)
    assert issubclass(InvalidArgumentError, GraphError)
    assert issubclass(BackendError, GraphError)
    assert issubclass(GraphError, Exception)


def test_get_node_missing_raises_not_found():
    g = _g()
    with pytest.raises(NotFoundError):
        g.get_node(999_999)


def test_get_edge_missing_raises_not_found():
    g = _g()
    with pytest.raises(NotFoundError):
        g.get_edge(999_999)


def test_delete_edge_missing_raises_not_found():
    g = _g()
    with pytest.raises(NotFoundError):
        g.delete_edge(999_999)


def test_update_node_missing_raises_not_found():
    g = _g()
    with pytest.raises(NotFoundError):
        g.update_node(999_999, kind="X", name="y")


def test_get_hnsw_index_missing_raises_not_found():
    g = _g()
    with pytest.raises(NotFoundError):
        g.get_hnsw_index("does-not-exist")


def test_create_duplicate_hnsw_index_raises_invalid():
    g = _g()
    g.create_hnsw_index("emb", dimension=4)
    with pytest.raises(InvalidArgumentError):
        g.create_hnsw_index("emb", dimension=4)


def test_not_found_is_graph_error():
    g = _g()
    with pytest.raises(GraphError):
        g.get_node(999_999)


def test_exception_message_contains_id():
    g = _g()
    try:
        g.get_node(999_999)
    except NotFoundError as e:
        assert "999999" in str(e) or "999_999" in str(e) or "entity 999999" in str(e)
    else:
        raise AssertionError("expected NotFoundError")
