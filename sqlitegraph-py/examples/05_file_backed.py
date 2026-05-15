"""Persistent file-backed graph with sqlitegraph.

Demonstrates: Graph.open(path), checkpoint, reopen, and data durability.
"""

import os
import tempfile

from sqlitegraph import Graph

# Use a temporary file for the demo
db_path = os.path.join(tempfile.gettempdir(), "sqlitegraph_demo.db")
print("=== File-Backed Graph ===\n")
print(f"Database path: {db_path}\n")

# Clean up any previous demo file
if os.path.exists(db_path):
    os.remove(db_path)

# -- Create and populate --

g = Graph.open(db_path)
print("Created file-backed graph.")

alice = g.add_node(kind="User", name="Alice", data={"age": 30})
bob = g.add_node(kind="User", name="Bob", data={"age": 25})
project = g.add_node(kind="Project", name="GraphDB")

g.add_edge(alice, project, "works_on")
g.add_edge(bob, project, "works_on")

print(f"Added {len(g.node_ids())} nodes and edges.")

# Checkpoint to ensure data is flushed
g.checkpoint()
print("Checkpoint complete.\n")

# -- Re-open and verify --

g2 = Graph.open(db_path)
print("Re-opened graph from disk.")

print(f"Node count: {len(g2.node_ids())}")
print(f"Nodes by kind 'User': {g2.nodes_by_kind('User')}")

# Retrieve Alice
users = g2.nodes_by_kind("User")
for uid in users:
    node = g2.get_node(uid)
    print(f"  {node['name']}: {node['data']}")

# -- Clean up --

if os.path.exists(db_path):
    os.remove(db_path)
    print(f"\nCleaned up {db_path}")
