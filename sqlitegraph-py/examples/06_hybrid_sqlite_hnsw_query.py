"""Hybrid sqlitegraph demo for Python.

This mirrors the Python-exposed part of the Rust hybrid demo:

- sqlite3 owns normal application rows.
- sqlitegraph stores graph metadata in the same SQLite file.
- HNSW stores vector similarity in the sqlitegraph layer.
- Graph.query() expands vector candidates through graph relationships.

Native V3 and pub/sub are available from Rust today, but are not exposed by the
current Python FFI surface.
"""

from __future__ import annotations

import sqlite3
import tempfile
from pathlib import Path

from sqlitegraph import Graph


def main() -> None:
    workdir = Path(tempfile.mkdtemp(prefix="sqlitegraph-python-hybrid-"))
    db_path = workdir / "application.sqlite"

    sql = sqlite3.connect(db_path)
    sql.execute(
        """
        CREATE TABLE documents (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            body TEXT NOT NULL
        )
        """
    )
    sql.executemany(
        "INSERT INTO documents (id, title, body) VALUES (?, ?, ?)",
        [
            (
                1,
                "SQLiteGraph architecture",
                "SQLite stores durable records, graph metadata links them, HNSW stores vectors.",
            ),
            (
                2,
                "Grounded coding workflow",
                "Code intelligence links symbols, files, notes, and provenance into a graph.",
            ),
            (
                3,
                "Bread baking notes",
                "Hydration, fermentation, flour, and oven spring belong to a different topic.",
            ),
        ],
    )
    sql.commit()

    graph = Graph.open(str(db_path))
    doc_1 = graph.add_node(kind="Document", name="doc:1", data={"sqlite_id": 1})
    doc_2 = graph.add_node(kind="Document", name="doc:2", data={"sqlite_id": 2})
    doc_3 = graph.add_node(kind="Document", name="doc:3", data={"sqlite_id": 3})
    graph.add_edge(doc_1, doc_2, "MENTIONS")
    graph.add_edge(doc_2, doc_1, "USES")
    graph.add_edge(doc_3, doc_1, "UNRELATED_EXAMPLE")

    index = graph.create_hnsw_index("document_embeddings", dimension=3, metric="cosine")
    vector_to_sqlite_id = {
        index.insert_vector([0.95, 0.80, 0.05], {"sqlite_id": 1}): 1,
        index.insert_vector([0.90, 0.85, 0.10], {"sqlite_id": 2}): 2,
        index.insert_vector([0.05, 0.10, 0.95], {"sqlite_id": 3}): 3,
    }

    nearest = index.search([0.92, 0.82, 0.05], 2)

    print("Hybrid sqlitegraph Python demo")
    print(f"workspace: {workdir}")
    print("note: Native V3 and pub/sub are Rust-only in the current Python FFI.")
    print()

    for vector_id, distance in nearest:
        sqlite_id = vector_to_sqlite_id[vector_id]
        title = sql.execute(
            "SELECT title FROM documents WHERE id = ?",
            (sqlite_id,),
        ).fetchone()[0]
        query = (
            "MATCH (a)-[:MENTIONS]->(b) "
            f'WHERE a.kind = "Document" AND a.name = "doc:{sqlite_id}" '
            "RETURN b.name"
        )
        graph_result = graph.query(query)
        print(
            f"- vector_id={vector_id}, sqlite_id={sqlite_id}, "
            f"distance={distance:.4f}, title={title!r}, graph_query={graph_result['results']}"
        )


if __name__ == "__main__":
    main()
