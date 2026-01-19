---
description: Evaluating jaq vs DuckDB for query and data transform needs
status: open
created: 2024-12-27
tags:
  - research
  - query
  - architecture
---

# Query Language Alternatives

## Current State

We have two query/transform needs:

1. **Data Transforms** - Transform JSON/YAML/TOML in scripts and event handlers
2. **Graph Queries** - Traverse the knowledge graph (outlinks, inlinks, neighbors)

Current implementation:
- **oq crate** - jaq (jq-like) for data transforms
- **graph_query module** - Translates jaq-like syntax → SurrealQL

## The Question

Is jaq the right choice when DuckDB offers:
- SQL syntax (more widely known)
- Portable single-file database
- Native JSON/Parquet support
- Graph queries via extensions
- Analytical workloads

## jaq Strengths

| Aspect | jaq |
|--------|-----|
| **Familiarity** | jq is ubiquitous in CLI/DevOps |
| **Streaming** | Process data without loading all into memory |
| **Composability** | Pipes naturally with shell |
| **Size** | Minimal dependency |
| **Data transforms** | Purpose-built for JSON manipulation |

## DuckDB Strengths

| Aspect | DuckDB |
|--------|--------|
| **SQL** | Most developers know SQL |
| **Portability** | Single-file, embeddable |
| **Performance** | Columnar, vectorized, analytical |
| **Types** | Strong typing, schema enforcement |
| **Extensions** | Graph, spatial, FTS, etc. |
| **Multi-format** | JSON, Parquet, CSV, Arrow |

## Architecture Considerations

### Current: jaq + SurrealQL

```
Scripts → oq (jaq) → JSON transforms
Graph queries → jaq-like → SurrealQL → SurrealDB
```

Pros:
- SurrealDB already handles storage + graph
- jaq is lightweight for transforms

Cons:
- Two query languages (jaq + SurrealQL)
- Translation layer complexity

### Alternative: DuckDB for everything

```
Scripts → DuckDB SQL → transforms
Graph queries → DuckDB SQL → DuckDB (with graph extension)
Storage → DuckDB files
```

Pros:
- Single query language
- SQL familiarity
- Portable file format
- Could replace SurrealDB entirely

Cons:
- Graph support via extension (not native)
- Different mental model from jq for transforms
- Would require significant refactoring

### Hybrid: DuckDB alongside SurrealDB

```
Scripts → DuckDB SQL → transforms (replaces oq)
Graph queries → SurrealQL → SurrealDB
Analytics → DuckDB → warehouse queries
```

Pros:
- Best tool for each job
- DuckDB for analytics, SurrealDB for graph
- Gradual migration path

Cons:
- Two databases to maintain
- Data sync complexity

## Questions to Answer

1. **Transform workloads** - How complex are our data transforms? Is jq sufficient or do we need SQL power?

2. **Graph vs Analytics** - Are we doing more graph traversal or analytical queries?

3. **Storage choice** - Is SurrealDB the right long-term choice? DuckDB could simplify the stack.

4. **Developer experience** - What do users expect? jq-style or SQL?

## Next Steps

- [ ] Prototype DuckDB for analytics queries (note stats, tag distributions)
- [ ] Evaluate DuckDB graph extension for outlinks/inlinks
- [ ] Benchmark jaq vs DuckDB for transform workloads
- [ ] Survey: Do users prefer jq or SQL for note queries?

## See Also

- [[../Analysis/Storage Types Flow]] - Current storage architecture
- [[Help/Query/Index]] - Planned query system
- `crates/crucible-surrealdb/src/graph_query.rs` - jaq → SurrealQL translator
