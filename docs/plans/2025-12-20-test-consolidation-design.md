# Test Consolidation Design

> Aggressive restructuring of the crucible test suite for faster CI and reduced maintenance burden.

**Date:** 2025-12-20
**Status:** Approved
**Goals:** Speed up CI (compilation + execution), reduce maintenance burden

## Current State

- **~4,000 tests** across the workspace
- **~2 minutes** test execution time
- **~1.5 minutes** compilation time
- **103 ignored tests** (various reasons)
- **Top test-heavy crates:** crucible-llm (20 files), crucible-cli (17), crucible-parser (16), crucible-acp (14), crucible-surrealdb (12)

## Constraints

- **Keep contract tests** — Catch API/library upgrade issues
- **Keep real SurrealDB integration tests** — Catch issues mocks wouldn't
- **Use cargo-nextest** — Already installed, enables parallel execution and profiles

## Design: Test Tiers

### Tier Structure

```
crates/<crate>/
├── src/
│   └── *.rs              # Inline unit tests (#[cfg(test)] mod tests)
├── tests/
│   ├── unit/             # Tier 1: Fast, isolated unit tests
│   │   └── *.rs
│   ├── integration/      # Tier 2: Real I/O, real DB
│   │   └── *.rs
│   ├── contract/         # Tier 3: API/trait contracts
│   │   └── *.rs
│   └── slow/             # Tier 4: Manual-only tests
│       └── *.rs
```

### Tier Definitions

| Tier | Max time/test | Dependencies | Mocking | When to run |
|------|---------------|--------------|---------|-------------|
| Unit | 10ms | None (pure functions) | Required for I/O | Every commit |
| Integration | 500ms | Real DB, real files | None | PR only |
| Contract | 50ms | Minimal | Optional | PR + dep updates |
| Slow/Manual | N/A | Any | Any | Manual only |

### Nextest Configuration

`.config/nextest.toml`:

```toml
[profile.default]
retries = 0
fail-fast = true

[profile.unit]
filter = "test(/unit/) or test(#[cfg(test)])"

[profile.integration]
filter = "test(/integration/)"

[profile.contract]
filter = "test(/contract/)"

[profile.ci]
filter = "not test(/slow/) and not test(/manual/)"
retries = 1
```

### CI Workflow

```yaml
jobs:
  test:
    steps:
      - run: cargo nextest run --profile unit      # Fast feedback first
      - run: cargo nextest run --profile ci        # Full non-slow suite
```

## Consolidation Strategy

### Pattern 1: Parameterized Tests

Replace repetitive tests with parameterized versions using `test-case`:

```rust
// Before: 5 separate test functions
#[test] fn parse_heading_h1() { ... }
#[test] fn parse_heading_h2() { ... }

// After: 1 parameterized test
#[test_case("# H1", 1; "h1")]
#[test_case("## H2", 2; "h2")]
fn parse_heading(input: &str, expected_level: u8) { ... }
```

### Pattern 2: Shared Test Fixtures

Each crate's `test-utils` feature exposes:
- Mock builders (`MockEmbeddingProvider::new()`)
- Fixture factories (`TestNote::builder().with_frontmatter().build()`)
- Shared assertions (`assert_parses_to!(input, expected)`)

### Pattern 3: Remove Redundant Tests

Delete tests that:
- Duplicate coverage from other tests (integration testing same path as unit)
- Test private implementation details (test via public API instead)
- Are pure smoke tests with no unique assertions

## Crate-Specific Changes

### crucible-llm (20 test files)

- Mock all HTTP calls (Ollama, OpenAI) in unit tests
- Move `test_burn_integration.rs`, `test_llama_cpp_backend.rs` to `slow/`
- Move `toon_eval/` to `slow/` (eval harness, not CI)
- Consolidate streaming tests into parameterized form

### crucible-parser (16 test files)

- Parameterize repetitive parsing tests
- Consolidate into fewer test files with test cases as data
- Keep property tests as-is

### crucible-surrealdb (12 test files)

- Keep real DB tests in `integration/` tier
- Create `MockStorage` for unit tests
- Share single DB setup across integration tests

### crucible-cli (17 test files)

- Consolidate `config_tests.rs` + `config_command_tests.rs`
- Parameterize `agent_factory_*.rs` tests
- Keep TUI snapshot tests as-is

### crucible-acp (14 test files)

- Create `MockAcpClient` for unit tests
- Keep 2-3 real integration tests
- Move rest to unit tier with mocks

## Migration Phases

### Phase 1: Infrastructure

1. Add `.config/nextest.toml` with profiles
2. Update CI workflow to use `cargo nextest`
3. Add `test-case` crate to workspace dependencies
4. Create tier directories in each crate

### Phase 2: Quick Wins

1. Move `#[ignore = "slow..."]` tests to `tests/slow/`
2. Move existing contract tests to `tests/contract/`
3. Parameterize obvious repetitive tests in crucible-parser

### Phase 3: Mock Infrastructure

1. Expand crucible-llm mock providers
2. Create `MockStorage` for crucible-surrealdb
3. Create `MockAcpClient` for crucible-acp

### Phase 4: Major Consolidation

1. Migrate crucible-llm tests to use mocks
2. Consolidate crucible-cli config/factory tests
3. Parameterize crucible-parser tests
4. Remove genuinely redundant tests

### Phase 5: Cleanup

1. Delete empty/obsolete test files
2. Update AGENTS.md with testing guidelines
3. Document tier expectations for contributors

## Success Metrics

| Metric | Current | Target |
|--------|---------|--------|
| CI test time | ~2 min | < 1 min |
| Unit tier time | N/A | < 30s |
| Compilation time | ~1.5 min | ~1 min |
| Test count | ~4,000 | ~2,500 |
| Ignored tests | 103 | < 30 |
| Test files | ~100 | ~60 |

## Compilation Speedup Levers

- Fewer test binaries (consolidate `tests/*.rs` files)
- `test-utils` features only enabled for tests
- Remove unused dev-dependencies

## Verification

The consolidation is successful when:

1. `cargo nextest run --profile unit` < 30s
2. CI total time (build + test) < 3 min
3. New contributors understand tier structure
4. No flaky CI test issues
