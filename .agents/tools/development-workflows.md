# Development Workflows

Common workflows for AI agents working on Crucible development.

## Code Generation Workflows

### New Rust Module
1. Create module file in appropriate crate
2. Add `pub mod` declaration to parent
3. Implement basic structure with error handling
4. Add to `lib.rs` exports
5. Create corresponding tests

### New Svelte Component
1. Create component file in `packages/desktop/src/lib/components/`
2. Define props interface
3. Implement component logic
4. Add to component exports
5. Create usage examples

### New Package/Crate
1. Create directory structure
2. Add `Cargo.toml` or `package.json`
3. Update workspace configuration
4. Create initial source files
5. Add to CI/CD configuration

## Testing Workflows

### Rust Testing
1. Add unit tests in same file
2. Create integration tests in `tests/`
3. Add property-based tests for complex logic
4. Update test coverage reports

### Frontend Testing
1. Add component tests
2. Create E2E tests for user flows
3. Test responsive design
4. Verify accessibility

## Documentation Workflows

### API Documentation
1. Add doc comments to public functions
2. Include usage examples
3. Document error conditions
4. Update README files

### Code Documentation
1. Add inline comments for complex logic
2. Document design decisions
3. Create architecture diagrams
4. Update changelog

## Refactoring Workflows

### Safe Refactoring
1. Identify refactoring scope
2. Create comprehensive tests
3. Make incremental changes
4. Verify functionality preserved
5. Update documentation

### Breaking Changes
1. Plan migration strategy
2. Create deprecation warnings
3. Update all usage sites
4. Remove deprecated code
5. Update version numbers

## Debugging Workflows

### Issue Investigation
1. Reproduce the issue
2. Identify root cause
3. Create minimal test case
4. Implement fix
5. Add regression test

### Performance Issues
1. Profile the code
2. Identify bottlenecks
3. Implement optimizations
4. Measure improvements
5. Document changes
