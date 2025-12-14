# Tag Query Optimization Plan

**Date:** 2025-12-13
**Status:** Ready for implementation
**Approach:** TDD with parallel agent execution

## Summary

Optimize tag-related queries in `crucible-surrealdb` by adding missing indexes and replacing N+1 query patterns with single-query alternatives. The existing schema supports hybrid trie/path queries - we're enabling both patterns efficiently.

## Task Dependency Graph

```
Track A (Schema):              Track B (Query Rewrites):

[A1] Add parent_id index       [B1] Path prefix query fix
         │                              │
         ▼                              │ (uses A1 for trie fallback)
[A2] Add tag_id index                   │
         │                              ▼
         │                     [B2] IN clause entity lookup
         │                              │
         └──────────┬───────────────────┘
                    ▼
            [Integration Test]
```

**Parallelism:**
- A1 and A2 are independent → run in parallel
- B1 depends on A1 (parent_id index improves trie traversal)
- B2 depends on A2 (tag_id index improves reverse lookups)
- B1 and B2 can run in parallel after their respective A task

**Agent Assignment:**
- Agent 1: A1 → B1 (tag hierarchy track)
- Agent 2: A2 → B2 (entity lookup track)

---

## Task A1: Add `tags.parent_id` Index

**File:** `crates/crucible-surrealdb/src/schema_eav_graph.surql`

**Location:** After line 306 (after `tag_path_idx`)

**Change:**
```sql
DEFINE INDEX tag_parent_idx ON TABLE tags COLUMNS parent_id;
```

### TDD Test

**File:** `crates/crucible-surrealdb/src/eav_graph/store.rs` (add to tests module)

```rust
#[tokio::test]
async fn tag_parent_id_index_exists() {
    let client = SurrealClient::new_isolated_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();

    let result = client
        .query("INFO FOR TABLE tags", &[])
        .await
        .unwrap();

    let info_str = format!("{:?}", result);
    assert!(
        info_str.contains("tag_parent_idx"),
        "Expected tag_parent_idx index to exist. Got: {}",
        info_str
    );
}
```

**Verification:** `cargo test -p crucible-surrealdb tag_parent_id_index_exists`

---

## Task A2: Add `entity_tags.tag_id` Index

**File:** `crates/crucible-surrealdb/src/schema_eav_graph.surql`

**Location:** After line 335 (after `entity_tag_unique`)

**Change:**
```sql
DEFINE INDEX entity_tag_tag_idx ON TABLE entity_tags COLUMNS tag_id;
```

### TDD Test

**File:** `crates/crucible-surrealdb/src/eav_graph/store.rs` (add to tests module)

```rust
#[tokio::test]
async fn entity_tag_tag_id_index_exists() {
    let client = SurrealClient::new_isolated_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();

    let result = client
        .query("INFO FOR TABLE entity_tags", &[])
        .await
        .unwrap();

    let info_str = format!("{:?}", result);
    assert!(
        info_str.contains("entity_tag_tag_idx"),
        "Expected entity_tag_tag_idx index to exist. Got: {}",
        info_str
    );
}
```

**Verification:** `cargo test -p crucible-surrealdb entity_tag_tag_id_index_exists`

---

## Task B1: Replace N+1 Tag Hierarchy with Path Prefix Query

**File:** `crates/crucible-surrealdb/src/eav_graph/store.rs`

**Location:** Lines 1327-1362 (`collect_descendant_tag_names`)

### Current Code (N+1 BFS)

```rust
async fn collect_descendant_tag_names(&self, tag_id: &str) -> StorageResult<Vec<String>> {
    let mut all_tag_ids = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(tag_id.to_string());
    all_tag_ids.push(tag_id.to_string());

    // BFS with query per level - O(depth * breadth)
    while let Some(current_tag_id) = queue.pop_front() {
        let children = self.get_child_tags(&current_tag_id).await?;
        for child in children {
            all_tag_ids.push(child.name.clone());
            queue.push_back(child.name);
        }
    }
    Ok(all_tag_ids)
}
```

### New Code (Single Query)

```rust
async fn collect_descendant_tag_names(&self, tag_id: &str) -> StorageResult<Vec<String>> {
    // Single query using path prefix matching
    // Matches: exact tag OR any path starting with "tag_id/"
    let params = json!({
        "exact_path": tag_id,
        "prefix_pattern": format!("{}/%", tag_id)
    });

    let result = self
        .client
        .query(
            r#"
            SELECT name FROM tags
            WHERE path = $exact_path OR path LIKE $prefix_pattern
            "#,
            &[params],
        )
        .await
        .map_err(|e| StorageError::Backend(e.to_string()))?;

    let names: Vec<String> = result
        .records
        .iter()
        .filter_map(|r| r.data.get("name").and_then(|v| v.as_str()).map(String::from))
        .collect();

    Ok(names)
}
```

### TDD Tests

**File:** `crates/crucible-surrealdb/src/eav_graph/store.rs` (add to tests module)

```rust
#[tokio::test]
async fn collect_descendant_tag_names_single_tag_no_children() {
    let client = SurrealClient::new_isolated_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    // Create single tag with no children
    store.store_tag(create_tag("orphan", "orphan", None)).await.unwrap();

    let result = store.collect_descendant_tag_names("orphan").await.unwrap();
    assert_eq!(result, vec!["orphan"]);
}

#[tokio::test]
async fn collect_descendant_tag_names_with_hierarchy() {
    let client = SurrealClient::new_isolated_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    // Create hierarchy: project -> project/ai -> project/ai/nlp
    store.store_tag(create_tag("project", "project", None)).await.unwrap();
    store.store_tag(create_tag("project/ai", "project/ai", Some("project"))).await.unwrap();
    store.store_tag(create_tag("project/ai/nlp", "project/ai/nlp", Some("project/ai"))).await.unwrap();

    let result = store.collect_descendant_tag_names("project").await.unwrap();

    assert!(result.contains(&"project".to_string()));
    assert!(result.contains(&"project/ai".to_string()));
    assert!(result.contains(&"project/ai/nlp".to_string()));
    assert_eq!(result.len(), 3);
}

#[tokio::test]
async fn collect_descendant_tag_names_excludes_siblings() {
    let client = SurrealClient::new_isolated_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    // Create: project/ai and project/web (siblings)
    store.store_tag(create_tag("project", "project", None)).await.unwrap();
    store.store_tag(create_tag("project/ai", "project/ai", Some("project"))).await.unwrap();
    store.store_tag(create_tag("project/web", "project/web", Some("project"))).await.unwrap();

    // Query only project/ai subtree
    let result = store.collect_descendant_tag_names("project/ai").await.unwrap();

    assert!(result.contains(&"project/ai".to_string()));
    assert!(!result.contains(&"project/web".to_string())); // sibling excluded
    assert!(!result.contains(&"project".to_string()));     // parent excluded
}

#[tokio::test]
async fn collect_descendant_tag_names_nonexistent_returns_empty() {
    let client = SurrealClient::new_isolated_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    let result = store.collect_descendant_tag_names("nonexistent").await.unwrap();
    assert!(result.is_empty());
}
```

**Verification:** `cargo test -p crucible-surrealdb collect_descendant_tag_names`

---

## Task B2: Replace N+1 Entity Lookup with IN Clause

**File:** `crates/crucible-surrealdb/src/eav_graph/store.rs`

**Location:** Lines 1709-1734 (inside `get_entities_by_tag_hierarchy`)

### Current Code (N+1 Loop)

```rust
for tag_name_to_query in &all_tag_names {
    let params = json!({"tag_id": tag_name_to_query});
    let result = self
        .client
        .query(
            r#"
            SELECT entity_id FROM entity_tags
            WHERE tag_id = type::thing('tags', $tag_id)
            "#,
            &[params],
        )
        .await
        .map_err(|e| StorageError::Backend(e.to_string()))?;

    for record in &result.records {
        if let Some(entity_id) = record.data.get("entity_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            all_entity_ids.insert(entity_id);
        }
    }
}
```

### New Code (Single Query with IN)

```rust
if all_tag_names.is_empty() {
    return Ok(Vec::new());
}

// Build array of tag record references for IN clause
let tag_refs: Vec<String> = all_tag_names
    .iter()
    .map(|name| format!("tags:`{}`", name))
    .collect();

let query = format!(
    r#"
    SELECT entity_id FROM entity_tags
    WHERE tag_id IN [{}]
    "#,
    tag_refs.join(", ")
);

let result = self
    .client
    .query(&query, &[])
    .await
    .map_err(|e| StorageError::Backend(e.to_string()))?;

let entity_ids: Vec<String> = result
    .records
    .iter()
    .filter_map(|r| {
        r.data.get("entity_id")
            .and_then(|v| v.as_str())
            .map(String::from)
    })
    .collect::<std::collections::HashSet<_>>()  // dedupe
    .into_iter()
    .collect();

Ok(entity_ids)
```

### TDD Tests

**File:** `crates/crucible-surrealdb/src/eav_graph/store.rs` (add to tests module)

```rust
#[tokio::test]
async fn get_entities_by_tag_hierarchy_empty_tags() {
    let client = SurrealClient::new_isolated_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    // No tags exist
    let result = store.get_entities_by_tag_hierarchy("nonexistent").await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn get_entities_by_tag_hierarchy_single_tag_multiple_entities() {
    let client = SurrealClient::new_isolated_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    // Create tag and associate with multiple entities
    store.store_tag(create_tag("project", "project", None)).await.unwrap();

    let e1 = RecordId::new("entities", "note:e1");
    let e2 = RecordId::new("entities", "note:e2");
    let e3 = RecordId::new("entities", "note:e3");

    // Create entities first
    create_test_entity(&store, &e1).await;
    create_test_entity(&store, &e2).await;
    create_test_entity(&store, &e3).await;

    // Associate with tag
    store.associate_tag(&e1.to_string(), "project").await.unwrap();
    store.associate_tag(&e2.to_string(), "project").await.unwrap();
    store.associate_tag(&e3.to_string(), "project").await.unwrap();

    let result = store.get_entities_by_tag_hierarchy("project").await.unwrap();
    assert_eq!(result.len(), 3);
}

#[tokio::test]
async fn get_entities_by_tag_hierarchy_deduplicates_entities() {
    let client = SurrealClient::new_isolated_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    // Create hierarchy
    store.store_tag(create_tag("project", "project", None)).await.unwrap();
    store.store_tag(create_tag("project/ai", "project/ai", Some("project"))).await.unwrap();

    let e1 = RecordId::new("entities", "note:e1");
    create_test_entity(&store, &e1).await;

    // Entity tagged with BOTH parent and child
    store.associate_tag(&e1.to_string(), "project").await.unwrap();
    store.associate_tag(&e1.to_string(), "project/ai").await.unwrap();

    // Query parent - should return e1 only once (not duplicated)
    let result = store.get_entities_by_tag_hierarchy("project").await.unwrap();
    assert_eq!(result.len(), 1);
}

#[tokio::test]
async fn get_entities_by_tag_hierarchy_includes_descendant_entities() {
    let client = SurrealClient::new_isolated_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    // Create hierarchy: project -> project/ai -> project/ai/nlp
    store.store_tag(create_tag("project", "project", None)).await.unwrap();
    store.store_tag(create_tag("project/ai", "project/ai", Some("project"))).await.unwrap();
    store.store_tag(create_tag("project/ai/nlp", "project/ai/nlp", Some("project/ai"))).await.unwrap();

    let e1 = RecordId::new("entities", "note:e1");
    let e2 = RecordId::new("entities", "note:e2");
    let e3 = RecordId::new("entities", "note:e3");

    create_test_entity(&store, &e1).await;
    create_test_entity(&store, &e2).await;
    create_test_entity(&store, &e3).await;

    // Each entity tagged at different level
    store.associate_tag(&e1.to_string(), "project").await.unwrap();
    store.associate_tag(&e2.to_string(), "project/ai").await.unwrap();
    store.associate_tag(&e3.to_string(), "project/ai/nlp").await.unwrap();

    // Query root - should get all 3
    let result = store.get_entities_by_tag_hierarchy("project").await.unwrap();
    assert_eq!(result.len(), 3);

    // Query middle - should get 2 (e2, e3)
    let result = store.get_entities_by_tag_hierarchy("project/ai").await.unwrap();
    assert_eq!(result.len(), 2);

    // Query leaf - should get 1 (e3)
    let result = store.get_entities_by_tag_hierarchy("project/ai/nlp").await.unwrap();
    assert_eq!(result.len(), 1);
}
```

**Verification:** `cargo test -p crucible-surrealdb get_entities_by_tag_hierarchy`

---

## Execution Plan

### Phase 1: Parallel Index Tasks (Agents 1 & 2)

**Agent 1 - Task A1:**
1. Write `tag_parent_id_index_exists` test
2. Run test, verify it fails
3. Add `tag_parent_idx` to schema
4. Run test, verify it passes
5. Run full test suite: `cargo test -p crucible-surrealdb`

**Agent 2 - Task A2:**
1. Write `entity_tag_tag_id_index_exists` test
2. Run test, verify it fails
3. Add `entity_tag_tag_idx` to schema
4. Run test, verify it passes
5. Run full test suite: `cargo test -p crucible-surrealdb`

### Phase 2: Parallel Query Tasks (Agents 1 & 2)

**Agent 1 - Task B1:**
1. Write 4 `collect_descendant_tag_names_*` tests
2. Run tests, verify they pass with current impl (they should - behavior unchanged)
3. Replace BFS implementation with path prefix query
4. Run tests, verify they still pass
5. Remove debug `eprintln!` statements from old code
6. Run full test suite

**Agent 2 - Task B2:**
1. Write 4 `get_entities_by_tag_hierarchy_*` tests
2. Run tests, verify they pass with current impl
3. Replace N+1 loop with IN clause query
4. Run tests, verify they still pass
5. Run full test suite

### Phase 3: Integration Verification

After both tracks complete:
```bash
cargo test -p crucible-surrealdb
cargo test -p crucible-cli  # if any CLI tests use tag hierarchy
```

---

## Query Pattern Reference

After implementation, the tag system supports:

```sql
-- Trie-style: direct children
SELECT * FROM tags WHERE parent_id = tags:project;

-- Trie-style: walk up to root
SELECT * FROM tags WHERE id = tags:`project/ai`;
-- then follow parent_id chain

-- Prefix-style: all descendants (single query)
SELECT * FROM tags WHERE path = 'project' OR path LIKE 'project/%';

-- Entities in subtree (single query)
SELECT DISTINCT entity_id FROM entity_tags
WHERE tag_id IN (
    SELECT id FROM tags WHERE path = 'project' OR path LIKE 'project/%'
);
```

---

## Rollback Plan

If issues arise:
1. Indexes are additive - remove from schema, they'll be gone on next fresh DB
2. Query changes are behavioral - revert to BFS/loop code if needed
3. No data migration required - schema structure unchanged
