# Kiln Schema TDD RED Phase Summary

## Overview

Successfully created a comprehensive Test-Driven Development (TDD) test suite for migrating the database schema from vault terminology to kiln terminology. This implements the RED phase of TDD, where failing tests drive the implementation.

## Files Created

- `/home/moot/crucible/crates/crucible-cli/tests/kiln_schema_tdd.rs` - Main TDD test file
- `/home/moot/crucible/crates/crucible-cli/tests/mod.rs` - Updated to include new test module

## TDD Test Suite Structure

### Test Coverage Areas

1. **Database Tables Use Kiln Naming** (`test_database_tables_use_kiln_naming`)
   - Tests that tables are named with `kiln_` prefixes (kiln_notes, kiln_tags, kiln_metadata)
   - Verifies old vault-based tables don't exist (notes, tags, metadata)
   - Tests relation tables use kiln terminology (kiln_wikilink, kiln_tagged_with)

2. **Kiln Column Names Use Proper Terminology** (`test_kiln_column_names_use_proper_terminology`)
   - Tests columns use kiln terminology (`kiln_path` instead of `path`)
   - Verifies old vault columns don't exist in new schema
   - Tests timestamp columns use kiln terminology (`kiln_created_at`, `kiln_modified_at`)
   - Tests embedding columns use kiln terminology (`kiln_embedding`, `kiln_embedding_model`)

3. **Kiln Metadata Queries Work Correctly** (`test_kiln_metadata_queries_work_correctly`)
   - Tests CRUD operations work with kiln schema
   - Tests tag queries use kiln terminology
   - Verifies full-text search works with kiln schema

4. **Embedding Storage with Kiln Terminology** (`test_embedding_storage_with_kiln_terminology`)
   - Tests embeddings stored with kiln column names
   - Verifies embedding updates use kiln terminology
   - Tests vector operations work with kiln schema

5. **Kiln Indexes and Constraints Use Proper Naming** (`test_kiln_indexes_and_constraints_use_proper_naming`)
   - Tests index names use kiln prefixes (kiln_unique_path, kiln_tags_idx, etc.)
   - Verifies constraints use kiln terminology
   - Tests index information queries work with kiln schema

6. **Kiln Function Names Use Proper Terminology** (`test_kiln_function_names_use_proper_terminology`)
   - Tests function names use kiln terminology (fn::kiln_cosine_similarity, etc.)
   - Verifies function calls work with kiln naming
   - Tests utility functions use kiln terminology

7. **Schema Migration from Vault to Kiln** (`test_schema_migration_from_vault_to_kiln`)
   - Tests migration function exists and works
   - Verifies data integrity during migration
   - Tests migration results are verifiable

8. **Error Messages Use Kiln Terminology** (`test_error_messages_use_kiln_terminology`)
   - Tests error messages reference kiln terminology, not vault
   - Verifies constraint errors mention kiln columns
   - Tests error messages are consistent with kiln schema

9. **Kiln Schema Version Tracking** (`test_kiln_schema_version_tracking`)
   - Tests schema version is tracked in kiln metadata
   - Verifies version updates work correctly
   - Tests migration history is maintained

10. **Performance with Kiln Schema** (`test_performance_with_kiln_schema`)
    - Tests CRUD operations performance with kiln schema
    - Verifies query performance is maintained
    - Tests indexed queries work efficiently

## Current Schema Analysis

### Identified Vault Terminology to Replace

**Table Names:**
- `notes` → `kiln_notes`
- `tags` → `kiln_tags`
- `metadata` → `kiln_metadata`
- `wikilink` → `kiln_wikilink`
- `tagged_with` → `kiln_tagged_with`
- `embeds` → `kiln_embeds`
- `relates_to` → `kiln_relates_to`

**Column Names:**
- `path` → `kiln_path`
- `created_at` → `kiln_created_at`
- `modified_at` → `kiln_modified_at`
- `embedding` → `kiln_embedding`
- `embedding_model` → `kiln_embedding_model`
- `embedding_updated_at` → `kiln_embedding_updated_at`

**Index Names:**
- `unique_path` → `kiln_unique_path`
- `tags_idx` → `kiln_tags_idx`
- `folder_idx` → `kiln_folder_idx`
- `content_search` → `kiln_content_search`
- `title_search` → `kiln_title_search`
- `embedding_idx` → `kiln_embedding_idx`

**Function Names:**
- `fn::cosine_similarity` → `fn::kiln_cosine_similarity`
- `fn::normalize_tag` → `fn::kiln_normalize_tag`
- `fn::get_folder` → `fn::kiln_get_folder`
- `fn::get_extension` → `fn::kiln_get_extension`

## Test Infrastructure

### Test Context (`KilnSchemaTestContext`)
- In-memory database client for testing
- Temporary directory management
- Schema version tracking
- Cleanup procedures

### Test Runner (`run_all_kiln_schema_tests`)
- Reports test status and progress
- Provides clear RED phase messaging
- Shows expected failure states
- Guides implementation progress

## RED Phase Verification

### Test Compilation
✅ **All tests compile successfully** with proper imports and dependencies

### Test State
✅ **All 10 main tests are ignored** (`#[ignore]`) - proper RED phase setup
✅ **Infrastructure tests pass** (context creation, test runner)
✅ **Individual test verification confirmed** - test fails when enabled with expected error about missing kiln schema

### Expected Failure Behavior
When enabled, tests fail with clear messages like:
- `"Old 'notes' table should not exist"`
- `"kiln_notes table should exist"`
- `"Should have kiln_path column"`
- `"Function 'fn::kiln_cosine_similarity' should exist"`

## Implementation Path Forward

### Phase 1: Schema Definition
1. Create new kiln schema file (`kiln_schema.surql`)
2. Define kiln tables with proper column names
3. Create kiln indexes and constraints
4. Define kiln functions

### Phase 2: Schema Migration
1. Implement migration function (`kiln::migrate_from_vault()`)
2. Create data transformation logic
3. Preserve data integrity during migration
4. Add migration history tracking

### Phase 3: Code Updates
1. Update Rust type definitions (`schema_types.rs`)
2. Modify database operations to use kiln terminology
3. Update error messages and comments
4. Adjust query builders and utilities

### Phase 4: Test Enablement
1. Remove `#[ignore]` attributes progressively
2. Fix failing tests one by one
3. Verify each implementation phase works
4. Achieve GREEN phase (all tests pass)

## Benefits of This TDD Approach

1. **Clear Specification**: Tests clearly define what the kiln schema should look like
2. **Incremental Implementation**: Tests can be enabled progressively as implementation progresses
3. **Regression Prevention**: Tests ensure future changes don't break kiln terminology
4. **Documentation**: Tests serve as executable documentation of the kiln schema requirements
5. **Risk Mitigation**: Comprehensive test coverage reduces risk of incomplete migration

## Next Steps

1. **Begin Implementation**: Start with schema definition files
2. **Enable First Test**: Remove `#[ignore]` from `test_database_tables_use_kiln_naming`
3. **Implement and Verify**: Implement kiln tables, fix test, move to next
4. **Progressive Migration**: Work through each test systematically
5. **Final Validation**: Remove all `#[ignore]` attributes, ensure all tests pass

## Technical Notes

- Tests use the existing `SurrealClient` from `crucible-surrealdb`
- In-memory database ensures tests are isolated and fast
- Proper async/await patterns throughout
- Comprehensive error handling and cleanup
- Performance testing included to ensure migration doesn't degrade performance

---

**Status**: ✅ RED phase complete. Ready for implementation phase.