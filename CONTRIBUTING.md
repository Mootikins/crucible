# Contributing to Crucible

> **Status**: Active Contribution Guidelines
> **Version**: 1.0.0
> **Date**: 2025-10-23
> **Purpose**: Guidelines for contributing to the Crucible knowledge management system

Thank you for your interest in contributing to Crucible! This document provides guidelines and processes for contributing to the project.

## Table of Contents

- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Code Style Guidelines](#code-style-guidelines)
- [Testing Guidelines](#testing-guidelines)
- [Documentation Standards](#documentation-standards)
- [Pull Request Process](#pull-request-process)
- [Community Guidelines](#community-guidelines)

## Getting Started

### Prerequisites

Before contributing, ensure you have the following installed:

```bash
# Rust toolchain
rustc --version  # 1.70.0 or later
cargo --version

# Node.js (for frontend development)
node --version  # 18.0.0 or later
npm --version

# Optional: pnpm for faster package management
npm install -g pnpm
```

### Initial Setup

1. **Fork the Repository**
   ```bash
   # Fork the repository on GitHub, then clone your fork
   git clone https://github.com/your-username/crucible.git
   cd crucible
   ```

2. **Add Upstream Remote**
   ```bash
   git remote add upstream https://github.com/matthewkrohn/crucible.git
   ```

3. **Install Dependencies**
   ```bash
   # Rust dependencies
   cargo build

   # Frontend dependencies
   cd packages/web
   npm install
   cd ../..
   ```

4. **Run Development Setup**
   ```bash
   ./scripts/setup.sh
   ```

5. **Verify Installation**
   ```bash
   cargo test
   cargo run -p crucible-cli -- --help
   ```

## Development Workflow

### 1. Create a Feature Branch

```bash
# Sync with upstream
git fetch upstream
git checkout main
git merge upstream/main

# Create feature branch
git checkout -b feature/your-feature-name
# or
git checkout -b fix/issue-number-description
```

### 2. Make Your Changes

- Follow the [Code Style Guidelines](#code-style-guidelines)
- Write tests for new functionality
- Update relevant documentation
- Ensure all tests pass

### 3. Test Your Changes

```bash
# Run all tests
cargo test

# Run tests with coverage
cargo tarpaulin --out Html

# Run clippy for linting
cargo clippy -- -D warnings

# Check formatting
cargo fmt -- --check

# Build documentation
cargo doc --no-deps
```

### 4. Commit Your Changes

```bash
# Stage changes
git add .

# Commit with conventional commit format
git commit -m "feat: add new feature description"
# or
git commit -m "fix: resolve issue description"
```

## Code Style Guidelines

### Rust Code Style

1. **Follow rustfmt standards**
   ```bash
   cargo fmt
   ```

2. **Use clippy lints**
   ```bash
   cargo clippy -- -D warnings
   ```

3. **Naming Conventions**
   - Functions and variables: `snake_case`
   - Types and structs: `PascalCase`
   - Constants: `SCREAMING_SNAKE_CASE`
   - Modules: `snake_case`

4. **Documentation Comments**
   ```rust
   /// Brief description of the function
   ///
   /// # Arguments
   ///
   /// * `param1` - Description of parameter
   /// * `param2` - Description of parameter
   ///
   /// # Returns
   ///
   /// Description of return value
   ///
   /// # Examples
   ///
   /// ```
   /// let result = function_call();
   /// assert_eq!(result, expected_value);
   /// ```
   pub fn example_function(param1: Type1, param2: Type2) -> ReturnType {
       // Implementation
   }
   ```

5. **Error Handling**
   - Use `Result<T, E>` for fallible operations
   - Implement proper error types with `thiserror`
   - Handle errors gracefully with appropriate user messages

### TypeScript/Svelte Code Style

1. **Use camelCase for variables and functions**
2. **Use PascalCase for components and classes**
3. **Add JSDoc comments for public functions**
4. **Follow Prettier formatting standards**

## Testing Guidelines

### Test Organization

```rust
// Unit tests go in the same module
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_functionality() {
        // Test implementation
    }
}

// Integration tests go in tests/ directory
// e.g., tests/integration_test.rs
```

### Test Coverage Requirements

- **New Features**: Must have unit tests with >80% line coverage
- **Bug Fixes**: Must include regression tests
- **Public APIs**: Must have comprehensive documentation tests
- **CLI Commands**: Must have integration tests

### Test Categories

1. **Unit Tests**: Fast, isolated tests for individual functions
2. **Integration Tests**: Tests for component interactions
3. **End-to-End Tests**: Full workflow tests
4. **Performance Tests**: Benchmarks for critical paths

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run ignored tests
cargo test -- --ignored

# Run benchmarks
cargo bench
```

## Documentation Standards

### Code Documentation

1. **Public APIs**: Must have comprehensive rustdoc comments
2. **Modules**: Should have module-level documentation
3. **Examples**: Include working examples in documentation
4. **Error Conditions**: Document all possible error cases

### Project Documentation

1. **Architecture Changes**: Update `/docs/ARCHITECTURE.md`
2. **New Features**: Update `/docs/NEW_FEATURES_AND_CAPABILITIES.md`
3. **API Changes**: Update `/docs/API_DOCUMENTATION.md`
4. **CLI Changes**: Update `/docs/CLI_REFERENCE.md`

### Documentation Format

```markdown
# Document Title

> **Status**: Active/Archive/Draft
> **Version**: X.Y.Z
> **Date**: YYYY-MM-DD
> **Purpose**: Brief description

## Overview

Brief description of what this document covers.

## Usage

### Basic Example

```rust
// Code example
```

## See Also

- [Related Document](./related-document.md)
- [API Reference](./API_DOCUMENTATION.md)
```

## Pull Request Process

### Before Submitting

1. **Ensure Code Quality**
   ```bash
   cargo fmt
   cargo clippy -- -D warnings
   cargo test
   ```

2. **Update Documentation**
   - Update relevant documentation files
   - Add examples for new features
   - Update CHANGELOG.md if applicable

3. **Create Draft PR**
   - Use descriptive title
   - Link to relevant issues
   - Include testing instructions

### Pull Request Template

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing
- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Manual testing completed
- [ ] Performance impact considered

## Checklist
- [ ] Code follows style guidelines
- [ ] Self-review completed
- [ ] Documentation updated
- [ ] CHANGELOG.md updated
```

### Review Process

1. **Automated Checks**: CI/CD pipeline runs tests and linting
2. **Code Review**: At least one maintainer review required
3. **Documentation Review**: Documentation changes reviewed
4. **Testing Review**: Test coverage and quality verified
5. **Approval**: Maintainer approval required for merge

### Merge Requirements

- All automated checks must pass
- At least one maintainer approval
- Documentation updated if needed
- No merge conflicts
- Tests passing on all targets

## Community Guidelines

### Code of Conduct

We are committed to providing a welcoming and inclusive environment. Please:

- Be respectful and considerate
- Use inclusive language
- Focus on constructive feedback
- Welcome newcomers and help them learn

### Communication Channels

- **GitHub Issues**: Bug reports and feature requests
- **GitHub Discussions**: General questions and community discussion
- **Pull Requests**: Code contributions and reviews

### Getting Help

1. **Check Documentation**: Review existing documentation first
2. **Search Issues**: Check if your question has been answered
3. **Create Discussion**: Start a GitHub discussion for questions
4. **Open Issue**: Report bugs or request features via GitHub issues

## Areas of Contribution

### Code Contributions

1. **Core Features**: ScriptEngine services, database operations
2. **CLI Tools**: New commands, improved user experience
3. **Frontend**: Svelte components, user interface improvements
4. **Performance**: Optimizations, caching, memory management
5. **Security**: Security improvements, vulnerability fixes

### Documentation Contributions

1. **API Documentation**: Improve rustdoc comments
2. **User Guides**: Tutorials, getting started guides
3. **Examples**: Code examples, use cases
4. **Architecture**: Design documents, technical explanations

### Testing Contributions

1. **Test Coverage**: Add tests for uncovered code
2. **Integration Tests**: End-to-end workflow tests
3. **Performance Tests**: Benchmarks and profiling
4. **Bug Reproduction**: Tests for reported issues

## Recognition

Contributors are recognized in several ways:

- **Contributors List**: Automatic recognition in GitHub
- **Release Notes**: Mentioned in release notes for significant contributions
- **Documentation**: Attributed in documentation for major contributions
- **Community**: Acknowledged in community discussions

## License

By contributing to Crucible, you agree that your contributions will be licensed under the same license as the project.

## Questions?

If you have questions about contributing:

1. Check this document first
2. Search existing GitHub issues and discussions
3. Create a new GitHub discussion
4. Contact maintainers via GitHub issues

Thank you for contributing to Crucible! 🚀