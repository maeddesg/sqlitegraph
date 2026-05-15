"""Social network analysis with sqlitegraph.

A realistic example: model a small social network, find influencers
via PageRank, detect communities via Louvain, and find connection paths.
"""

from sqlitegraph import Graph

g = Graph.open_in_memory()
print("=== Social Network Analysis ===\n")

# Create people
people = {
    "alice": g.add_node(kind="Person", name="Alice", data={"role": "engineer"}),
    "bob": g.add_node(kind="Person", name="Bob", data={"role": "designer"}),
    "carol": g.add_node(kind="Person", name="Carol", data={"role": "manager"}),
    "dave": g.add_node(kind="Person", name="Dave", data={"role": "engineer"}),
    "eve": g.add_node(kind="Person", name="Eve", data={"role": "analyst"}),
    "frank": g.add_node(kind="Person", name="Frank", data={"role": "intern"}),
    "grace": g.add_node(kind="Person", name="Grace", data={"role": "cto"}),
}

# Create follows edges (directed: A follows B)
follows = [
    ("alice", "bob"),  # alice follows bob
    ("alice", "carol"),  # alice follows carol
    ("bob", "carol"),  # bob follows carol
    ("carol", "grace"),  # carol follows grace (the CTO)
    ("dave", "alice"),  # dave follows alice
    ("dave", "bob"),  # dave follows bob
    ("eve", "carol"),  # eve follows carol
    ("frank", "alice"),  # frank follows alice
    ("frank", "dave"),  # frank follows dave
    ("grace", "carol"),  # grace follows carol (mutual)
]

for follower, followee in follows:
    g.add_edge(people[follower], people[followee], "follows")

print("Social network created:")
print(f"  People: {len(people)}")
print(f"  Follows edges: {len(follows)}")

# -- Find influencers with PageRank --

scores = g.pagerank(damping=0.85, iterations=50)
print("\n--- Influencers (PageRank) ---")
# Sort by score descending
ranked = sorted(scores, key=lambda x: x[1], reverse=True)
for nid, score in ranked:
    name = g.get_node(nid)["name"]
    in_deg, out_deg = g.node_degree(nid)
    print(f"  {name:6s}: score={score:.4f}  (in={in_deg}, out={out_deg})")

# -- Find communities --

communities = g.louvain_communities(max_iterations=10)
print("\n--- Communities (Louvain) ---")
print(f"Found {len(communities)} communities:")
for i, comm in enumerate(communities):
    names = [g.get_node(nid)["name"] for nid in comm]
    print(f"  Community {i}: {', '.join(names)}")

# -- Find connection paths --

print("\n--- Connection Paths ---")
path = g.shortest_path(people["frank"], people["grace"])
if path:
    names = [g.get_node(nid)["name"] for nid in path]
    print(f"  Frank -> Grace: {' -> '.join(names)}")
else:
    print("  Frank -> Grace: no path")

path = g.shortest_path(people["eve"], people["dave"])
if path:
    names = [g.get_node(nid)["name"] for nid in path]
    print(f"  Eve -> Dave: {' -> '.join(names)}")
else:
    print("  Eve -> Dave: no path")

# -- Mutual follows --

print("\n--- Mutual Follows ---")
for name_a, nid_a in people.items():
    for name_b, nid_b in people.items():
        if name_a >= name_b:
            continue
        a_follows_b = nid_b in g.neighbors(nid_a, direction="outgoing")
        b_follows_a = nid_a in g.neighbors(nid_b, direction="outgoing")
        if a_follows_b and b_follows_a:
            print(f"  {name_a.title()} <-> {name_b.title()}")

# -- Who follows the CTO? --

grace_id = people["grace"]
followers_of_cto = g.neighbors(grace_id, direction="incoming")
print("\n--- Followers of Grace (CTO) ---")
for nid in followers_of_cto:
    print(f"  {g.get_node(nid)['name']}")
