# Contributing to Crucible

Thank you for your interest in contributing to Crucible! This document provides guidelines and information for contributors.

## Getting Started

1. **Fork the repository** and clone your fork
2. **Install Rust** (stable toolchain): https://rustup.rs/
3. **Install Just** (optional but recommended): `cargo install just`
4. **Build the project**: `cargo build` or `just build`
5. **Run tests**: `cargo test --workspace` or `just test`

## Development Workflow

### Before You Start

- Check existing [issues](https://github.com/YOUR_USERNAME/crucible/issues) to avoid duplicate work
- For large changes, open an issue first to discuss the approach
- Read the [AGENTS.md](./AGENTS.md) file for project architecture overview

### Making Changes

1. Create a feature branch from `master`
2. Make your changes with clear, focused commits
3. Write tests for new functionality
4. Ensure all tests pass: `cargo test --workspace`
5. Run formatting and lints: `cargo fmt && cargo clippy`
6. Submit a pull request

### Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `refactor`: Code change that neither fixes a bug nor adds a feature
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

**Examples:**
```
feat(parser): add support for nested wikilinks
fix(cli): correct path handling on Windows
docs: update installation instructions
```

## Code Style

### Rust Guidelines

- Follow standard Rust naming conventions
- Use `rustfmt` for formatting (run `cargo fmt`)
- Address all `clippy` warnings (run `cargo clippy`)
- Write doc comments for public items
- Prefer explicit error handling over `.unwrap()`

### Project Conventions

- **Type ownership**: Each type is defined in exactly one crate (see AGENTS.md)
- **Error handling**: Use `Result<T, E>` with descriptive error types
- **Testing**: Write unit tests for new functionality
- **Feature flags**: Use them for optional dependencies

## Testing

### Test Tiers

Tests are organized into tiers to make contributing easier. By default, only fast unit tests run:

| Tier | Command | Description |
|------|---------|-------------|
| `quick` | `just test` | Fast unit tests, no external dependencies (default) |
| `fixtures` | `just test fixtures` | Tests using docs/ or examples/test-kiln fixtures |
| `infra` | `just test infra` | Tests requiring Ollama, ACP agents, embedding endpoints |
| `slow` | `just test slow` | Performance benchmarks and timing-sensitive tests |
| `all` | `just test all` | All tiered tests (quick + fixtures + infra + slow) |
| `full` | `just test full` | Everything including ignored tests |

**For contributors:** `just test` should pass with zero external setup. This runs ~4000 fast unit tests.

**For maintainers:** `just test all` runs the full integration suite (requires infrastructure).

### Running Tests

```bash
# Fast unit tests (default - should always pass for contributors)
just test

# Include fixture tests (requires docs/ kiln)
just test fixtures

# Include infrastructure tests (requires Ollama, etc.)
just test infra

# Run all tiers
just test all

# Run specific crate
cargo test -p crucible-core

# Run with output
cargo test --workspace -- --nocapture
```

### Feature Flags for Tests

Some tests are gated behind feature flags:

- `test-fixtures` - Tests that use the docs/ kiln or examples/test-kiln
- `test-infrastructure` - Tests requiring external services (Ollama, ACP agents)
- `test-slow` - Performance/benchmark tests

To add infrastructure-dependent tests, use:

```rust
#![cfg(feature = "test-infrastructure")]
// or for individual tests:
#[cfg(feature = "test-fixtures")]
#[tokio::test]
async fn my_fixture_test() { ... }
```

## Pull Request Process

1. **Title**: Use conventional commit format
2. **Description**: Explain what changed and why
3. **Tests**: Include tests for new functionality
4. **CI**: Ensure all CI checks pass
5. **Review**: Address reviewer feedback

### PR Checklist

- [ ] Code follows project style guidelines
- [ ] Tests pass locally
- [ ] New functionality includes tests
- [ ] Documentation updated if needed
- [ ] No unrelated changes included

## Reporting Issues

When reporting bugs, please include:

- Rust version (`rustc --version`)
- Operating system
- Steps to reproduce
- Expected vs actual behavior
- Relevant logs or error messages

## Feature Requests

Feature requests are welcome! Please:

- Check if the feature was already requested
- Describe the use case clearly
- Explain why existing solutions don't work

## Questions?

- Open a [discussion](https://github.com/YOUR_USERNAME/crucible/discussions) for general questions
- Check [AGENTS.md](./AGENTS.md) for architecture questions

## License

By contributing to Crucible, you agree that your contributions will be licensed under the MIT License or Apache License 2.0, at your option.
