# Disabled Integration Tests

## Overview

The following integration test files have been disabled (renamed to `.disabled` suffix) because they use a deprecated API that no longer exists in the current EAVGraphStore implementation.

## Disabled Files

### `block_storage_integration_tests.rs.disabled`
- **Reason**: Uses old `BlockStorage` trait methods that are no longer implemented
- **Old API used**:
  - `EAVGraphStore::new_in_memory()` (doesn't exist, should be `SurrealClient::new_isolated_memory()` + `EAVGraphStore::new(client)`)
  - `store_block()`, `get_block()`, `get_blocks()`, etc. (replaced by `replace_blocks()`)
  - `store_entity()` (replaced by `upsert_entity()`)

### `property_storage_integration_tests.rs.disabled`
- **Reason**: Uses old property storage API
- **Old API used**:
  - `SurrealClient::new_memory()` (should use `new_isolated_memory()` for tests)
  - Old property storage methods that may no longer match current API

## Current API

The current EAVGraphStore uses:
- `upsert_entity(entity: &Entity)` - Store/update entities
- `upsert_property(property: &Property)` - Store/update properties
- `replace_blocks(entity_id, blocks: &[BlockNode])` - Replace all blocks for an entity
- `upsert_embedding(embedding: &EmbeddingVector)` - Store embeddings
- `upsert_tag(tag: &SurrealTag)` - Store tags
- `upsert_relation(relation: &SurrealRelation)` - Store relations

## Next Steps

To re-enable these tests:

1. Update the test setup to use:
   ```rust
   let client = SurrealClient::new_isolated_memory().await.unwrap();
   apply_eav_graph_schema(&client).await.unwrap();
   let store = EAVGraphStore::new(client);
   ```

2. Replace old method calls with new API:
   - `store_block(block)` → `replace_blocks(&entity_id, &[block])`
   - `store_entity(entity)` → `upsert_entity(&entity)`
   - Update assertions to match new return types

3. Consider whether these tests duplicate coverage already provided by the unit tests in `src/eav_graph/store.rs`

## Date Disabled

2025-11-09 - Test isolation refactor
