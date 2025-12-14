---
type: analysis
system: storage
status: review
updated: 2024-12-13
---

# Storage Types & Data Flow Analysis

## Executive Summary

The crucible-surrealdb crate implements a dual-schema storage layer:
1. **EAV+Graph Schema** (Primary) - Entity-Attribute-Value with graph relations
2. **Legacy Embedding Schema** - Being phased out

**CRITICAL Issues Found:**
- N+1 query patterns in tag hierarchy and wikilink resolution
- Missing indexes for common query patterns
- Inefficient batch operations using FOR loops

---

## Critical Issues

- [ ] **N+1 Query: Tag Hierarchy Traversal** (HIGH)
  - Location: `eav_graph/store.rs:1327-1362` (`collect_descendant_tag_names`)
  - Issue: BFS with individual SELECT per tag level
  - Impact: O(depth × breadth) queries
  - **FIX**: Use recursive CTE or single query with path matching

- [ ] **N+1 Query: Tag Entity Lookup** (HIGH)
  - Location: `eav_graph/store.rs:1709-1722`
  - Issue: Loops over tags with one query per tag
  - Impact: O(num_tags) queries
  - **FIX**: Use IN clause with all tag IDs

- [ ] **Missing Index: tags.parent_id** (HIGH)
  - Location: `schema_eav_graph.surql:286-307`
  - Impact: Full table scan for each `get_child_tags` call
  - **FIX**: `DEFINE INDEX tag_parent_idx ON TABLE tags COLUMNS parent_id;`

- [ ] **Missing Index: entity_tags.tag_id** (HIGH)
  - Location: `schema_eav_graph.surql:312-336`
  - Impact: Full table scan for reverse tag lookups
  - **FIX**: `DEFINE INDEX entity_tag_tag_idx ON TABLE entity_tags COLUMNS tag_id;`

---

## High Priority Issues

- [ ] **Inefficient Batch Operations** (MEDIUM-HIGH)
  - Location: `store.rs:728`, `store.rs:1068`
  - Issue: FOR loop in SurrealQL instead of bulk INSERT
  - **FIX**: Use SurrealDB's bulk insert syntax

- [ ] **Wikilink Resolution N+1** (MEDIUM)
  - Location: `eav_graph/ingest/mod.rs:291-298`
  - Issue: Fetches ALL entities, filters in Rust
  - **FIX**: Use WHERE clause with LIKE filter

- [ ] **Non-Atomic Block Replacement** (MEDIUM)
  - Location: `store.rs:250-315`
  - Issue: DELETE then CREATE without transaction
  - **FIX**: Wrap in transaction or use UPSERT

---

## Schema Tables

| Table | Indexes | Issues |
|-------|---------|--------|
| entities | ✅ id, type, content_hash | Missing (type, created_at) |
| properties | ✅ (entity_id, namespace, key) | Missing source, confidence |
| relations | ✅ relation_type, (in, type) | Missing (in, type, content_category) |
| blocks | ✅ (entity_id, block_index) | Missing parent_block_id |
| tags | ✅ name, path | **MISSING parent_id** |
| entity_tags | ✅ (entity_id, tag_id) | **MISSING tag_id alone** |
| embeddings | ✅ (entity_id, model) | MTREE created at runtime |

---

## Data Flow

### Note Ingestion
```
ParsedNote → NoteIngestor
  ↓
1. Create Entity (type: Note)
2. Extract Properties (frontmatter → namespace: "frontmatter")
3. Build Block Hierarchy (AST → blocks table)
4. Extract Relations (wikilinks → relations)
5. Extract Tags (create/upsert tags + entity_tags)
6. Compute Section Hashes (Merkle tree)
```

**Issues:**
- N+1 in wikilink resolution
- Sequential property upsert (FOR loop)
- Block replacement not atomic

---

## Performance Impact

| Operation | Current | Optimized | Improvement |
|-----------|---------|-----------|-------------|
| Tag search (3-level) | 7 queries | 1 query | 7x |
| Note ingest (10 wikilinks) | 11 queries | 2 queries | 5.5x |
| Batch 100 properties | FOR loop | Bulk INSERT | Significant |

---

## Immediate Actions

1. Add missing indexes:
```sql
DEFINE INDEX tag_parent_idx ON TABLE tags COLUMNS parent_id;
DEFINE INDEX entity_tag_tag_idx ON TABLE entity_tags COLUMNS tag_id;
```

2. Fix tag hierarchy N+1 - Replace BFS with recursive CTE
3. Fix wikilink resolution - Move filtering to SQL WHERE clause
