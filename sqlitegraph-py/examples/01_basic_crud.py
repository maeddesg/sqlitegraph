"""Basic CRUD operations with sqlitegraph.

Demonstrates: open, add_node, get_node, update_node, delete_node,
add_edge, get_edge, delete_edge, neighbors, node_degree.
"""

from sqlitegraph import Graph

# Open an in-memory graph (fast, no persistence)
g = Graph.open_in_memory()
print("=== Basic CRUD ===\n")

# -- Nodes --

alice = g.add_node(kind="User", name="Alice", data={"age": 30, "city": "Berlin"})
bob = g.add_node(kind="User", name="Bob", data={"age": 25, "city": "Paris"})
project = g.add_node(kind="Project", name="GraphDB", data={"status": "active"})
company = g.add_node(kind="Company", name="TechCorp", data={"founded": 2020})

print(f"Created nodes: Alice={alice}, Bob={bob}, Project={project}, Company={company}")

# Read a node back
node = g.get_node(alice)
print(f"\nAlice's node: {node}")

# Update a node
g.update_node(bob, kind="User", name="Bob", data={"age": 26, "city": "Paris", "promoted": True})
updated = g.get_node(bob)
print(f"Updated Bob: {updated}")

# -- Edges --

# Alice works_on Project
e1 = g.add_edge(alice, project, "works_on", data={"role": "developer"})
# Project belongs_to Company
e2 = g.add_edge(project, company, "belongs_to", data={"ownership": 0.75})
# Alice employed_by Company
e3 = g.add_edge(alice, company, "employed_by", data={"department": "engineering"})

print(f"\nCreated edges: works_on={e1}, belongs_to={e2}, employed_by={e3}")

# Read an edge back
edge = g.get_edge(e1)
print(f"Edge {e1}: {edge['edge_type']} from {edge['from_id']} to {edge['to_id']}")

# -- Neighbors & degrees --

print(f"\nAlice's outgoing neighbors: {g.neighbors(alice, direction='outgoing')}")
print(f"Alice's incoming neighbors: {g.neighbors(alice, direction='incoming')}")
print(f"Alice's all neighbors: {g.neighbors(alice)}")

in_deg, out_deg = g.node_degree(alice)
print(f"\nAlice's degree: in={in_deg}, out={out_deg}")

# -- Query by kind / pattern --

users = g.nodes_by_kind("User")
print(f"\nAll users: {users}")

matches = g.nodes_by_name_pattern("Al*")
print(f"Names matching 'Al*': {matches}")

# -- Delete --

# Delete an edge
g.delete_edge(e3)
print(f"\nDeleted edge {e3}. Alice's neighbors now: {g.neighbors(alice)}")

# Delete a node (also deletes attached edges)
g.delete_node(bob)
print(f"Deleted Bob. Remaining node IDs: {g.node_ids()}")
