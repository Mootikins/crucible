# Crucible Roadmap

Local-first knowledge management means the CLI and future desktop app must share a simple, testable core. The target architecture is:

```
UI layers (CLI, desktop, agents)
          │
          ▼
    crucible-core (domain logic)
          ├── crucible-config   (configuration)
          ├── crucible-surrealdb (storage via trait)
          └── crucible-llm      (LLM/embeddings via trait)
```

This roadmap focuses on **fixing test flakiness** and **establishing minimal trait boundaries** for swappable dependencies (database, LLM, embeddings). The goal is a working, well-tested personal knowledge management system—not a perfectly abstracted framework.

---

## Anti-Goals

**What we're NOT doing and why:**

- **No CommandService wrappers** - Commands can call domain logic directly; we don't need service layers for a single-binary CLI
- **No core façades** - crucible-core exposes domain types directly; presentation layers can import what they need
- **No CliApp builder pattern** - main.rs can construct dependencies directly; premature lifecycle management adds complexity without solving real problems
- **No premature desktop app abstractions** - Design for CLI testability today; refactor when we actually build the desktop app
- **No Rune tool system (for now)** - Removing until MVP is stable; scripted tools aren't critical for core workflows

**Why?** This is a personal project. Over-abstraction creates maintenance burden and makes debugging harder. We add complexity only when it solves concrete flakiness or enables trait-based mocking.

---

## Phase 0 – Baseline & Record

**WHY:** Establish measurable starting point so we know if we're improving

**WHAT:**
- Run `cargo test --workspace --no-run` and document which crates build tests
- Run `cargo test --workspace` and capture flaky test patterns (race conditions, timing issues, global state conflicts)
- Document current CLI boot flow in README.md (config → command dispatch → tool execution)
- Create `PERSONAL.md` checklist for tracking phase completion

**COMPLETION CRITERIA:**
- [ ] `docs/roadmap/testing-baseline.md` exists with test counts and flaky test list
- [ ] README.md documents current architecture (before refactor)
- [ ] PERSONAL.md has phase checklist

---

## Phase 1 – Remove Dead Artifacts

**WHY:** Dead code creates maintenance burden and false positives in searches

**WHAT:**
- Delete unused workspace-root `tests/*.rs` and archived test harnesses
- Remove legacy Phase 8 docs that describe removed architecture (`README_PHASE8_INTEGRATION_TESTS.md`, etc.)
- Update CONTRIBUTING.md: integration tests live inside workspace crates (prevent future workspace-root pollution)

**COMPLETION CRITERIA:**
- [ ] `tests/` directory removed or contains only actively maintained integration tests
- [ ] No references to removed `crucible-services` or Phase 8 architecture in docs
- [ ] CONTRIBUTING.md updated

---

## Phase 2 – Local Test Utilities

**WHY:** Shared test fixtures reduce boilerplate and ensure consistent test setup across crates

**WHAT:**
- Expose `crucible_core::test_support` with common fixtures (temp kiln, mock configs, sample documents)
- Add per-crate `tests::support` modules that wrap crate-specific helpers (CLI arg parsing, REPL mocking)
- Port existing CLI integration tests to use shared fixtures, removing custom env/tempfile plumbing
- Add smoke tests to verify fixtures work correctly

**COMPLETION CRITERIA:**
- [ ] `crucible_core::test_support` module exists and is documented
- [ ] At least 3 test files migrated to use shared fixtures
- [ ] No `std::env::set_var` calls in migrated tests (all use config structs)
- [ ] Smoke tests pass: `cargo test -p crucible-core test_support`

---

## Phase 3 – Remove Rune Tool System ✅

**WHY:** Rune tools aren't critical for MVP; removing them simplifies architecture and removes a dependency

**STATUS:** Complete (2025-11-01)

**WHAT WAS DONE:**
- Excluded `crucible-plugins` and `crucible-rune-macros` from workspace (preserved for re-integration)
- Removed all Rune dependencies from CLI, a2a, watch, tauri crates
- Removed Rune CLI commands (`:rune`, `Run` variant)
- Deleted Rune test files and handler implementations
- Created stub `UnifiedToolRegistry` to maintain REPL compilation
- Verified project compiles successfully with `cargo check`

**COMPLETION CRITERIA:**
- [x] Rune code removed from all integration points
- [x] Core Rune crates preserved in `crates/` for future use
- [x] REPL compiles without tool execution
- [x] Project compiles: `cargo check` succeeds
- [x] Documented in `docs/roadmap/rune-removal-phase3.md`

---

## Phase 4 – Database Trait Boundary

**WHY:** Enable mocking SurrealDB in tests to eliminate flaky network/timing issues

**WHAT:**
- Define minimal `KilnStore` trait in `crucible-surrealdb` with essential operations:
  - `async fn store_embedding(&self, ...) -> Result<()>`
  - `async fn get_embedding(&self, file_path: &str) -> Result<Option<EmbeddingData>>`
  - `async fn search_similar(&self, query_embedding: &[f32], top_k: u32) -> Result<Vec<SearchResultWithScore>>`
  - `async fn delete_document(&self, file_path: &str) -> Result<bool>`
  - Other methods as needed by actual use cases
- Implement `KilnStore` for `SurrealEmbeddingDatabase`
- Create `InMemoryKilnStore` in test utilities for fast, isolated testing
- Update kiln processor and search commands to accept `Arc<dyn KilnStore>` instead of direct `SurrealEmbeddingDatabase`

**COMPLETION CRITERIA:**
- [ ] `trait KilnStore` defined in `crucible-surrealdb/src/kiln_store.rs`
- [ ] `impl KilnStore for SurrealEmbeddingDatabase` compiles
- [ ] `InMemoryKilnStore` supports basic CRUD operations
- [ ] At least one test uses `InMemoryKilnStore` instead of real SurrealDB
- [ ] All existing tests still pass: `cargo test --workspace`

---

## Phase 5 – LLM Trait Boundary

**WHY:** Enable mocking LLM calls in tests to avoid API dependencies and flakiness

**NOTE:** The `TextGenerationProvider` trait already exists in `crucible-llm`. This phase is about ensuring it's being used consistently and has good test coverage.

**WHAT:**
- Verify existing `TextGenerationProvider` trait is minimal and well-designed
- Create `MockLLM` in test fixtures that returns predictable responses (if not already present)
- Update REPL and agent code to accept `Arc<dyn TextGenerationProvider>` where needed
- Add tests using mock LLM instead of real API

**COMPLETION CRITERIA:**
- [ ] `TextGenerationProvider` trait is documented
- [ ] Mock LLM implementation exists with configurable test responses
- [ ] At least one test uses mock LLM instead of real API
- [ ] All existing tests still pass

---

## Phase 6 – Embedding Trait Boundary

**WHY:** Enable mocking embeddings in tests to avoid slow/flaky embedding generation

**NOTE:** The `EmbeddingProvider` trait already exists with excellent mock implementations (`MockEmbeddingProvider`, `FixtureBasedMockProvider`). This phase is about ensuring consistent usage.

**WHAT:**
- Verify `EmbeddingProvider` trait is being used throughout the codebase
- Ensure tests use `MockEmbeddingProvider` or `FixtureBasedMockProvider` instead of real embeddings
- Document when to use each mock (deterministic vs fixture-based)
- Add tests for semantic search using mock embeddings

**COMPLETION CRITERIA:**
- [ ] `EmbeddingProvider` trait usage is consistent
- [ ] Mock embedding providers are documented
- [ ] Semantic search tests use mock embeddings
- [ ] All tests pass without requiring Ollama/OpenAI: `cargo test --workspace`

---

## Phase 7 – Fix Flaky Tests

**WHY:** This is the MAIN PAIN POINT - tests must be reliable to support development

**WHAT:**
- Audit all tests for race conditions (shared global state, timing assumptions, filesystem conflicts)
- Fix or rewrite flaky tests using:
  - Isolated temp directories per test (already started in Phase 2)
  - Config structs instead of env vars (already migrated)
  - Trait mocking instead of real I/O (Phases 4-6)
  - Replace `sleep()` calls with event-driven synchronization (channels, completion signals)
  - Explicit cleanup in test teardown
- Add test isolation guards where needed (e.g., mutex around global state if unavoidable)
- Document any remaining known-flaky tests in KNOWN_ISSUES.md with repro steps

**COMPLETION CRITERIA:**
- [ ] `cargo test --workspace` runs 3 times in a row without failures
- [ ] No `#[ignore]` tests except those requiring real API keys
- [ ] KNOWN_ISSUES.md documents any acceptable flakiness (with mitigation plan)
- [ ] All `sleep()` calls in tests have been replaced or justified with comments

---

## Phase 8 – Kiln & Data Access Cleanup

**WHY:** Consolidate data access patterns to reduce test surface area

**WHAT:**
- Replace `kiln_processor`'s external binary invocation with in-process code using `KilnStore` trait
- Refactor `KilnRepository` to use `Arc<dyn KilnStore>` instead of direct SurrealDB client
- Add focused unit tests for:
  - Metadata refresh (using `InMemoryKilnStore`)
  - Embedding updates (using `MockEmbeddingProvider`)
  - Query parsing edge cases
- Remove redundant kiln access code paths

**COMPLETION CRITERIA:**
- [ ] No external kiln binary invocations in code
- [ ] `KilnRepository` methods accept `Arc<dyn KilnStore>`
- [ ] Unit tests cover metadata/embedding workflows using mocks
- [ ] Integration tests use real DB to verify end-to-end: `cargo test -p crucible-cli --test kiln_integration`

---

## Phase 9 – REPL Alignment

**WHY:** REPL currently has duplicate config loading and global state issues

**WHAT:**
- Refactor `commands::repl::execute` to accept pre-initialized dependencies:
  - `config: &CliConfig`
  - `store: Arc<dyn KilnStore>`
  - `llm: Arc<dyn TextGenerationProvider>`
- Remove duplicate config loading and global state access from REPL modules
- Add unit tests for REPL parsing edge cases (whitespace, quoted args, history limits) using mocks
- Cover REPL workflows with integration tests

**COMPLETION CRITERIA:**
- [ ] REPL takes dependencies as arguments (no global state)
- [ ] REPL unit tests use `InMemoryKilnStore` and mock LLM
- [ ] REPL parsing edge cases have tests: `cargo test -p crucible-cli repl::parse`
- [ ] All REPL tests pass

---

## Phase 10 – Test Coverage Audit

**WHY:** Ensure we have balanced unit + integration coverage as requested

**WHAT:**
- Generate coverage report: `cargo tarpaulin --workspace --out Html`
- Identify gaps in critical paths:
  - Semantic search query execution
  - Kiln metadata indexing
  - REPL command dispatch
  - Config loading and validation
- Add missing unit tests (using mocks) for uncovered branches
- Add missing integration tests (using real dependencies) for end-to-end workflows
- Document test strategy in `docs/testing.md`

**COMPLETION CRITERIA:**
- [ ] Coverage report shows >70% line coverage on core modules
- [ ] At least 5 new unit tests added for critical paths
- [ ] At least 3 new integration tests for end-to-end workflows
- [ ] `docs/testing.md` documents where unit vs integration tests live

---

## Phase 11 – Documentation Refresh

**WHY:** Docs should reflect simplified architecture, not over-engineered plans

**WHAT:**
- Update README.md: describe current architecture (CLI → core domain → trait-based DB/LLM)
- Update CLAUDE.md: remove references to removed components (services, Rune tools), add trait boundaries
- Update CONTRIBUTING.md: document test strategy (unit with mocks, integration with real deps)
- Write `docs/testing.md`: explain fixture usage, when to use mocks vs real deps, how to run tests
- Add "Future Work" section in PERSONAL.md for potential enhancements (desktop app, p2p sync)

**COMPLETION CRITERIA:**
- [ ] README.md reflects simplified architecture
- [ ] CLAUDE.md updated with trait boundaries
- [ ] `docs/testing.md` documents test strategy with examples
- [ ] CONTRIBUTING.md explains how to add tests

---

## Phase 12 – Final Sanity

**WHY:** Verify the refactor is complete and stable

**WHAT:**
- Run `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings`
- Run `cargo test --workspace` 5 times to verify stability
- Confirm no references to deleted code (services, Phase 8 harnesses, global tool helpers)
- Update ROADMAP.md with completion dates
- Archive this roadmap in `docs/roadmap/2025-refactor-completed.md`

**COMPLETION CRITERIA:**
- [ ] Zero clippy warnings
- [ ] Tests pass 5/5 runs
- [ ] No TODO comments referencing removed architecture
- [ ] ROADMAP.md archived with completion date

---

## Future Work (Not in Scope)

These are explicitly deferred until MVP is stable:

- **Desktop app integration** - Design for CLI first; refactor when we actually build UI
- **Multi-device sync** - CRDT infrastructure exists but sync transport isn't critical yet
- **Multi-user collaboration** - Permissions and session management are future enhancements
- **Rune tool system** - Removed in Phase 3; can be re-added later if needed
- **Advanced agent orchestration** - Focus on basic LLM integration first

**Decision point:** Revisit this list after Phase 12 is complete and evaluate what's actually needed.

---

## How This Supports the Goal

> "Local-first knowledge management means the CLI and future desktop app must share a simple, testable core"

**This roadmap achieves that by:**

1. **Simple**: Remove abstractions that don't solve real problems (no façades, no service layers, no builder patterns)
2. **Testable**: Add trait boundaries ONLY where we need mocking (DB, LLM, Embeddings); use fixtures for shared setup
3. **Shared core**: `crucible-core` contains domain logic; CLI and future desktop app both import it directly
4. **Stable**: Fix flakiness first (Phases 2-7); only then do cleanup (Phases 8-12)

**Key differences from old roadmap:**
- Removed CommandService, ToolManager, Core façades, CliApp builder - these were premature abstractions
- Added focused phases for trait boundaries (Phases 4-6) - these solve real testing problems
- Added explicit flaky test fixing phase (Phase 7) - this is the actual pain point
- Removed sync/collaboration (Phase 11 old) - defer until MVP is stable
- Added Anti-Goals section to prevent scope creep

This is a **pragmatic, test-driven refactor** focused on making the codebase work reliably, not architecting for imagined future requirements.
