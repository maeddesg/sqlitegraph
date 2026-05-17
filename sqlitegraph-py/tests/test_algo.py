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


# ── New algorithms exposed in M5: SCC, label-prop, cycles, dominators, critical-path ──


def test_strongly_connected_components_finds_cycle():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    c = g.add_node(kind="N", name="c")
    isolated = g.add_node(kind="N", name="isolated")
    # 3-cycle a→b→c→a
    g.add_edge(a, b, "r")
    g.add_edge(b, c, "r")
    g.add_edge(c, a, "r")

    sccs = g.strongly_connected_components()
    # Sort each component for stable comparison.
    sccs_sorted = sorted([sorted(comp) for comp in sccs], key=len)
    # The isolated node is its own SCC; the cycle is one SCC of three.
    assert [isolated] in sccs_sorted
    assert sorted([a, b, c]) in sccs_sorted
    assert sum(len(comp) for comp in sccs) == 4


def test_label_propagation_groups_dense_clusters():
    g = _g()
    # Two triangles linked by a single bridge edge — label-prop should still
    # split them in most random initialisations, but at minimum every node
    # ends up in *some* community.
    nodes = [g.add_node(kind="N", name=f"n{i}") for i in range(6)]
    a, b, c, d, e, f = nodes
    for u, v in [(a, b), (b, c), (c, a), (d, e), (e, f), (f, d)]:
        g.add_edge(u, v, "r")

    communities = g.label_propagation(max_iterations=20)
    flat = sorted(n for comm in communities for n in comm)
    assert flat == sorted(nodes)


def test_find_cycles_finds_simple_cycle():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    c = g.add_node(kind="N", name="c")
    g.add_edge(a, b, "r")
    g.add_edge(b, c, "r")
    g.add_edge(c, a, "r")

    cycles = g.find_cycles(limit=10)
    assert len(cycles) >= 1
    # The cycle nodes are a, b, c (the trailing repeat closing the cycle is allowed).
    found = set(cycles[0])
    assert {a, b, c}.issubset(found)


def test_find_cycles_empty_on_dag():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    c = g.add_node(kind="N", name="c")
    g.add_edge(a, b, "r")
    g.add_edge(b, c, "r")
    cycles = g.find_cycles(limit=10)
    assert cycles == []


def test_dominators_idom_tree():
    g = _g()
    # Diamond: 1 → 2,3 → 4
    a = g.add_node(kind="N", name="a")  # entry
    b = g.add_node(kind="N", name="b")
    c = g.add_node(kind="N", name="c")
    d = g.add_node(kind="N", name="d")
    g.add_edge(a, b, "r")
    g.add_edge(a, c, "r")
    g.add_edge(b, d, "r")
    g.add_edge(c, d, "r")

    result = g.dominators(entry=a)
    idom = result["idom"]
    # Entry node's idom is None.
    assert idom[a] is None
    # Both b and c are immediately dominated by the entry.
    assert idom[b] == a
    assert idom[c] == a
    # d is immediately dominated by the entry (no single intermediate dominator).
    assert idom[d] == a


def test_critical_path_finds_longest_dag_path():
    g = _g()
    # Linear chain 1 → 2 → 3 → 4 plus a shortcut 1 → 4.
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    c = g.add_node(kind="N", name="c")
    d = g.add_node(kind="N", name="d")
    g.add_edge(a, b, "r")
    g.add_edge(b, c, "r")
    g.add_edge(c, d, "r")
    g.add_edge(a, d, "r")  # shortcut

    result = g.critical_path()
    # With uniform weights, longest path has length 3 (a→b→c→d), distance 3.0.
    assert result["distance"] == 3.0
    assert result["path"] == [a, b, c, d]


def test_critical_path_errors_on_cycle():
    g = _g()
    a = g.add_node(kind="N", name="a")
    b = g.add_node(kind="N", name="b")
    g.add_edge(a, b, "r")
    g.add_edge(b, a, "r")
    try:
        g.critical_path()
    except Exception:
        return
    raise AssertionError("expected exception on cyclic graph")
