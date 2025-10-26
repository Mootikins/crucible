# Archived Daemon Embedding Tests - 2025-10-26

## Reason for Archival

These tests were archived during config consolidation cleanup. They test against an architecture that was removed:
- `crucible_services` crate (removed)
- Over-engineered `DaemonConfig` (being simplified)
- Old embedding provider structure
- Complex test harness infrastructure

## Archived Files - This Session (2025-10-26 Evening)

### Daemon Embedding Tests (Primary Targets):
1. **embedding_content_type_tests.rs** - Tests different content types (markdown, code, etc.)
2. **embedding_mock_provider_tests.rs** - Tests mock embedding providers
3. **embedding_real_provider_tests.rs** - Tests real embedding providers
4. **embedding_storage_retrieval_tests.rs** - Tests embedding storage in database
5. **event_pipeline_integration.rs** - Tests event pipeline integration
6. **watcher_integration_tests.rs** - Tests file watcher integration with embedding

### Test Infrastructure:
7. **harness.rs** - Test harness utilities (DaemonEmbeddingHarness, EmbeddingHarnessConfig)

### Tests Dependent on Harness:
8. **batch_embedding.rs** - Tests batch embedding operations
9. **embedding_pipeline.rs** - Tests embedding pipeline
10. **re_embedding.rs** - Tests re-embedding of documents
11. **semantic_search.rs** - Tests semantic search functionality

### Previously Archived (Earlier Sessions):
- binary_safety_tdd.rs
- cli_repl_tool_consistency_tests.rs
- configuration_integration_test.rs
- daemon_test_utilities.rs
- embedding_processor_integration.rs
- embedding_test_runner.rs
- enhanced_rune_command_tests.rs
- error_recovery_integration.rs
- error_recovery_tdd.rs
- event_driven_embedding_integration.rs
- event_driven_embedding_integration_failing.rs
- integration_test.rs
- migration_management_tests.rs
- mod.rs
- performance_load_tests.rs
- repl_direct_integration_tests.rs
- repl_error_handling_comprehensive.rs
- repl_error_handling_simple.rs
- repl_process_integration_tests.rs
- repl_tool_execution_tests.rs
- repl_unified_tool_error_handling_tests.rs
- repl_unified_tools_test.rs
- repl_unit_tests.rs
- test_chat.rs
- tool_registry.rs
- vault_integration_tests.rs
- vault_processing_integration_tdd.rs

## Impact

**Before archival**: 89+ compilation errors
**After archival**: 0 compilation errors

Files archived in this session: **11 daemon embedding test files**

## Next Steps

These tests should be rewritten after Phase 2 (config consolidation) is complete, using:
- Simplified daemon architecture (daemon = watcher + background events)
- Unified `crucible-config::Config`
- New embedding provider structure
- Simpler test utilities without heavy harness infrastructure

See: `/home/moot/crucible/docs/CONFIG_CONSOLIDATION_PLAN.md`

## Status

These tests can be:
1. **Rewritten** to use new simplified APIs (preferred)
2. **Deleted** if functionality is covered by other tests
3. **Left archived** for reference during refactoring
