# crucible-query Capabilities

This document describes what each parser (input) and renderer (output) in the crucible-query crate supports.

## Architecture

```
Query String → [Parser] → GraphIR → [Renderer] → Database Query
              ↑                     ↑
              Cypher, PGQ, jaq      SQLite, SurrealQL
```

## Parser Capabilities (Input)

Each parser converts a query string into the common GraphIR representation.

| Feature | Cypher | PGQ | SQL Sugar | jaq |
|---------|--------|-----|-----------|-----|
| **Priority** | 55 | 50 | 40 | 30 |
| **Node patterns** `(alias:Label)` | ✅ | ✅ | ❌ | ❌ |
| **Edge patterns** `-[:TYPE]->` | ✅ | ✅ | ❌ | ✅* |
| **Bidirectional** `<-[:TYPE]->` | ✅ | ✅ | ❌ | ✅ |
| **Undirected** `-[:TYPE]-` | ✅ | ✅ | ❌ | ✅ |
| **Node properties** `{key: 'value'}` | ✅ | ✅ | ❌ | ❌ |
| **WHERE clause** | ✅ | ❌ | ❌ | ✅* |
| **RETURN clause** | ✅ | ❌ | ❌ | ❌ |
| **Path quantifiers** `*1..3` | ✅ | ❌ | ❌ | ❌ |
| **Parameters** `$name` | ✅ | ❌ | ❌ | ❌ |
| **Value types** (string, int, bool, null) | ✅ | strings only | strings only | strings only |

\* jaq uses arrow syntax (`->`, `<-`) after function calls, and supports jaq filter expressions.

### Cypher (Priority 55)

Full Cypher syntax for LLM-friendly graph queries:

```cypher
MATCH (n:Note {path: 'index.md'})-[:LINKS_TO*1..3]->(related)
WHERE related.folder = 'Projects'
RETURN related.path, related.title AS name
```

**Supported operators:**
- Equality: `=`, `!=`, `<>`
- String matching: `CONTAINS`, `STARTS WITH`, `ENDS WITH`

### PGQ (Priority 50)

SQL/PGQ MATCH syntax for graph patterns:

```sql
MATCH (a {title: 'Index'})-[:wikilink]->(b)
```

Note: PGQ focuses on pattern matching without WHERE/RETURN clauses.

### SQL Sugar (Priority 40)

SQL-like shortcuts for common operations:

```sql
SELECT outlinks FROM 'Index'
SELECT inlinks FROM 'Project'
SELECT neighbors FROM 'Hub'
```

### jaq (Priority 30)

Function-call syntax inspired by jaq:

```jaq
outlinks("Index") | select(.folder == "Projects")
find("Index") -> inlinks
```

## Renderer Capabilities (Output)

Each renderer converts GraphIR into database-specific query syntax.

| Feature | SQLite | SurrealQL |
|---------|--------|-----------|
| **Simple lookup** `SELECT ... WHERE` | ✅ | ✅ |
| **1-hop traversal** via JOIN | ✅ | ✅ |
| **N-hop traversal** via CTE | ✅ | ❌ |
| **Variable-length paths** `*1..3` | ✅ | ❌ |
| **Bidirectional edges** | ✅ | ✅ |
| **Filter: Eq, Ne** | ✅ | ✅ |
| **Filter: Contains** | ✅ (LIKE) | ✅ (IN) |
| **Filter: StartsWith, EndsWith** | ✅ (LIKE) | ✅ (LIKE) |
| **Projections with aliases** | ✅ | ❌ |
| **Parameter binding** | ✅ (`:name`) | ✅ (`$name`) |

### SQLite Renderer

Generates SQLite SQL with:
- JOINs for fixed-length patterns
- Recursive CTEs for variable-length paths
- Named parameter binding (`:param` style)

**Simple query:**
```sql
SELECT a.*
FROM notes a
JOIN edges e0 ON e0.source = a.path
JOIN notes b ON b.path = e0.target
WHERE a.title = :source_title
  AND e0.type = 'wikilink'
```

**Recursive query (variable-length path):**
```sql
WITH RECURSIVE traverse(path, depth, visited) AS (
    SELECT :source, 0, :source
    UNION ALL
    SELECT e.target, t.depth + 1, t.visited || ',' || e.target
    FROM traverse t
    JOIN edges e ON e.source = t.path AND e.type = 'LINKS_TO'
    WHERE instr(t.visited, e.target) = 0
        AND t.depth < 3
)
SELECT DISTINCT n.*
FROM notes n
JOIN traverse t ON n.path = t.path
WHERE t.depth >= 1
  AND t.path != :source
```

**Schema assumptions:**
```sql
CREATE TABLE notes (
    path TEXT PRIMARY KEY,
    title TEXT,
    content TEXT,
    file_hash TEXT NOT NULL
);

CREATE TABLE edges (
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    type TEXT NOT NULL,
    PRIMARY KEY (source, target, type)
);
```

### SurrealQL Renderer

Generates SurrealQL for SurrealDB:
- Simple lookups and 1-hop traversals
- Uses `FETCH` for following relations
- SurrealDB parameter style (`$param`)

**Example output:**
```sql
SELECT out FROM relations
WHERE `in`.title = $title
AND relation_type = "wikilink"
FETCH out
```

## GraphIR Capabilities

The intermediate representation supports:

### Patterns
- `NodePattern`: alias, label, property constraints
- `EdgePattern`: alias, type, direction, quantifier

### Edge Directions
- `Out`: `-[:TYPE]->`
- `In`: `<-[:TYPE]-`
- `Both`: `<-[:TYPE]->`
- `Undirected`: `-[:TYPE]-`

### Quantifiers
- `ZeroOrMore`: `*`
- `OneOrMore`: `+`
- `Exactly(n)`: `*n`
- `Range { min, max }`: `*min..max`

### Query Sources
- `ByTitle(String)`: Find by title
- `ByPath(String)`: Find by path
- `ById(String)`: Find by ID
- `All`: No specific source

### Match Operations
- `Eq`: Equality
- `Ne`: Not equal
- `Contains`: String/array contains
- `StartsWith`: String prefix
- `EndsWith`: String suffix

## Not Yet Implemented

These features are planned for future phases:

### Phase 2: Mutations
- `CREATE (n:Note {props})` - Insert nodes
- `DELETE n` / `DETACH DELETE n` - Remove nodes
- `MERGE` - Upsert patterns

### Phase 2: Advanced Queries
- `ORDER BY`, `SKIP`, `LIMIT`
- Aggregations (`COUNT`, `SUM`, etc.)
- `WITH` clause for chaining
- `OPTIONAL MATCH`
- `UNION`

## Testing

Each component has comprehensive test coverage:

```bash
# Run all crucible-query tests
cargo test -p crucible-query

# Run specific parser tests
cargo test -p crucible-query syntax::cypher
cargo test -p crucible-query syntax::pgq

# Run renderer tests
cargo test -p crucible-query render::sqlite
cargo test -p crucible-query render::surreal
```
