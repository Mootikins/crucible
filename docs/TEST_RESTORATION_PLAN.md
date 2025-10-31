> **Note:** The `crucible-daemon` crate has been removed; references in this document remain for historical context.

# Test Restoration Plan - Post Phase 2

**Status:** Planning Document
**Created:** 2025-10-26
**Purpose:** Prioritize which archived tests to restore after config consolidation
**Prerequisites:** Phase 2 (config consolidation) must be complete

---

## Executive Summary

During Phase 1 test cleanup, we archived **38+ test files** (combined from multiple cleanup sessions) that depended on removed architecture. Not all need to be restored - some were redundant, some test obsolete features, and some are already covered by other tests.

This document categorizes archived tests by:
1. **Priority** (Critical → Low → Skip)
2. **Restoration Effort** (Simple → Complex)
3. **Coverage Status** (Unique → Redundant)

---

## Test Inventory & Analysis

### HIGH PRIORITY - Core Functionality (Restore First)

#### 1. Embedding Pipeline Tests ⭐⭐⭐
**Files:**
- `embedding_pipeline.rs` - Core embedding pipeline
- `batch_embedding.rs` - Batch operations
- `re_embedding.rs` - Re-embedding logic
- `embedding_storage_retrieval_tests.rs` - Database integration

**Why Critical:**
- Tests core value proposition (semantic search via embeddings)
- No other tests cover embedding pipeline end-to-end
- Users depend on this working correctly

**Restoration Approach:**
```rust
// New simplified test structure
#[tokio::test]
async fn test_embedding_pipeline_basic() {
    // Use crucible-config::Config (simplified)
    let config = Config::test_default();

    // Use crucible-watch directly (no daemon layer)
    let watcher = WatchManager::new(config.watch_config())?;

    // Use crucible-surrealdb::EmbeddingPipeline directly
    let pipeline = EmbeddingPipeline::new(config.embedding_provider())?;

    // Test: File event → Embedding → Storage
    let file_event = create_test_file_event();
    let embeddings = pipeline.process_event(file_event).await?;
    assert!(embeddings.len() > 0);
}
```

**Estimated Effort:** 4-6 hours
**Dependencies:** Phase 2 EmbeddingConfig consolidation complete

---

#### 2. Semantic Search Integration Tests ⭐⭐⭐
**Files:**
- `semantic_search.rs` - Search functionality
- `semantic_search_real_integration_tdd.rs` (not archived, but may need updates)

**Why Critical:**
- Core user-facing feature
- Tests actual search quality, not just mechanics
- Validates embeddings are useful

**Restoration Approach:**
```rust
#[tokio::test]
async fn test_semantic_search_quality() {
    // Simplified setup
    let config = Config::test_default();
    let search = SemanticSearch::new(config)?;

    // Index test corpus
    search.index_directory("tests/fixtures/semantic-corpus").await?;

    // Test search quality
    let results = search.query("rust async programming").await?;
    assert!(results.iter().any(|r| r.path.contains("async-rust.md")));
    assert!(results[0].similarity > 0.7);
}
```

**Estimated Effort:** 3-4 hours
**Dependencies:** Embedding pipeline tests restored first

---

#### 3. Event Pipeline Integration ⭐⭐
**Files:**
- `event_pipeline_integration.rs` - File events → Processing
- `watcher_integration_tests.rs` - Watcher → Events

**Why Important:**
- Tests integration between crucible-watch and processing
- Covers error handling and event routing
- Real-world usage patterns

**Restoration Approach:**
```rust
#[tokio::test]
async fn test_file_change_triggers_embedding() {
    let temp_dir = TempDir::new()?;
    let config = Config::test_default_with_path(temp_dir.path());

    let daemon = Daemon::new(config).await?;
    daemon.start().await?;

    // Create/modify file
    fs::write(temp_dir.path().join("test.md"), "# Test")?;

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify embedding was created
    let db = daemon.database();
    let embeddings = db.get_embeddings_for_file("test.md").await?;
    assert!(embeddings.len() > 0);
}
```

**Estimated Effort:** 4-5 hours
**Dependencies:** Simplified DaemonConfig complete

---

### MEDIUM PRIORITY - Quality Assurance

#### 4. Provider-Specific Tests ⭐
**Files:**
- `embedding_mock_provider_tests.rs` - Mock provider
- `embedding_real_provider_tests.rs` - Real providers (Ollama, OpenAI)
- `embedding_content_type_tests.rs` - Different content types

**Why Important:**
- Provider-specific behavior needs validation
- Mock tests enable fast CI/CD
- Content type handling is error-prone

**Current Coverage:**
- `crucible-llm/tests/mock_embedding_provider_tests.rs` - EXISTS ✅
- May already have provider tests at LLM layer

**Action:**
1. Audit existing crucible-llm tests
2. If gaps exist, restore and simplify
3. If well-covered, skip restoration

**Estimated Effort:** 2-3 hours (if restoration needed)

---

#### 5. Error Recovery & Edge Cases ⭐
**Files:**
- `error_recovery_integration.rs` - Error handling
- `error_recovery_tdd.rs` - Error recovery patterns

**Why Important:**
- Real-world reliability depends on error handling
- Edge cases often reveal bugs

**Current Coverage:**
- `crucible-cli/tests/error_recovery_*.rs` may still exist
- Check if functionality is covered

**Action:**
1. Review remaining error recovery tests
2. Identify gaps
3. Restore specific test cases, not full files

**Estimated Effort:** 3-4 hours

---

### LOW PRIORITY - Nice to Have

#### 6. Performance & Load Tests
**Files:**
- `performance_load_tests.rs` - Load testing

**Why Low Priority:**
- Not blocking for functionality
- Can be done manually or with benchmarks
- Requires test corpus setup

**Action:**
- Defer until post-1.0
- Consider using `cargo bench` instead of integration tests

**Estimated Effort:** 4-6 hours

---

#### 7. Configuration Tests
**Files:**
- `configuration_integration_test.rs` - Config loading
- `migration_management_tests.rs` - Config migration

**Why Low Priority:**
- crucible-config crate has own tests
- Config consolidation will change these anyway

**Action:**
- Let crucible-config tests handle this
- Add integration tests only if gaps found

**Estimated Effort:** 2-3 hours

---

### SKIP - Obsolete or Redundant

#### 8. Service Architecture Tests ❌
**Files:**
- `tool_registry.rs` (20k lines!)
- `daemon_event_integration_tests.rs`
- `kiln_integration_tests.rs`

**Why Skip:**
- Tested removed `crucible_services` architecture
- 20k lines of test code for obsolete features
- New architecture doesn't have this complexity

**Action:** Leave archived for reference only

---

#### 9. REPL Tool Tests (Moved to CLI) ❌
**Files:**
- `repl_direct_integration_tests.rs`
- `repl_error_handling_*.rs`
- `repl_process_integration_tests.rs`
- `repl_tool_execution_tests.rs`
- `repl_unified_*.rs`
- `repl_unit_tests.rs`

**Why Skip:**
- REPL is CLI concern, not daemon
- CLI already has `repl_tool_integration_tests.rs` (we fixed this)
- Redundant with existing CLI tests

**Action:** Leave archived, use CLI test suite

---

#### 10. Deprecated Tests ❌
**Files:**
- `binary_safety_tdd.rs` - Binary detection (specific feature)
- `test_chat.rs` - Chat interface (moved/changed?)
- `kiln_processing_integration_tdd.rs` - Kiln terminology (deprecated)

**Action:** Review if features still exist, otherwise skip

---

## Restoration Order & Timeline

### Phase 2A: Immediate (Week 1 post-consolidation)
**Goal:** Core embedding functionality works

1. **Embedding Pipeline** (4-6 hours)
   - Restore: `embedding_pipeline.rs`, `batch_embedding.rs`
   - Simplify using new config structure
   - Focus on happy path first

2. **Semantic Search** (3-4 hours)
   - Restore: `semantic_search.rs`
   - Validate search quality
   - Add regression tests

**Deliverable:** Users can index files and search semantically

---

### Phase 2B: Integration (Week 2)
**Goal:** End-to-end workflow works reliably

3. **Event Pipeline** (4-5 hours)
   - Restore: `event_pipeline_integration.rs`
   - Test watcher → embeddings → storage
   - Add timeout/error handling tests

4. **Storage & Retrieval** (3-4 hours)
   - Restore: `embedding_storage_retrieval_tests.rs`
   - Test CRUD operations on embeddings
   - Verify database schema

**Deliverable:** File changes automatically trigger embeddings

---

### Phase 2C: Quality (Week 3)
**Goal:** Robust error handling and edge cases

5. **Provider Tests** (2-3 hours)
   - Audit existing LLM tests
   - Fill gaps if needed
   - Add content type edge cases

6. **Error Recovery** (3-4 hours)
   - Restore specific error scenarios
   - Add timeout handling tests
   - Test rate limiting

**Deliverable:** System handles errors gracefully

---

### Phase 3: Optional Enhancements
**Goal:** Performance and polish

7. **Performance Tests** (optional, 4-6 hours)
8. **Re-embedding** (optional, 2-3 hours)

---

## New Test Structure Pattern

### Old Pattern (Avoid)
```rust
// ❌ Old: Heavy test harness, service mocks, complex setup
let harness = DaemonEmbeddingHarness::new(
    DaemonEmbeddingConfig::with_all_features()
).await?;

harness.start_services().await?;
harness.register_event_handlers().await?;
harness.setup_mock_providers().await?;

// 50+ lines of setup...
```

### New Pattern (Use)
```rust
// ✅ New: Direct usage, minimal setup, clear intent
#[tokio::test]
async fn test_feature_name() {
    // Arrange: Simple config
    let config = Config::test_default();
    let component = Component::new(config)?;

    // Act: Test one thing
    let result = component.do_thing().await?;

    // Assert: Clear expectations
    assert_eq!(result, expected);
}
```

**Key Principles:**
1. **Direct API usage** - No test harness layer
2. **Single responsibility** - One test, one concept
3. **Fast setup** - TempDir, in-memory DB, mock providers
4. **Clear names** - `test_<feature>_<scenario>_<expected>`

---

## Testing Strategy Post-Restoration

### Unit Tests (Fast, Many)
**Location:** `crates/*/src/**/*.rs` (inline `#[cfg(test)]` modules)
**Coverage:** Individual functions, edge cases, error conditions
**Example:**
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_frontmatter_empty() { ... }

    #[test]
    fn test_parse_frontmatter_invalid_yaml() { ... }
}
```

### Integration Tests (Slower, Focused)
**Location:** `crates/*/tests/*.rs`
**Coverage:** Component interactions, API contracts, persistence
**Example:**
```rust
// crates/crucible-surrealdb/tests/embedding_pipeline.rs
#[tokio::test]
async fn test_embedding_pipeline_end_to_end() {
    // Tests: File → Parser → Embedder → Database
}
```

### End-to-End Tests (Slowest, Critical Path Only)
**Location:** `crates/crucible-daemon (removed)/tests/*.rs`
**Coverage:** Full system, real files, real database
**Example:**
```rust
// crates/crucible-daemon (removed)/tests/e2e_daemon.rs
#[tokio::test]
async fn test_daemon_full_workflow() {
    // Tests: Start daemon → Add file → Search → Verify results
}
```

---

## Coverage Goals

### Must Have (Critical Path)
- ✅ Embedding generation (basic)
- ✅ Embedding storage
- ✅ Semantic search (basic)
- ✅ File watcher integration
- ✅ Config loading

### Should Have (Quality)
- ⚠️ Provider-specific behavior
- ⚠️ Error recovery
- ⚠️ Content type handling
- ⚠️ Batch operations
- ⚠️ Re-embedding

### Nice to Have (Polish)
- ❓ Performance benchmarks
- ❓ Load testing
- ❓ Migration testing

---

## Test Corpus & Fixtures

### Current Fixtures
```
tests/fixtures/
├── semantic-corpus/      # For search quality tests
├── examples/test-kiln/           # For kiln processing tests
└── examples/test-kiln/           # For kiln terminology tests
```

### New Fixtures Needed
```
tests/fixtures/
├── embedding-test-docs/
│   ├── markdown/        # Various markdown files
│   ├── code/           # Code samples
│   └── mixed/          # Mixed content
├── search-corpus/       # Curated for search quality
│   ├── rust-async.md
│   ├── tokio-tutorial.md
│   └── README.md
└── edge-cases/          # Edge case documents
    ├── empty.md
    ├── huge-file.md (1MB+)
    ├── binary.pdf
    └── malformed.md
```

---

## Success Metrics

### Phase 2A Success (Week 1)
- [ ] Embedding pipeline test passes
- [ ] Batch embedding test passes
- [ ] Basic semantic search test passes
- [ ] CI pipeline green

### Phase 2B Success (Week 2)
- [ ] Event pipeline test passes
- [ ] Storage/retrieval test passes
- [ ] Watcher integration test passes
- [ ] End-to-end test passes

### Phase 2C Success (Week 3)
- [ ] Error recovery tests pass
- [ ] Content type tests pass
- [ ] Provider tests pass
- [ ] All critical paths covered

### Overall Success
- [ ] Core functionality: 100% coverage
- [ ] Error handling: 80%+ coverage
- [ ] Edge cases: 60%+ coverage
- [ ] CI time: < 5 minutes
- [ ] Test maintainability: High (no complex harnesses)

---

## Migration Checklist

For each restored test file:

- [ ] Read old test file
- [ ] Identify what it was testing (core concept)
- [ ] Check if already covered by other tests
- [ ] If not covered, design new simplified test
- [ ] Write new test using new pattern
- [ ] Verify test passes
- [ ] Add to CI pipeline
- [ ] Document in test plan
- [ ] Delete old test file from archive (or mark as "restored")

---

## Open Questions

1. **Test Data Strategy**
   - Use real files or generated fixtures?
   - How to maintain search quality corpus?

2. **Database Strategy**
   - In-memory for tests or temp files?
   - How to test migrations?

3. **Provider Strategy**
   - Always use mock providers in CI?
   - Optional real provider tests?

4. **CI Strategy**
   - Run all tests always, or split fast/slow?
   - Parallel test execution?

---

## References

- Archived tests: `/home/moot/crucible/tests/archive/broken_tests_2025_10_26/`
- Config consolidation plan: `/home/moot/crucible/docs/CONFIG_CONSOLIDATION_PLAN.md`
- Current test status: Run `cargo test --workspace -- --list`
- Coverage report: Run `cargo tarpaulin` (if installed)

---

## Appendix: Archived Test Categorization

### By Functionality
**Embedding:** 8 files (HIGH PRIORITY)
**Search:** 2 files (HIGH PRIORITY)
**Event Pipeline:** 2 files (HIGH PRIORITY)
**REPL:** 10 files (SKIP - CLI tests exist)
**Services:** 3 files (SKIP - architecture removed)
**Config:** 2 files (LOW PRIORITY - library tests cover)
**Other:** 11 files (EVALUATE case-by-case)

### By Complexity
**Simple (< 100 LOC):** 5 files - Easy to restore
**Medium (100-500 LOC):** 18 files - Moderate effort
**Complex (> 500 LOC):** 15 files - High effort or skip

### By Dependencies
**No external deps:** 8 files - Fast tests
**Requires DB:** 12 files - Moderate speed
**Requires providers:** 6 files - Slow tests (mock them)
**Requires services:** 12 files - SKIP (services removed)

---

**End of Test Restoration Plan**

This document will be updated as tests are restored with actual restoration notes, issues encountered, and final test coverage metrics.
