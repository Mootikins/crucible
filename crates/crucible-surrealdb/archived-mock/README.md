# Archived Mock Implementation & Tests

This directory contains mock-based tests that were archived due to a critical architectural issue.

## The Architectural Problem

**CRITICAL ISSUE**: The current `SurrealClient` in `multi_client.rs` is actually a mock, not a real client!

### What Should Be:
- **`SurrealClient`** → Wrapper around real `surrealdb::Surreal<Db>` for production use
- **`MockSurrealClient`** → In-memory mock for simple unit tests
- Production code uses the real `SurrealClient`
- Tests choose between real or mock as needed

### What Currently Is:
- **`SurrealClient`** → IS THE MOCK (in-memory HashMap with custom SQL parser)
- **No real client exists** → Production code uses the mock!
- Mock grew increasingly complex trying to replicate SurrealDB:
  - Custom SQL parser with growing edge cases
  - Graph traversal query support
  - DELETE statement pattern matching
  - Nested query handling
  - Relationship storage and traversal

**The names are backwards and the mock is pretending to be production code.**

## Contents

- `multi_client.rs` - In-memory mock SurrealDB client with custom SQL parser
- `embedding_storage_tests.rs` - Tests that depended on the mock
- `common/` - Test utilities for the mock-based tests

## Required Refactoring

To fix this architectural issue:

### Phase 1: Create Real SurrealClient
1. Create new `SurrealClient` that wraps `surrealdb::Surreal<Db>`
2. Implement proper connection management (file, memory, remote)
3. Expose real SurrealDB query API (no custom SQL parsing)

### Phase 2: Rename Mock
1. Rename current `SurrealClient` → `MockSurrealClient`
2. Keep mock simple (basic CRUD only, no complex queries)
3. Or just use `surrealdb::Surreal<Mem>` directly in tests

### Phase 3: Update All Usages
Files that need updating:
- `vault_integration.rs` - All vault operations
- `vault_processor.rs` - Vault processing pipeline
- `vault_scanner.rs` - Document scanning
- `vault_pipeline_connector.rs` - Pipeline coordination
- `embedding_pipeline.rs` - Embedding storage

### Phase 4: Restore Tests
1. Update tests to use real `SurrealClient`
2. Use real SurrealDB in-memory mode for integration tests
3. Only use mocks for true unit tests (if needed)

## Date Archived

2025-10-27

## Context

This was archived during work on embedding storage tests after a graph relations refactor. The tests were failing due to the mock not supporting complex graph traversal queries, and attempts to extend the mock revealed it had grown too complex.
