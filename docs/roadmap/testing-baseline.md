# Crucible Testing Baseline

**Date Created**: 2025-10-30
**Date Updated**: 2025-11-01
**Purpose**: Establish measurable starting point for roadmap refactor

---

## Test Suite Statistics

### Overall Metrics
- **Total test files**: 44 files across 8 crates
- **Total async tests**: 309 tests
- **Ignored tests**: 16+ tests (require external services)
- **Test execution time**: Variable (1-5 minutes depending on environment)

### Tests by Crate

| Crate | Test Files | Status |
|-------|-----------|--------|
| `crucible-config` | 1 | Stable unit tests |
| `crucible-llm` | 7 | Mix of unit + ignored integration tests |
| `crucible-tools` | 1 | Unit tests for parsing |
| `crucible-core` | 2 | Unit tests for agent/task routing |
| `crucible-rune-macros` | 1 | Macro expansion tests |
| `crucible-surrealdb` | 22 | Integration tests with timing issues |
| `crucible-cli` | 9 | Integration tests with external deps |
| `crucible-watch` | 1 | File watcher tests (timing-dependent) |

---

## Build Status

**Command**: `cargo test --workspace --no-run`

**Outcome**:
- Build succeeded, but emitted extensive warnings (primarily from `crucible-surrealdb`)
- Common warning themes:
  - Unused imports and variables
  - Dead code in embedding/kiln helpers
  - Redundant comparisons (`>= 0` on unsigned durations)
- No failing crates; integration binaries compiled without running

**Notes**:
- Capture representative warning examples (e.g., `crates/crucible-surrealdb/src/kiln_integration.rs` unused `RelationalDB` import) for later cleanup
- Future phases should drive warning count down as modules are simplified or retired
- When re-running this baseline later, compare warning volume to confirm improvements

---

## Known Flaky Test Patterns

### 1. Timing-Dependent Tests (HIGH PRIORITY)

**Problem**: Tests use `tokio::time::sleep()` to wait for async operations

**Affected Files**:
- `/crates/crucible-surrealdb/tests/vector_similarity_tests.rs`
  - 14 sleep calls (50ms-500ms)
  - Timing assertions that can fail on slow CI: `assert!(total_time < Duration::from_secs(30))`

- `/crates/crucible-surrealdb/tests/kiln_embedding_pipeline_tests.rs`
  - Lines 420-428: Hard-coded time limits
  - `assert!(avg_time_per_doc < Duration::from_secs(5))` - fails on slow machines

- `/crates/crucible-cli/tests/file_watcher_tests.rs`
  - Lines 111, 164, 258, 370: Sleep-based event synchronization
  - 3-second sleep at line 370 (excessive)

- `/crates/crucible-watch/tests/file_watch_integration.rs`
  - Comment at line 1016: "This is timing-dependent"

**Impact**: Tests pass locally but fail on CI, or vice versa

**Fix Strategy**: Replace sleeps with event-driven synchronization (channels, completion signals)

---

### 2. External Service Dependencies (MEDIUM PRIORITY)

**Problem**: Tests call real HTTP endpoints (Ollama, OpenAI, embedding services)

**Ignored Tests** (always skip in CI):

- `/crates/crucible-cli/tests/test_backend.rs`
  - 5 tests `#[ignore]` - require Ollama running on localhost:11434
  - Tests: `test_list_models`, `test_chat`, `test_chat_with_params`, etc.

- `/crates/crucible-llm/src/embeddings/openai.rs`
  - 4 tests `#[ignore]` - require OpenAI API key
  - Lines: 442, 461, 488, 502

- `/crates/crucible-llm/src/embeddings/ollama.rs`
  - 4 tests `#[ignore]` - require Ollama running
  - Lines: 436, 451, 475, 559

- `/crates/crucible-llm/src/reranking/fastembed.rs`
  - 2 tests `#[ignore]` - require network access

- `/crates/crucible-cli/tests/file_watcher_tests.rs:347`
  - 1 test `#[ignore]` - requires embedding service

**Impact**: Reduced test coverage in CI; manual testing required for these paths

**Fix Strategy**: Add trait-based mocks so these code paths have CI coverage

---

### 3. Database Lock Poisoning Risk (HIGH PRIORITY)

**Problem**: `Arc<Mutex<HashMap>>` with `.expect()` pattern can cause cascading failures

**Affected Code**: `/crates/crucible-surrealdb/src/database.rs`

**Pattern** (lines 88-90, 99-101, 113-116, etc.):
```rust
let mut storage = self.storage
    .lock()
    .expect("Storage lock poisoned - kiln database is in inconsistent state");
```

**Risk** (documented in `database_concurrency_tests.rs:3-11`):
- If a thread panics while holding lock, all future `.unwrap()` calls panic
- Deadlock risk with multiple lock acquisitions
- Race conditions on concurrent writes (see `kiln_integration.rs:279` comment)

**Current Mitigation**: 27 concurrency tests with timeouts in `database_concurrency_tests.rs`

**Fix Strategy**:
- Replace `.expect()` with proper error propagation
- Add `KilnStore` trait for test mocking
- Use `RwLock` for read-heavy workloads

---

### 4. File System Race Conditions (LOW PRIORITY)

**Problem**: Tests modify files and immediately assert events received

**Affected Files**:
- `/crates/crucible-cli/tests/file_watcher_tests.rs`
- `/crates/crucible-cli/tests/filesystem_edge_case_tdd.rs:436`
- `/crates/crucible-watch/tests/file_watch_integration.rs`

**Pattern**:
1. Create/modify file
2. Sleep 100-300ms
3. Assert event received
4. Fails if FS is slow or system under load

**Good Practice**: Tests use `TempDir` for isolation ✅

**Fix Strategy**: Add retry logic for event assertions, use event counters instead of timing

---

### 5. Broken Tests (Removed APIs)

**To Delete/Rewrite**:

- `/crates/crucible-llm/tests/candle_factory_integration_tests.rs:141`
  - Uses removed `from_env()` API
  - Action: Delete or rewrite with new config system

- `/crates/crucible-llm/tests/archived-mock/embedding_storage_tests.rs`
  - Already in archived folder, obsolete
  - Action: Delete

---

## Existing Test Infrastructure (GOOD)

### What's Already Working Well

1. **EmbeddingProvider Trait** ✅
   - Location: `/crates/crucible-llm/src/embeddings/provider.rs:611`
   - Excellent mock: `MockEmbeddingProvider` (577 lines in `mock.rs`)
   - Multiple implementations: `FastEmbedProvider`, `OllamaProvider`, `OpenAIProvider`, `CandleProvider`
   - Used extensively in tests

2. **Test Isolation** ✅
   - Tests use `TempDir` for filesystem isolation
   - No `serial_test` usage - tests run in parallel
   - Each test creates isolated temp directories

3. **Concurrency Testing** ✅
   - `database_concurrency_tests.rs` has 27 tests with proper timeouts
   - Tests validate thread safety and lock behavior

4. **Good Test Fixtures** ✅
   - `EmbeddingFixtures` in `mock.rs`
   - `create_basic_kiln()` in `crucible-core/test_support`

---

## Test Quality Issues

### Excessive `.unwrap()` Usage
- **422 unwraps** in test files
- Pattern: `result.await.unwrap()` everywhere
- Risk: Panics hide root cause, poor error messages
- Recommendation: Use `Result<()>` and `?` operator

### No Shared Test Fixtures
- Most tests inline all setup
- Duplication across similar tests
- Exception: `EmbeddingFixtures` (excellent example to follow)

### Missing Cleanup Logic
- Some tests leave temp files (rare)
- Most use `TempDir` correctly ✅
- Database state not cleaned between tests (in-memory is acceptable)

---

## Trait Boundaries Assessment

### Existing Traits (NO CHANGES NEEDED)

1. **EmbeddingProvider** ✅
   - Well-designed, minimal interface
   - Multiple production implementations
   - Excellent mock infrastructure

2. **TextGenerationProvider** (LLM) ✅
   - Located in `/crates/crucible-llm/src/text_generation.rs`
   - Already supports mocking

3. **Backend** ✅
   - Located in `/crates/crucible-cli/src/agents/backend/mod.rs:42`
   - Interface for LLM backends (Ollama, etc.)

4. **SearchBackend** ✅
   - Located in `/crates/crucible-cli/src/commands/search.rs:58`
   - Already has mock implementation

### Missing Traits (NEED TO ADD)

1. **KilnStore** (Database) ❌
   - Current: `SurrealEmbeddingDatabase` is concrete struct
   - Problem: Can't mock database in tests
   - Impact: Database tests have timing dependencies and shared state
   - **Priority: HIGH** - This is the main missing abstraction

2. **FileWatcher** (Partial) ⚠️
   - Trait exists: `/crates/crucible-watch/src/backends/mod.rs:14`
   - Gap: Tests don't use mock implementations
   - **Priority: MEDIUM**

---

## Current CLI Boot Flow

**Documented** in README.md (as of 2025-11-01):

```
1. Parse args with clap → cli::Cli
2. Load config → CliConfig::load()
3. Match command:
   - Search → commands::search::execute()
   - Semantic → commands::semantic::execute()
   - Fuzzy → commands::fuzzy::execute()
   - Note → commands::note::execute()
   - Config → commands::config::execute()
   - Repl → commands::repl::execute()
   - Stats → commands::stats::execute()
4. Commands call domain logic directly
5. Presentation layer (println!) in command modules
```

**No service layers, no global app state (except config)**

---

## Completion Criteria for Phase 0

- [x] Test count documented (309 async tests, 44 files)
- [x] Flaky test patterns identified (timing, external services, lock poisoning, FS races)
- [x] Existing good patterns documented (EmbeddingProvider, TempDir usage)
- [x] Missing abstractions identified (KilnStore trait)
- [ ] README.md documents CLI boot flow (ACTION ITEM)
- [ ] PERSONAL.md checklist created (ACTION ITEM)

---

## Next Steps (Phase 1+)

1. **Phase 1**: Remove dead code (`archived-mock/`, broken tests)
2. **Phase 2**: Consolidate test fixtures in `crucible_core::test_support`
3. **Phase 3**: Remove Rune tool system (if user confirms)
4. **Phase 4**: Add `KilnStore` trait (highest impact for flakiness)
5. **Phase 7**: Fix flaky tests using new trait boundaries

**Success Metric**: `cargo test --workspace` runs 3x without failures
