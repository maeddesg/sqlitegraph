"""Graph algorithms with sqlitegraph.

Demonstrates: bfs, k_hop, shortest_path, pagerank,
louvain_communities, connected_components.
"""

from sqlitegraph import Graph

g = Graph.open_in_memory()
print("=== Graph Algorithms ===\n")

# Build a sample graph: two disjoint triangles plus a bridge
# Triangle 1: a-b-c-a
# Triangle 2: d-e-f-d
# Bridge: c-d

a = g.add_node(kind="Node", name="a")
b = g.add_node(kind="Node", name="b")
c = g.add_node(kind="Node", name="c")
d = g.add_node(kind="Node", name="d")
e = g.add_node(kind="Node", name="e")
f = g.add_node(kind="Node", name="f")

for u, v in [(a, b), (b, c), (c, a)]:
    g.add_edge(u, v, "link")
for u, v in [(d, e), (e, f), (f, d)]:
    g.add_edge(u, v, "link")
g.add_edge(c, d, "bridge")

print("Graph: two triangles linked by a bridge (c-d)")

# -- BFS --

reached = g.bfs(a, depth=2)
print(f"\nBFS from a (depth=2): {sorted(reached)}")

# -- k-hop --

khop = g.k_hop(a, depth=2)
print(f"k-hop from a (depth=2): {sorted(khop)}")

# -- Shortest path --

path = g.shortest_path(a, f)
print(f"\nShortest path a -> f: {path}")

path = g.shortest_path(a, e)
print(f"Shortest path a -> e: {path}")

# No connection possible if we delete the bridge
g.delete_edge(g.get_edge(7)["id"] if False else 7)  # edge IDs are 1-based
# Actually let's just show unreachable
isolated = g.add_node(kind="Node", name="isolated")
no_path = g.shortest_path(a, isolated)
print(f"Shortest path a -> isolated (no edge): {no_path}")

# -- PageRank --

# Build a directed cycle for PageRank: a->b->c->a
h = Graph.open_in_memory()
na = h.add_node(kind="Page", name="A")
nb = h.add_node(kind="Page", name="B")
nc = h.add_node(kind="Page", name="C")
h.add_edge(na, nb, "links")
h.add_edge(nb, nc, "links")
h.add_edge(nc, na, "links")

scores = h.pagerank(damping=0.85, iterations=20)
print("\n--- PageRank (cycle of 3) ---")
for nid, score in scores:
    node = h.get_node(nid)
    print(f"  {node['name']}: {score:.6f}")

# -- Louvain Communities --

communities = g.louvain_communities(max_iterations=10)
print("\n--- Louvain Communities ---")
print(f"Found {len(communities)} communities:")
for i, comm in enumerate(communities):
    names = [g.get_node(nid)["name"] for nid in comm]
    print(f"  Community {i}: {names}")

# -- Connected Components --

# Add an isolated pair to the first graph
g2 = Graph.open_in_memory()
x = g2.add_node(kind="Node", name="x")
y = g2.add_node(kind="Node", name="y")
z = g2.add_node(kind="Node", name="z")
g2.add_edge(x, y, "link")  # component 1
# z is alone — but singletons aren't returned by connected_components
# unless they have self-loops, so this gives 1 component

components = g2.connected_components()
print("\n--- Connected Components ---")
print(f"Found {len(components)} component(s)")
for i, comp in enumerate(components):
    names = [g2.get_node(nid)["name"] for nid in comp]
    print(f"  Component {i}: {names}")
