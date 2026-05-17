# SQLiteGraph Query Language

SQLiteGraph supports a Cypher-inspired query language for pattern matching and traversal. This is the primary interface for both the CLI (`sqlitegraph query`) and the Python API (`Graph.query()`).

## Supported Syntax

```
MATCH <pattern> [WHERE <conditions>] [RETURN <fields>] [LIMIT <n>]
```

### Node Patterns

Find all nodes with a label:

```sql
MATCH (n:Function) RETURN n.name
```

Find nodes with inline property filters:

```sql
MATCH (n:Function {lang: "rust"}) RETURN n
```

Match any node (no label filter):

```sql
MATCH (n) RETURN n
```

### Edge Patterns

Traverse edges by type:

```sql
MATCH (a)-[:CALLS]->(b) RETURN a, b
```

With label filters on endpoints:

```sql
MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a.name, b.name
```

### Star Patterns

Comma-separated edge patterns where every leg starts from the same root
variable form a *star*. The result is the cartesian product of per-leg
matches, joined on the shared root binding:

```sql
MATCH (r)-[:OWNS]->(x), (r)-[:LIKES]->(y) RETURN r.name, x.name, y.name
```

All legs must share the *first* variable (`r` above). Multi-pattern joins
on other variables (e.g. `(a)-[:X]->(b), (b)-[:Y]->(c)` as a 3-node
chain) are not yet supported — express such patterns as a chain
`(a)-[:X]->(b)-[:Y]->(c)`.

### WHERE Clause

Filter results by property values:

```sql
MATCH (n:Function) WHERE n.lang = "rust" RETURN n.name
```

Supported operators: `=`, `!=`, `<`, `<=`, `>`, `>=`, and `=~` (regex match).

```sql
MATCH (n) WHERE n.count > 5 RETURN n
MATCH (n) WHERE n.name =~ "main.*" RETURN n.name
```

Combine predicates with `AND` and `OR`. `OR` binds looser than `AND` (standard
precedence), so `a AND b OR c` is `(a AND b) OR c`:

```sql
MATCH (n) WHERE n.lang = "rust" AND n.name = "main" OR n.name = "util" RETURN n
```

Parentheses are not yet supported; rewrite expressions to fit the fixed
precedence if needed.

On edge patterns, filter by either endpoint:

```sql
MATCH (a)-[:CALLS]->(b) WHERE b.lang = "python" RETURN a.name
```

### RETURN Clause

Return specific fields:

```sql
MATCH (n:Function) RETURN n.name, n.kind
```

Return entire nodes:

```sql
MATCH (n:Function) RETURN n
```

Return everything (default when RETURN is omitted):

```sql
MATCH (n:Function)
```

### LIMIT

Cap the number of results:

```sql
MATCH (n:Function) RETURN n.name LIMIT 10
```

## Field Reference

When accessing node fields in RETURN or WHERE:

| Field | Meaning |
|-------|---------|
| `n.id` | Node ID |
| `n.kind` | Node type (the "label" in Cypher maps to `kind`) |
| `n.name` | Node name |
| `n.<key>` | Property from the node's `data` JSON |

## CLI Usage

```bash
sqlitegraph query "MATCH (n:Function) RETURN n.name" --db graph.db
```

## Python Usage

```python
import sqlitegraph as sg

g = sg.Graph.open("graph.db")
result = g.query('MATCH (n:Function) WHERE n.lang = "rust" RETURN n.name')
for row in result["results"]:
    print(row)
```

## Supported Labels

Labels in node patterns (e.g., `:Function`) map to the `kind` field of nodes. For edge patterns to use label filtering efficiently, labels must be registered via the index API (`add_label` in Rust). The `insert_node` method stores the `kind` but does not auto-register labels.

## Limitations

- No aggregation (COUNT, SUM, AVG, etc.)
- No ORDER BY
- No WITH or UNWIND
- No variable bindings carried across patterns (each MATCH is independent)
- No shortestPath() function
- Parentheses inside WHERE are not supported (precedence is fixed: OR binds looser than AND)
- Edge patterns require the SQLite backend; the V3 backend returns an error
