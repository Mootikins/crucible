# Conventional Commits for Crucible

## Format

```
<type>[(!)][scope]: <description>

[optional body]

[optional footer(s)]
```

## Types

- `feat`: New features
- `fix`: Bug fixes
- `refactor`: Code refactoring without behavior change
- `test`: Adding or updating tests
- `docs`: Documentation changes
- `chore`: Maintenance, dependencies, build changes
- `perf`: Performance improvements
- `style`: Code style changes (formatting, naming)
- `build`: Build system or dependency changes
- `ci`: CI/CD configuration changes

## Scopes

Common scopes in the Crucible project:
- `cli` - CLI commands and interfaces
- `core` - Core business logic
- `watch` - File watching functionality
- `parser` - Document parsing
- `surrealdb` - Database operations
- `daemon` - Background services
- `docs` - Documentation

## Examples

```bash
feat(cli): add comprehensive search safety protections

- Add large file memory protection (10MB file limit, 1MB content limit)
- Implement UTF-8 encoding safety with error recovery
- Add input validation (2-1000 character query limits)

BREAKING CHANGE: empty queries now return validation errors instead of help

fix(parser): maintain plain_text backward compatibility

Refactor existing parser to preserve existing plain_text field
while adding new structured content extraction capabilities.

test(surrealdb): add comprehensive integration test suite

Add 15 new integration tests covering:
- Vector embedding operations
- Multi-client scenarios
- Error handling and recovery
- Performance benchmarks

chore: update dependencies across workspace

Update all Rust dependencies to latest stable versions.
- pulldown-cmark: 0.9.0 → 0.9.2
- walkdir: 2.3.2 → 2.3.3
- All other dependencies updated accordingly
```

## Guidelines

1. **Subject line**: Keep under 50 characters, use imperative mood
2. **Scope**: Use specific module/component names when relevant
3. **Body**: Explain what and why, limit lines to 72 characters
4. **Breaking changes**: Use `!` and `BREAKING CHANGE:` footer
5. **Claude attribution**: Add "Co-Authored-By: Claude <noreply@anthropic.com>" when applicable