"""Smoke tests for the public Python surface of `sqlitegraph`."""

import sqlitegraph


def test_module_exposes_graph():
    assert hasattr(sqlitegraph, "Graph")


def test_in_memory_open():
    g = sqlitegraph.Graph.open_in_memory()
    assert g is not None


def test_add_node_returns_id():
    g = sqlitegraph.Graph.open_in_memory()
    nid = g.add_node(kind="User", name="Alice", data={"age": 30})
    assert isinstance(nid, int)
    assert nid > 0


def test_add_node_without_data():
    g = sqlitegraph.Graph.open_in_memory()
    nid = g.add_node(kind="Order", name="Order-123")
    assert isinstance(nid, int)


def test_get_node_roundtrip():
    g = sqlitegraph.Graph.open_in_memory()
    nid = g.add_node(kind="User", name="Alice", data={"age": 30, "city": "Berlin"})
    node = g.get_node(nid)
    assert node["id"] == nid
    assert node["kind"] == "User"
    assert node["name"] == "Alice"
    assert node["data"]["age"] == 30
    assert node["data"]["city"] == "Berlin"


def test_add_edge_and_neighbors():
    g = sqlitegraph.Graph.open_in_memory()
    user = g.add_node(kind="User", name="Alice")
    order = g.add_node(kind="Order", name="Order-123")
    eid = g.add_edge(user, order, "placed")
    assert isinstance(eid, int)

    out = g.neighbors(user)
    assert order in out


def test_neighbors_direction():
    g = sqlitegraph.Graph.open_in_memory()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    g.add_edge(a, b, "rel")

    assert b in g.neighbors(a, direction="outgoing")
    assert a in g.neighbors(b, direction="incoming")
    assert b not in g.neighbors(b, direction="outgoing")


def test_nodes_by_kind():
    g = sqlitegraph.Graph.open_in_memory()
    g.add_node(kind="User", name="Alice")
    g.add_node(kind="User", name="Bob")
    g.add_node(kind="Order", name="Order-1")

    users = g.nodes_by_kind("User")
    assert len(users) == 2


def test_bfs():
    g = sqlitegraph.Graph.open_in_memory()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    c = g.add_node(kind="N", name="c")
    g.add_edge(a, b, "next")
    g.add_edge(b, c, "next")

    reached = g.bfs(a, depth=2)
    assert b in reached
    assert c in reached


def test_node_degree_returns_in_out_order():
    g = sqlitegraph.Graph.open_in_memory()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    c = g.add_node(kind="N", name="c")
    # a -> b, a -> c, c -> a
    g.add_edge(a, b, "next")
    g.add_edge(a, c, "next")
    g.add_edge(c, a, "back")

    in_deg, out_deg = g.node_degree(a)
    assert in_deg == 1
    assert out_deg == 2

    in_b, out_b = g.node_degree(b)
    assert in_b == 1
    assert out_b == 0
