"""HNSW vector search with sqlitegraph.

Demonstrates: create_hnsw_index, insert_vector, search,
get_vector, vector_count, list_hnsw_indexes, get_hnsw_index.
"""

from sqlitegraph import Graph

g = Graph.open_in_memory()
print("=== HNSW Vector Search ===\n")

# Create an HNSW index for 3-dimensional vectors using cosine similarity
idx = g.create_hnsw_index("embeddings", dimension=3, metric="cosine")
print(f"Created index: {idx.name()}")

# Insert some vectors
# These are simplified "document embeddings" — in practice you'd use
# an embedding model (e.g. sentence-transformers, OpenAI, etc.)

# "machine learning" direction
v_ml = idx.insert_vector([1.0, 0.8, 0.1])
# "deep learning" direction (similar to ML)
v_dl = idx.insert_vector([0.9, 0.85, 0.05])
# "baking" direction (unrelated)
v_bake = idx.insert_vector([0.1, 0.2, 1.0])
# "cooking" direction (similar to baking)
v_cook = idx.insert_vector([0.05, 0.15, 0.95])

print(f"Inserted vectors: ML={v_ml}, DL={v_dl}, Bake={v_bake}, Cook={v_cook}")
print(f"Total vectors in index: {idx.vector_count()}")

# Search for vectors similar to "machine learning"
query = [1.0, 0.9, 0.0]  # close to ML/DL
top2 = idx.search(query, k=2)
print("\nSearch for [1.0, 0.9, 0.0] (top 2):")
for vid, distance in top2:
    print(f"  vector_id={vid}, distance={distance:.4f}")

# Search for vectors similar to "cooking/baking"
query2 = [0.0, 0.0, 1.0]  # close to Bake/Cook
top2 = idx.search(query2, k=2)
print("\nSearch for [0.0, 0.0, 1.0] (top 2):")
for vid, distance in top2:
    print(f"  vector_id={vid}, distance={distance:.4f}")

# Retrieve a specific vector by ID
vec = idx.get_vector(v_ml)
print(f"\nVector {v_ml}: {vec}")

# List all indexes
print(f"\nAll HNSW indexes: {g.list_hnsw_indexes()}")

# Re-open the index by name
idx2 = g.get_hnsw_index("embeddings")
print(f"Re-opened index: {idx2.name()}, vectors: {idx2.vector_count()}")

# Bulk insert example
print("\n--- Bulk insert ---")
vectors = [
    ([0.5, 0.5, 0.0], None),
    ([0.6, 0.4, 0.0], {"tag": "ml"}),
    ([0.0, 0.0, 0.8], {"tag": "cooking"}),
]
bulk_ids = idx.bulk_insert_vectors(vectors)
print(f"Bulk inserted {len(bulk_ids)} vectors: {bulk_ids}")
print(f"Total vectors now: {idx.vector_count()}")
