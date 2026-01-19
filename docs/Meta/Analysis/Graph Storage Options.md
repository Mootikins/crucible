# Crucible Graph Storage Options

**Date**: 2025-12-30
**Status**: Implementation in progress - Cypher parser and SQLite renderer complete

## Context

Evaluating alternatives to SurrealDB for graph storage with these requirements:
1. **LLM-friendly syntax** - models can read/write queries correctly
2. **Embeddable** - no external server required
3. **OLTP patterns** - frequent small reads/writes
4. **Graph traversal** - native or efficient emulation
5. **Vector search** - for semantic retrieval
6. **Minimal binary** - small footprint preferred

## Options Evaluated

### Option A: LadybugDB (Kuzu fork)

**Approach**: Native Cypher database

```cypher
MATCH (n:Note {path: $p})-[:LINKS_TO*1..3]-(related)
RETURN related.path
```

| Aspect | Assessment |
|--------|------------|
| Binary size | ~50MB (C++ library) |
| Stability | 2-month-old fork, inherited bugs |
| Cypher support | Full openCypher |
| Vector search | Built-in |
| Rust bindings | `lbug` crate (FFI to C++) |
| LLM training data | Excellent (Neo4j ecosystem) |

**Risks**:
- [Crash on large edge ingestion](https://github.com/kuzudb/kuzu/issues/4953)
- [Query hangs with recursive patterns](https://github.com/kuzudb/kuzu/issues/5040)
- Project abandonment (VC interest mitigates)

**Files created**:
- `ladybug-schema-prototype.cypher` - Full schema design
- `ladybug-rust-api-sketch.rs` - API usage patterns

---

### Option B: Cypher→SQLite Transpiler

**Approach**: Parse Cypher, generate SQLite SQL with recursive CTEs

```
Cypher Query → chumsky parser → GraphIR → SQLite Renderer → SQL + sqlite-vec
```

| Aspect | Assessment |
|--------|------------|
| Binary size | ~2MB (SQLite + extensions) |
| Stability | SQLite is bulletproof |
| Cypher support | Subset (sufficient for Crucible) |
| Vector search | sqlite-vec extension |
| Rust bindings | Native rusqlite |
| LLM training data | Cypher syntax (same as Option A) |

**Advantages**:
- No upstream dependency risk
- Existing infrastructure: `crucible-query` has chumsky + GraphIR
- SQLite's proven reliability
- Smaller binary

**Effort estimate**: ~2-3 weeks

**Files created**:
- `cypher-syntax-sketch.rs` - Parser implementation
- `sqlite-renderer-sketch.rs` - SQL code generation

---

## Detailed Comparison

### Development Effort

| Component | LadybugDB | Cypher→SQLite |
|-----------|-----------|---------------|
| Schema design | Done | Done |
| Storage integration | 1 week | 1 week |
| Query syntax | 0 (native) | 2-3 weeks (transpiler) |
| Vector search | Built-in | sqlite-vec integration (2-3 days) |
| Testing | 1 week | 1 week |
| **Total** | **2 weeks** | **4-5 weeks** |

### Runtime Characteristics

| Query Type | LadybugDB | SQLite |
|------------|-----------|--------|
| Simple lookup | O(log n) | O(log n) |
| 1-hop traversal | Native graph | JOIN |
| N-hop traversal | Native `*1..N` | Recursive CTE |
| Variable-length path | Native | CTE with cycle detection |
| Vector similarity | HNSW index | sqlite-vec |

### Binary Size

```
LadybugDB:  libkuzu.so (~50MB) + Rust wrapper
SQLite:     libsqlite3.so (~1MB) + sqlite-vec (~500KB) + rusqlite
```

### LLM Query Generation

Both use Cypher syntax for the external interface:

```cypher
-- LLM generates this (same for both options)
MATCH (n:Note {path: 'Index.md'})-[:LINKS_TO]->(target)
RETURN target.path, target.title
```

Transpiler output for SQLite:
```sql
SELECT target.path, target.title
FROM notes n
JOIN edges e ON e.source = n.path AND e.type = 'LINKS_TO'
JOIN notes target ON target.path = e.target
WHERE n.path = 'Index.md'
```

---

## Cypher Subset for Crucible

The transpiler only needs to support:

### Patterns
- `(alias:Label {prop: value})` - node with properties
- `-[:TYPE]->`, `<-[:TYPE]-`, `-[:TYPE]-` - edges
- `*1..3`, `*`, `+` - path quantifiers

### Clauses
- `MATCH pattern` - graph pattern
- `WHERE condition [AND condition]*` - filters
- `RETURN projection [, projection]*` - output
- `CREATE pattern` - insert
- `DELETE alias` / `DETACH DELETE alias` - remove

### Not Needed
- `MERGE` (upsert)
- `OPTIONAL MATCH`
- `CASE` expressions
- `UNION`
- Aggregations beyond `COUNT`
- `WITH` clause
- `ORDER BY`, `SKIP`, `LIMIT` (can add later)

This subset covers ~95% of Crucible's query patterns.

---

## Existing Infrastructure

The `crucible-query` crate already has:

```
src/
├── syntax/
│   ├── pgq.rs      # MATCH pattern parser (90% of Cypher!)
│   ├── jaq.rs      # jaq-style functions
│   └── sql_sugar.rs
├── ir.rs           # GraphIR (node/edge patterns, quantifiers)
├── render/
│   └── surreal.rs  # SurrealQL renderer
└── pipeline.rs     # Syntax registry + transforms
```

**What's already done**:
- Node pattern parsing: `(alias:Label {props})`
- Edge pattern parsing: `-[:TYPE]->`, `<-[:TYPE]-`
- GraphIR with `Quantifier::Range { min, max }`
- chumsky 0.12 parser combinators

**What's needed**:
- Add WHERE/RETURN clause parsing to PGQ syntax (or new CypherSyntax)
- Implement path quantifier parsing (`*1..3`)
- Add CREATE/DELETE support
- SQLite renderer (recursive CTEs)
- sqlite-vec integration

---

## Recommendation

### For Personal/Homelab Use

**Option B (Cypher→SQLite)** is recommended:

1. **Zero upstream risk** - SQLite won't be abandoned
2. **Smaller footprint** - 2MB vs 50MB
3. **Existing infra** - 90% of parser already exists
4. **Better debugging** - SQLite tools are mature
5. **Progressive disclosure** - Schema can be cached, surfaced as needed

### For Production/Multi-User (Future)

Wait for LadybugDB to mature (6-12 months), then re-evaluate. The transpiler approach makes migration easy since both use Cypher syntax externally.

---

## Implementation Plan

### Phase 1: Cypher Parser (1 week)
1. Extend `PgqSyntax` or create `CypherSyntax`
2. Add WHERE clause parsing
3. Add RETURN clause parsing
4. Add path quantifiers (`*1..3`)
5. Add CREATE/DELETE

### Phase 2: SQLite Renderer (1 week)
1. Simple query rendering (JOINs)
2. Recursive CTE generation
3. Cycle detection
4. Parameter binding

### Phase 3: Storage Layer (1 week)
1. SQLite schema setup
2. Note CRUD operations
3. Edge management
4. Block storage

### Phase 4: Vector Search (3 days)
1. sqlite-vec extension loading
2. Embedding storage
3. Similarity queries

### Phase 5: Integration (1 week)
1. Replace SurrealDB usage
2. Migration tooling
3. Testing

---

## Files in This Analysis

```
docs/Meta/Analysis/
├── graph-storage-options.md      # This document
├── ladybug-schema-prototype.cypher
├── ladybug-rust-api-sketch.rs
├── cypher-syntax-sketch.rs       # Parser implementation
└── sqlite-renderer-sketch.rs     # SQL code generation
```
