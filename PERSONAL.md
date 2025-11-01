# Crucible Refactor Checklist

_Owner: Solo project – updated 2025-11-01_

**Status**: Simplified roadmap now in place (removed 60-70% of over-engineering)

## Completed Phases

### Phase 0 – Baseline & Record ✅
- [x] Document test baseline (`docs/roadmap/testing-baseline.md`)
- [x] Identify flaky test patterns (timing, external services, lock poisoning)
- [x] Current CLI flow documented in README.md

### Phase 1 – Remove Dead Artifacts ✅
- [x] Removed legacy root tests and Phase 8 harnesses
- [x] Cleaned up obsolete documentation

### Phase 2 – Local Test Utilities ✅
- [x] Exposed `crucible_core::test_support` with shared fixtures
- [x] Created per-crate `tests::support` modules
- [x] Created `KilnStore` trait for database abstraction ✅
- [x] Implemented `InMemoryKilnStore` test mock ✅
- [x] All trait tests passing (4/4 tests green) ✅

### Phase 3 – Remove Rune Tool System ✅
**Status**: Complete (2025-11-01)
**Goal**: Remove all Rune integrations while preserving core crates

- [x] Excluded `crucible-plugins` and `crucible-rune-macros` from workspace
- [x] Removed Rune dependencies from CLI, a2a, watch, tauri
- [x] Removed Rune CLI commands and REPL integration
- [x] Deleted Rune test files and handlers
- [x] Created stub `UnifiedToolRegistry` for REPL compilation
- [x] Verified project compiles successfully (`cargo check`)
- [x] Documented removal in `docs/roadmap/rune-removal-phase3.md`

**Core Rune crates preserved**: `crates/crucible-plugins/` and `crates/crucible-rune-macros/` remain intact for future re-integration.

### Phase 4 – Database Trait Boundary ✅
**Status**: Complete (2025-11-01)
**Goal**: Start using `KilnStore` trait in actual code

- [x] Identified 3 flaky DB tests to refactor
- [x] Refactored tests to use `InMemoryKilnStore` instead of real DB
- [x] Verified **100x+ speed improvement** (from ~1-2s to 0.01s)
- [x] Documented pattern in `docs/roadmap/phase4-kiln-store-refactoring.md`

**Results**: 3 tests now run in 0.01s total, completely deterministic, zero flakiness!

### Phase 5 – LLM Trait Boundary ✅
**Status**: Complete (2025-11-01)
**Goal**: Ensure consistent usage and test coverage

- [x] Verify trait is documented
- [x] Create mock LLM implementation (`MockTextProvider`)
- [x] Add tests using mock LLM (7 tests passing)
- [x] Documented in `docs/roadmap/phase5-6-trait-boundaries.md`

**Results**: Created comprehensive `MockTextProvider` with 7 passing tests, matching quality of embedding mocks!

### Phase 6 – Embedding Trait Boundary ✅
**Status**: Complete (2025-11-01)
**Goal**: Ensure consistent usage

- [x] Verify trait usage is consistent
- [x] Document when to use each mock variant
- [x] Tests already using mocks (`MockEmbeddingProvider`, `FixtureBasedMockProvider`)
- [x] Documented in `docs/roadmap/phase5-6-trait-boundaries.md`

**Results**: Verified excellent existing mocks, documented usage patterns for test selection!

## Current Phase

### Phase 7 – Fix Flaky Tests ✅
**Status**: Complete (2025-11-01)
**Goal**: Eliminate race conditions and flaky test patterns

- [x] Fixed 8 flaky tests caused by global state race conditions
- [x] Eliminated all `TOOL_CONFIG_CONTEXT` usage in tests
- [x] Applied direct instantiation pattern (`KilnRepository::new()`)
- [x] Verified 100% success rate (5/5 consecutive test runs)
- [x] Documented patterns in `docs/roadmap/phase7-fix-flaky-tests.md`

**Results**: Improved from 66% failure rate to 100% success rate!

**Note**: Tests with `sleep()` calls are currently NOT flaky (passing consistently), so timing-based refactoring is deferred.

## Upcoming Phases

### Phase 8-12 – Cleanup & Documentation
- Phase 8: Kiln & Data Access Cleanup
- Phase 9: REPL Alignment
- Phase 10: Test Coverage Audit
- Phase 11: Documentation Refresh
- Phase 12: Final Sanity Check

---

## Anti-Goals (What We're NOT Doing)

❌ **No CommandService wrappers** - Commands call domain logic directly
❌ **No core façades** - Direct imports from crucible-core
❌ **No CliApp builder pattern** - main.rs constructs dependencies directly
❌ **No premature desktop app abstractions** - Design for CLI first

**Why?** Personal project. Over-abstraction creates maintenance burden without solving real problems.

---

## Success Metrics

- [x] **Phase 0-2**: Foundation complete, `KilnStore` trait working
- [x] **Phase 3**: Rune removed, project compiles cleanly
- [x] **Phase 4**: Using `KilnStore` in tests, **100x+ speed improvement**
- [x] **Phase 5-6**: Created `MockTextProvider`, verified embedding mocks, documented patterns
- [x] **Phase 7**: `cargo test --workspace --lib` runs 5x without failures (100% success rate)
- [x] **Phase 7**: Fixed all flaky tests, improved from 66% to 100% success rate
- [ ] **Phase 12**: Zero clippy warnings, tests pass 5/5 runs

---

## Notes & Decisions

### 2025-11-01: Phase 7 Complete - All Flaky Tests Fixed
- Fixed 8 flaky tests by eliminating global `TOOL_CONFIG_CONTEXT` state
- Tests in `kiln_tools.rs` (3) and `kiln_file_parsing_tests.rs` (5)
- Applied direct instantiation pattern: `KilnRepository::new()` instead of `from_context()`
- **100% success rate**: 5/5 consecutive test runs pass (was 66% before)
- Tests are now completely isolated - no shared global state
- Documented complete pattern in `docs/roadmap/phase7-fix-flaky-tests.md`
- Tests with `sleep()` are currently NOT flaky, so timing refactor deferred

### 2025-11-01: Roadmap Simplified
- Analyzed original roadmap - 60-70% over-engineered
- Removed CommandService, Core façades, CliApp builder phases
- Added explicit Anti-Goals section to prevent scope creep
- Focus shifted to fixing actual flakiness vs architectural purity

### 2025-11-01: Phases 5-6 Complete - Trait-Based Testing Infrastructure
- **Phase 5**: Created `MockTextProvider` for LLM testing (560 lines, 7 tests passing)
- **Phase 6**: Verified excellent existing embedding mocks (`MockEmbeddingProvider`, `FixtureBasedMockProvider`)
- Documented when to use each mock variant for different test scenarios
- Both traits (`TextGenerationProvider`, `EmbeddingProvider`) are well-abstracted
- Complete testing toolkit now available: DB mocks (Phase 4) + LLM mocks (Phase 5) + Embedding mocks (Phase 6)
- Documented in `docs/roadmap/phase5-6-trait-boundaries.md`
- Ready to apply all three mock types to fix flaky tests in Phase 7

### 2025-11-01: Phase 4 Complete - KilnStore Pattern Proven
- Refactored 3 database tests to use `InMemoryKilnStore`
- **100x+ speed improvement**: 6 tests now run in 0.01s (was ~1-2s)
- Eliminated all file I/O variability and timing flakiness
- Pattern is simple: replace TempDir+SurrealDB with `InMemoryKilnStore::new()`
- Tests are now completely deterministic
- Documented complete pattern in `docs/roadmap/phase4-kiln-store-refactoring.md`
- Ready to apply this pattern to more tests in Phase 7

### 2025-11-01: Phase 3 Complete - Rune Removed
- Removed all Rune integrations from CLI, a2a, watch, tauri crates
- Preserved core `crucible-plugins` and `crucible-rune-macros` crates
- Created stub tool registry to maintain REPL compilation
- Project compiles successfully with `cargo check`
- Documented removal path and re-integration strategy

### 2025-11-01: KilnStore Trait Complete
- Created `KilnStore` trait with 15 methods
- Implemented for `SurrealEmbeddingDatabase` (delegation)
- Created `InMemoryKilnStore` mock (fast, isolated, deterministic)
- Added 4 comprehensive tests - all passing ✅
- Ready to use in Phase 4 to fix flaky DB tests

### 2025-10-30: Baseline Established
- 309 async tests across 44 files
- 16+ ignored tests (external services)
- Main issues: timing dependencies, external services, lock poisoning
- Existing good patterns: `EmbeddingProvider` trait, `TempDir` usage

---

## Quick Reference

**Current priority**: Phase 7 (fix remaining flaky tests using mocks from Phases 4-6)

**Roadmap**: See `ROADMAP.md` for full details
**Test baseline**: See `docs/roadmap/testing-baseline.md`
**Architecture**: See `docs/ARCHITECTURE.md`
