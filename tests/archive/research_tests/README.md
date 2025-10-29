# Research and Edge Case Tests Archive

This directory contains research tests and edge case validations that were moved from the main test suite to reduce compilation overhead while preserving their value for future reference and testing.

## Archived Tests

### Security and Edge Case Tests
- `filesystem_security_integration.rs` - Filesystem security integration tests
- `search_edge_case_tests.rs` - Search functionality edge case tests
- `edge_cases.rs` (from crucible-surrealdb) - Database edge case tests

## Purpose of These Tests

### Security Validation
- **Path Traversal Protection**: Tests for preventing access outside kiln boundaries
- **Symlink Handling**: Circular symlinks, broken symlinks, and external symlinks
- **Permission Errors**: Graceful handling of files with restricted permissions
- **Input Sanitization**: Validation of user-provided paths and inputs

### Edge Case Coverage
- **Large Files**: Memory usage and performance with very large files
- **Nested Directories**: Deep directory structure handling
- **Special Characters**: Unicode, encoding, and special character handling
- **Concurrent Access**: Multi-threaded file access scenarios
- **Error Conditions**: Network failures, disk full, and other system errors

### Research and Validation
- **Performance Limits**: Identifying system boundaries and bottlenecks
- **Security Boundaries**: Validating security measures under various conditions
- **Integration Scenarios**: Testing complex interaction patterns
- **Stress Testing**: System behavior under extreme conditions

## Archiving Reason

These tests were archived because:
- They focus on edge cases that are rare in normal usage
- They often require complex test setup with special filesystem conditions
- They can be resource-intensive (large files, deep directories, etc.)
- They are more relevant for security audits and edge case validation
- They help maintain clean separation between core functionality tests and research validation

## Future Usage

### Security Audits
- Run these tests before releases to validate security measures
- Use as regression tests for security-related fixes
- Include in security testing workflows and penetration testing

### Performance Validation
- Use for performance benchmarking and limit testing
- Validate system behavior under extreme conditions
- Test memory usage and resource management

### Development Reference
- Reference for implementing similar security measures in other components
- Examples of comprehensive edge case testing patterns
- Documentation of discovered edge cases and their solutions

## Running Archived Tests

To run these tests when needed:
```bash
# Run security and edge case tests
cargo test --manifest-path crates/crucible-cli/Cargo.toml --test archived_security_test

# Or run directly from the archive directory
cd tests/archive/research_tests
# Copy test file back to active test directory temporarily
```

## Integration with Main Tests

While archived, these tests serve as:
- Documentation of discovered edge cases and security considerations
- Reference implementations for comprehensive testing
- Validation patterns that can be applied to active tests
- Historical record of security and performance research

---

*Archived on: 2025-10-25*
*Reason: Reduce compilation overhead while preserving research value*