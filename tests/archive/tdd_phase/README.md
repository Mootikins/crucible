# TDD Phase Tests Archive

This directory contains Test-Driven Development (TDD) tests that were intentionally designed to fail during the RED phase of development. These tests have been archived to reduce compilation noise in the main test suite while preserving their value for future reference.

## Archived Tests

### Core TDD Test Files
- `embedding_pipeline_tdd.rs` - Integrated embedding generation pipeline tests (archived - depends on removed service architecture)
- `kiln_schema_tdd.rs` - Database schema migration to kiln terminology tests (archived - depends on removed migration tools)

### Restored TDD Test Files (moved to main test suite)
- `binary_detection_tdd_standalone.rs` - Binary file detection and memory protection tests ✅ RESTORED
- `binary_safety_tdd.rs` - Binary safety and security tests ✅ RESTORED
- `error_recovery_tdd.rs` - Comprehensive error recovery mechanism tests ✅ RESTORED
- `filesystem_edge_case_tdd.rs` - Filesystem security and edge case tests ✅ RESTORED
- `kiln_terminology_tdd.rs` - Kiln terminology consistency tests ✅ RESTORED
- `semantic_search_daemonless_tdd.rs` - Semantic search without external daemon tests ✅ RESTORED
- `semantic_search_json_output_tdd.rs` - Semantic search JSON output format tests ✅ RESTORED
- `semantic_search_real_integration_tdd.rs` - Real-world semantic search integration tests ✅ RESTORED
- `surrealdb_client_integration_tdd.rs` - SurrealDB client integration tests ✅ RESTORED
- `vault_processing_integration_tdd.rs` - Vault processing integration tests ✅ RESTORED

## Purpose of These Tests

### RED Phase (Current State)
All tests in this directory were designed to **FAIL** to drive implementation of missing features:

1. **Binary Safety**: Detection of binary files masquerading as text files
2. **Memory Protection**: Proper handling of large files and memory limits
3. **Error Recovery**: Graceful handling of service failures and network issues
4. **Integration Gaps**: End-to-end functionality between components
5. **Schema Migration**: Migration from vault to kiln terminology
6. **Security Boundaries**: Protection against path traversal and symlink attacks

### Implementation Guidance
These tests provide clear specifications for:
- What functionality needs to be implemented
- How components should integrate
- Edge cases that need to be handled
- Security requirements that must be met

## Future Usage

When implementing the corresponding features:
1. Copy the relevant test file back to the active test directory
2. Implement the minimal functionality needed to make the test pass
3. Refactor and optimize while keeping the test green
4. Move the test back to archive once functionality is stable

## Archiving Reason

These tests were archived because:
- They intentionally fail, causing compilation noise
- They document features that are not yet implemented
- They serve as research and specification documents
- They can be easily restored when implementation begins
- They reduce friction in the main development workflow

## Notes

- All tests follow proper TDD methodology with clear RED/GREEN/REFACTOR phases
- Tests include comprehensive documentation of implementation requirements
- Each test file contains detailed analysis of current gaps and required functionality
- Tests can be used as reference when implementing corresponding features

---

*Archived on: 2025-10-25*
*Reason: Reduce compilation noise from intentionally failing tests*