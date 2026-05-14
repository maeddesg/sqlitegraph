"""Tests for graph algorithms added in M3."""

import sqlitegraph


def _g():
    return sqlitegraph.Graph.open_in_memory()


def test_pagerank_sums_to_about_one():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    c = g.add_node(kind="N", name="c")
    g.add_edge(a, b, "r")
    g.add_edge(b, c, "r")
    g.add_edge(c, a, "r")

    scores = g.pagerank(damping=0.85, iterations=20)
    assert len(scores) == 3
    total = sum(score for _, score in scores)
    assert 0.99 <= total <= 1.01
    # Triangle: all scores equal
    score_a = next(s for nid, s in scores if nid == a)
    score_b = next(s for nid, s in scores if nid == b)
    assert abs(score_a - score_b) < 1e-6


def test_pagerank_empty_graph():
    g = _g()
    assert g.pagerank() == []


def test_louvain_assigns_communities():
    g = _g()
    # Two disjoint triangles
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    c = g.add_node(kind="N", name="c")
    d = g.add_node(kind="N", name="d")
    e = g.add_node(kind="N", name="e")
    f = g.add_node(kind="N", name="f")
    for u, v in [(a, b), (b, c), (c, a)]:
        g.add_edge(u, v, "r")
    for u, v in [(d, e), (e, f), (f, d)]:
        g.add_edge(u, v, "r")

    communities = g.louvain_communities(max_iterations=10)
    assert isinstance(communities, list)
    flat = sorted(n for comm in communities for n in comm)
    assert flat == sorted([a, b, c, d, e, f])
    # Each community should be non-empty
    assert all(len(c) >= 1 for c in communities)


def test_connected_components_two_islands():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    c = g.add_node(kind="N", name="c")
    d = g.add_node(kind="N", name="d")
    g.add_edge(a, b, "r")
    g.add_edge(c, d, "r")

    components = g.connected_components()
    assert len(components) == 2
    sizes = sorted(len(c) for c in components)
    assert sizes == [2, 2]


def test_connected_components_single():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    g.add_edge(a, b, "r")
    components = g.connected_components()
    assert len(components) == 1
    assert sorted(components[0]) == sorted([a, b])
