"""CRUD/query tests for additions in M3."""

import pytest
import sqlitegraph
from sqlitegraph import NotFoundError


def _g():
    return sqlitegraph.Graph.open_in_memory()


def test_update_node_preserves_id():
    g = _g()
    nid = g.add_node(kind="User", name="Alice", data={"age": 30})
    same_id = g.update_node(nid, kind="User", name="Alice", data={"age": 31})
    assert same_id == nid
    node = g.get_node(nid)
    assert node["data"]["age"] == 31


def test_update_node_can_change_kind_and_name():
    g = _g()
    nid = g.add_node(kind="Draft", name="x")
    g.update_node(nid, kind="Published", name="y")
    node = g.get_node(nid)
    assert node["kind"] == "Published"
    assert node["name"] == "y"


def test_get_edge_returns_dict():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    eid = g.add_edge(a, b, "rel", data={"weight": 0.5})
    edge = g.get_edge(eid)
    assert edge["id"] == eid
    assert edge["from_id"] == a
    assert edge["to_id"] == b
    assert edge["edge_type"] == "rel"
    assert edge["data"]["weight"] == 0.5


def test_get_edge_missing_raises():
    g = _g()
    with pytest.raises(NotFoundError):
        g.get_edge(999_999)


def test_delete_edge_removes_neighbor():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    eid = g.add_edge(a, b, "rel")
    assert b in g.neighbors(a)
    g.delete_edge(eid)
    assert b not in g.neighbors(a)


def test_delete_edge_missing_raises():
    g = _g()
    with pytest.raises(NotFoundError):
        g.delete_edge(999_999)


def test_shortest_path_direct():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    g.add_edge(a, b, "next")
    path = g.shortest_path(a, b)
    assert path == [a, b]


def test_shortest_path_multi_hop():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    c = g.add_node(kind="N", name="c")
    g.add_edge(a, b, "next")
    g.add_edge(b, c, "next")
    path = g.shortest_path(a, c)
    assert path == [a, b, c]


def test_shortest_path_unreachable():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    # No edge between them
    path = g.shortest_path(a, b)
    assert path is None


def test_nodes_by_name_pattern():
    g = _g()
    g.add_node(kind="User", name="Alice")
    g.add_node(kind="User", name="Albert")
    g.add_node(kind="User", name="Bob")
    matches = g.nodes_by_name_pattern("Al*")
    assert len(matches) == 2
