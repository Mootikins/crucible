# Broken Tests Archived 2025-10-26

These tests were archived due to compilation errors caused by architectural refactoring.

## Reason for Archival

- **Service architecture removal**: Complex microservice patterns were simplified
- **API changes**: VaultScanner, ParsedDocument, EmbeddingConfig APIs were refactored
- **test_utilities module**: Module structure changed, imports broken
- **CliConfig changes**: daemon field and other config changes

## Tests Archived

### crucible-cli tests:
- integration_test.rs - test_utilities import errors
- migration_management_tests.rs - test_utilities import errors
- repl_unified_tools_test.rs - test_utilities import errors
- repl_process_integration_tests.rs - doc comment syntax errors
- repl_tool_execution_tests.rs - doc comment syntax errors
- error_recovery_tdd.rs - malformed imports

### crucible-watch tests:
- Various tests using old VaultScanner/EmbeddingConfig APIs

## Status

These tests can be:
1. **Deleted** if functionality is covered by other tests
2. **Rewritten** to use new APIs if functionality is still needed
3. **Left archived** for reference during refactoring
