# Disabled Integration Tests

## Overview

The following integration test files have been disabled (renamed to `.disabled` suffix) because they use deprecated APIs or test functionality that is no longer relevant.

## Disabled Files

### `block_storage_integration_tests.rs.disabled`
- **Reason**: Tests outdated `BlockStorage` trait and `Block` struct that are no longer used
- **Status**: **PERMANENTLY DISABLED** - Functionality is now tested by `src/eav_graph/integration_tests.rs`
- **Current Approach**: Uses `DocumentIngestor::ingest()` which handles end-to-end parsing → storage
- **Old API used**:
  - `Block` struct and `BlockStorage` trait (replaced by `BlockNode` and `DocumentIngestor`)
  - `store_block()`, `get_block()`, `get_blocks()` methods (replaced by `replace_blocks()`)
  - Individual block CRUD operations (replaced by document-level ingestion)

## Re-enabled Files

### `property_storage_integration_tests.rs` ✅ **RE-ENABLED**
- **Date Re-enabled**: 2025-11-11
- **Status**: All 8 tests passing
- **API Updated**: Uses current `SurrealClient::new_memory()` and existing property storage methods
- **Coverage**: Tests frontmatter → property mapping → storage pipeline

## Current Testing Approach

The current integration testing strategy uses:

1. **Document-Level Integration Tests** (`src/eav_graph/integration_tests.rs`):
   - Tests complete parsing → ingestion → storage pipeline
   - Uses `DocumentIngestor::ingest()` for end-to-end testing
   - Covers all block types, metadata, and relationships
   - 1 active test with comprehensive coverage

2. **Property Storage Tests** (`tests/property_storage_integration_tests.rs`):
   - Tests frontmatter → property mapping pipeline
   - Validates all property value types (Text, Number, Bool, Date, JSON)
   - 8 passing tests with full CRUD operations

3. **Comprehensive Unit Tests** (`src/eav_graph/store.rs`):
   - Individual method testing with 60+ test functions
   - Schema validation, query execution, data integrity
   - All storage operations and edge cases

## Date Updated

2025-11-11 - Phase 5 integration testing work
