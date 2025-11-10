# Tag Search Specification

**Capability**: `tag-search`
**Version**: 1.0.0
**Status**: Active
**Created**: 2025-11-09
**Last Updated**: 2025-11-09

## Purpose

Enable intuitive, hierarchical tag-based search that leverages tag taxonomy to discover entities. When users search for a parent tag, they should find all entities tagged with that parent and its descendants, matching the mental model of folder hierarchies and categorical organization.

## Requirements

### Requirement: Hierarchical Tag Search

Tag search SHALL be hierarchical by default. When searching for a tag, the system SHALL return entities associated with:
- The exact tag being searched
- All direct child tags
- All descendant tags (recursively through the hierarchy)

**Rationale**: Tag hierarchies exist to organize knowledge taxonomically. Users expect that searching for a parent tag (e.g., `project`) will surface all entities under that category, including those tagged with more specific variants (`project/ai`, `project/ai/nlp`). This matches user mental models from folder systems, category hierarchies, and taxonomies.

#### Scenario: Parent tag returns children

- **GIVEN** a tag hierarchy exists: `project` → `project/ai` → `project/ai/nlp`
- **AND** entity E1 is tagged with `#project`
- **AND** entity E2 is tagged with `#project/ai`
- **AND** entity E3 is tagged with `#project/ai/nlp`
- **WHEN** user searches for tag `"project"`
- **THEN** the system SHALL return all three entities: E1, E2, and E3

#### Scenario: Child tag does not return parent

- **GIVEN** a tag hierarchy exists: `project` → `project/ai` → `project/ai/nlp`
- **AND** entity E1 is tagged with `#project`
- **AND** entity E2 is tagged with `#project/ai`
- **AND** entity E3 is tagged with `#project/ai/nlp`
- **WHEN** user searches for tag `"project/ai/nlp"`
- **THEN** the system SHALL return only entity E3
- **AND** SHALL NOT return E1 or E2

#### Scenario: Mid-level tag returns descendants only

- **GIVEN** a tag hierarchy exists: `project` → `project/ai` → `project/ai/nlp` → `project/ai/nlp/transformers`
- **AND** entity E1 is tagged with `#project`
- **AND** entity E2 is tagged with `#project/ai`
- **AND** entity E3 is tagged with `#project/ai/nlp`
- **AND** entity E4 is tagged with `#project/ai/nlp/transformers`
- **WHEN** user searches for tag `"project/ai"`
- **THEN** the system SHALL return entities E2, E3, and E4
- **AND** SHALL NOT return E1

#### Scenario: Leaf tag with no children

- **GIVEN** tag `project/ai/nlp/transformers` has no child tags
- **AND** entity E1 is tagged with `#project/ai/nlp/transformers`
- **WHEN** user searches for tag `"project/ai/nlp/transformers"`
- **THEN** the system SHALL return entity E1 only

#### Scenario: Tag with no entities but descendants have entities

- **GIVEN** a tag hierarchy exists: `research` → `research/ml`
- **AND** tag `research` has NO entities directly associated with it
- **AND** entity E1 is tagged with `#research/ml`
- **WHEN** user searches for tag `"research"`
- **THEN** the system SHALL return entity E1
- **AND** SHALL include entities from descendant tags even when parent has none

### Requirement: Exact Match Search Option

The system SHALL provide an option to perform exact-match-only tag search when hierarchical search is not desired.

**Rationale**: Some use cases require precision over recall. Users may want to find only entities tagged with exactly `#project` and not its children. This supports filtering and advanced query scenarios.

#### Scenario: Exact match excludes children

- **GIVEN** a tag hierarchy exists: `project` → `project/ai`
- **AND** entity E1 is tagged with `#project`
- **AND** entity E2 is tagged with `#project/ai`
- **WHEN** user searches for tag `"project"` with `exact_match: true`
- **THEN** the system SHALL return only entity E1
- **AND** SHALL NOT return E2

#### Scenario: Exact match on leaf tag

- **GIVEN** tag `project/ai/nlp` exists
- **AND** entity E1 is tagged with `#project/ai/nlp`
- **WHEN** user searches for tag `"project/ai/nlp"` with `exact_match: true`
- **THEN** the system SHALL return entity E1

### Requirement: Tag Name Format

Tags SHALL use slash separators (`/`) to denote hierarchy. Tag names SHALL be stored with their full hierarchical path.

**Rationale**: Slash separators are intuitive, widely used (URLs, file paths), and easy to parse. Storing full paths simplifies queries and enables efficient string-based hierarchy navigation.

#### Scenario: Tag stored with full path

- **GIVEN** user creates a tag `#project/ai/nlp`
- **WHEN** the tag is stored
- **THEN** the tag name SHALL be stored as `"project/ai/nlp"`
- **AND** parent relationships SHALL be automatically inferred and stored

#### Scenario: Tag hierarchy from path

- **GIVEN** tag `project/ai/nlp` is created
- **WHEN** the system processes the tag
- **THEN** parent tag `project/ai` SHALL exist or be created
- **AND** `project/ai` SHALL have parent tag `project`
- **AND** `project` SHALL have no parent (root tag)

### Requirement: Performance Optimization

Hierarchical tag search SHALL use recursive queries via the `get_child_tags()` method to collect all descendant tags, then batch-fetch entities associated with the tag set.

**Rationale**: Modern databases (including SurrealDB) support recursive CTEs and efficient tree traversal. Collecting tag IDs first, then batch-fetching entities, minimizes query round-trips and enables query optimization.

#### Scenario: Efficient recursive traversal

- **GIVEN** a tag hierarchy with 5 levels and 100 total tags
- **WHEN** user searches for a root tag
- **THEN** the system SHALL collect all descendant tag IDs in a single recursive query
- **AND** SHALL fetch entities in a single batch query using the collected tag IDs

#### Scenario: Caching of tag hierarchy

- **GIVEN** tag hierarchy is relatively stable (infrequent tag creation/deletion)
- **WHEN** the same parent tag is searched multiple times
- **THEN** the system MAY cache descendant tag IDs to improve performance
- **AND** SHALL invalidate cache when tag hierarchy changes

## Implementation Notes

### Current Implementation Status

As of 2025-11-09, the tag storage system has:
- ✅ Tag storage with slash separator format (`project/ai/nlp`)
- ✅ Hierarchical parent-child relationships via `parent_tag_id`
- ✅ `get_child_tags(parent_tag_id)` method for retrieving direct children
- ⏳ `get_entities_by_tag(tag_id)` performs EXACT match only (needs hierarchical support)

### Required Changes

To implement hierarchical tag search:

1. **Update `TagStorage::get_entities_by_tag()`**
   - Add optional `exact_match: bool` parameter (default: `false`)
   - When `exact_match == false`:
     - Use `get_child_tags()` recursively to collect all descendant tag IDs
     - Query entities associated with any tag in the collected set
   - When `exact_match == true`:
     - Use existing exact-match behavior

2. **Add `TagStorage::get_all_descendant_tags()` helper**
   - Recursive helper that collects all descendant tag IDs
   - Can be implemented using `get_child_tags()` in a loop or via database recursive query
   - Returns `Vec<String>` of all descendant tag IDs

3. **Update SurrealDB implementation**
   - Modify `get_entities_by_tag()` in `crucible-surrealdb/src/eav_graph/store.rs`
   - Use SurrealDB's graph traversal or recursive query capabilities if available
   - Ensure batch entity fetch is efficient (single query with `IN` clause)

### Example Implementation Pattern

```rust
async fn get_entities_by_tag(
    &self,
    tag_id: &str,
    exact_match: bool
) -> StorageResult<Vec<String>> {
    let tag_ids = if exact_match {
        vec![tag_id.to_string()]
    } else {
        // Collect all descendant tags
        let mut all_tags = vec![tag_id.to_string()];
        all_tags.extend(self.get_all_descendant_tags(tag_id).await?);
        all_tags
    };

    // Batch fetch entities for all tags
    self.get_entities_for_tag_set(&tag_ids).await
}

async fn get_all_descendant_tags(&self, parent_tag_id: &str) -> StorageResult<Vec<String>> {
    let mut descendants = Vec::new();
    let mut queue = vec![parent_tag_id.to_string()];

    while let Some(current_tag) = queue.pop() {
        let children = self.get_child_tags(&current_tag).await?;
        for child in children {
            descendants.push(child.id.clone());
            queue.push(child.id);
        }
    }

    Ok(descendants)
}
```

## Related Specifications

- **Tag Storage**: Tag hierarchy and storage schema (to be created)
- **Entity Search**: General entity search capabilities (to be created)
- **Graph Queries**: Knowledge graph traversal patterns (to be created)

## Open Questions

1. Should there be a depth limit for hierarchical search to prevent performance issues?
   - **Recommendation**: No limit initially; monitor performance and add if needed

2. Should search support wildcards (e.g., `project/*/nlp` to match any intermediate level)?
   - **Recommendation**: Defer to future enhancement; hierarchical search covers 90% of use cases

3. How should we handle tag renames that affect hierarchy?
   - **Recommendation**: Tag rename should cascade to children (update their paths)
   - **Out of scope**: Defer to separate tag management specification

## Migration Path

### For Existing Implementations

If an implementation already has `get_entities_by_tag()` with exact-match behavior:

1. Add `exact_match` parameter with default value `false`
2. Preserve existing exact-match queries when `exact_match == true`
3. Implement hierarchical logic for `exact_match == false`
4. Update all call sites to explicitly pass `exact_match: true` if exact behavior is required
5. Update documentation and tests

### Backward Compatibility

The change is **backward compatible** if:
- Default behavior is hierarchical (most intuitive)
- Explicit `exact_match: true` flag provides old behavior
- Existing code can be gradually migrated

Alternative (if breaking change is acceptable):
- Rename `get_entities_by_tag()` → `get_entities_by_tag_exact()`
- Add new `get_entities_by_tag_hierarchical()` method
- Deprecate old method with migration guide

## Testing Requirements

### Test Coverage

All implementations SHALL include tests for:

1. ✅ Basic hierarchical search (parent returns children)
2. ✅ Multi-level hierarchy (grandchildren, great-grandchildren)
3. ✅ Exact match option
4. ✅ Empty tag (no entities) but descendants have entities
5. ✅ Leaf tag with no children
6. ✅ Mid-level tag search
7. ✅ Performance test with large tag hierarchies (>100 tags)
8. ✅ Batch entity retrieval efficiency

### Test Fixtures

Recommended test tag hierarchies:

```
research/
├── ml/
│   ├── supervised/
│   │   ├── classification/
│   │   └── regression/
│   └── unsupervised/
│       ├── clustering/
│       └── dimensionality-reduction/
└── nlp/
    ├── transformers/
    └── embeddings/

project/
├── ai/
│   ├── nlp/
│   └── cv/
└── web/
    ├── frontend/
    └── backend/
```

## Success Metrics

- ✅ Hierarchical search returns correct entities across all test scenarios
- ✅ Performance: <100ms for tag hierarchy with 1000 tags and 10,000 entities
- ✅ Exact match option works correctly
- ✅ No regression in existing tag storage functionality
- ✅ Test coverage >90% for tag search code paths
