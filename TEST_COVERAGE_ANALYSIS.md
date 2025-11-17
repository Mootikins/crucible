# Test Coverage Analysis for Crucible CLI MVP

**Analysis Date:** 2025-11-17
**Branch:** claude/fix-integration-tests-01GTvPtBK1n8kNbk91sYwniy
**Goal:** Increase coverage and build confidence for an initial CLI MVP

## Executive Summary

### Current State
- **Unit Tests:** 188 tests across 31 files (inline `#[test]` blocks)
- **Integration Tests:** 3 major integration test files for core libraries
- **CLI Integration Tests:** **NONE** ❌ (Critical gap for MVP)
- **End-to-End Tests:** **NONE** ❌ (Critical gap for MVP)

### Critical Findings
1. ⚠️ **No CLI command integration tests** - Zero tests for the actual CLI interface
2. ⚠️ **No tests directory** - `/home/user/crucible/crates/crucible-cli/tests/` does not exist
3. ⚠️ **No REPL testing** - Interactive REPL has no automated tests
4. ⚠️ **No end-to-end workflow tests** - User workflows untested
5. ✅ **Good library coverage** - Core libraries (pipeline, merkle, parser) have solid tests

---

## Detailed Analysis

### ✅ What's Well Tested

#### 1. Pipeline Integration (`crucible-pipeline/tests/pipeline_integration_tests.rs`)
**Coverage:** Excellent (758 lines)
- ✅ Full pipeline flow through 5 phases
- ✅ Error handling at each phase
- ✅ Mock implementations for all dependencies
- ✅ Edge cases (unchanged files, force reprocess, skip enrichment)
- ✅ Content change detection
- ✅ Merkle diff validation

**Tests:**
- `test_full_pipeline_with_embeddings`
- `test_pipeline_skip_unchanged_files`
- `test_pipeline_force_reprocess`
- `test_pipeline_skip_enrichment_mode`
- `test_pipeline_parse_error_handling`
- `test_pipeline_enrichment_error_handling`
- `test_pipeline_storage_error_handling`
- `test_pipeline_detects_content_changes`
- `test_pipeline_no_changes_after_merkle_diff`

#### 2. Property Storage Integration (`crucible-surrealdb/tests/property_storage_integration_tests.rs`)
**Coverage:** Good
- ✅ End-to-end frontmatter pipeline
- ✅ PropertyStorage trait implementation
- ✅ Batch operations
- ✅ Namespace filtering
- ✅ Type preservation (text, number, bool, date, JSON)

#### 3. Merkle Tree Integration (`crucible-surrealdb/tests/merkle_integration_tests.rs`)
**Coverage:** Good
- ✅ Document parsing → tree building → persistence
- ✅ Large document virtualization
- ✅ Incremental updates
- ✅ Tree retrieval and verification

#### 4. Parser Tests
**Location:** `crucible-parser/tests/`
- ✅ Frontmatter types (YAML, TOML)
- ✅ Blockquote parsing
- ✅ Table parsing
- ✅ Horizontal rules
- ✅ Metadata extraction
- ✅ Heading hierarchy
- ✅ Debug blocks

#### 5. Unit Tests in CLI Source
**Coverage:** 188 inline tests across 31 files
- ✅ `search.rs` - SearchExecutor with mock backend (2 tests)
- ✅ `config.rs` - Configuration logic (19 tests)
- ✅ `interactive.rs` - FuzzyPicker logic (9 tests)
- ✅ `output.rs` - Formatting utilities (11 tests)
- ✅ REPL components (highlighter, formatter, completer, etc.)

---

### ❌ Critical Gaps for CLI MVP

#### 1. **No CLI Command Integration Tests** (HIGHEST PRIORITY)
**Missing:** `/home/user/crucible/crates/crucible-cli/tests/` directory doesn't exist

**Commands Without Integration Tests:**
- ❌ `cru search` - No end-to-end test
- ❌ `cru fuzzy` - No interactive test
- ❌ `cru stats` - No command execution test
- ❌ `cru config init` - No file creation test
- ❌ `cru config show` - No output validation
- ❌ `cru diff` - No comparison test
- ❌ `cru status` - No status display test
- ❌ `cru storage` subcommands - No test coverage
- ❌ `cru parse` - No parsing command test
- ❌ Default REPL mode - No test

**Impact:** Can't verify CLI works as expected for users

**Recommendation:** Create integration tests using:
- `assert_cmd` crate (already in dev-dependencies ✅)
- `predicates` crate (already in dev-dependencies ✅)
- `tempfile` for test fixtures (already in dev-dependencies ✅)

#### 2. **No REPL Integration Tests**
**File:** `crates/crucible-cli/src/commands/repl/mod.rs` (627 lines)

**Missing Tests:**
- ❌ REPL startup and initialization
- ❌ Command parsing (`:stats`, `:help`, `:quit`, etc.)
- ❌ SurrealQL query execution
- ❌ Tool execution (`:run`, `:tools`)
- ❌ History navigation
- ❌ Tab completion
- ❌ Syntax highlighting
- ❌ Error recovery
- ❌ Non-interactive mode (`--non-interactive` flag)

**Exists:** `TESTING.md` documentation but no actual tests

**Impact:** REPL is a core feature but untested in integration

#### 3. **No End-to-End Workflow Tests**
**Missing User Workflows:**
- ❌ New user setup: `config init` → `stats` → `search`
- ❌ Search workflow: query → results → selection
- ❌ Configuration workflow: init → show → modify → verify
- ❌ Error handling: invalid config → helpful error message
- ❌ File not found scenarios
- ❌ Database corruption recovery
- ❌ Concurrent access scenarios

#### 4. **Missing Error Path Coverage**
**Commands Without Error Tests:**
- ❌ Invalid kiln path handling
- ❌ Missing database file
- ❌ Corrupted config file
- ❌ Permission errors
- ❌ Disk full scenarios
- ❌ Network timeout (for future embedding service)
- ❌ Invalid command arguments
- ❌ Binary file handling in search

#### 5. **No Performance/Load Tests**
**Missing:**
- ❌ Large kiln handling (10,000+ files)
- ❌ Search performance benchmarks
- ❌ Memory usage under load
- ❌ REPL responsiveness with large results
- ❌ Fuzzy search with many matches

#### 6. **No Cross-Platform Tests**
**Current:** Tests run on Linux only
**Missing:**
- ❌ Windows path handling
- ❌ macOS-specific behaviors
- ❌ Unicode path handling
- ❌ Case-insensitive filesystem handling

---

## Specific Test Errors and Issues

### Issue #1: No CLI Binary Tests
**File:** `crates/crucible-cli/Cargo.toml`
```toml
[dev-dependencies]
assert_cmd = { workspace = true }  # ✅ Available
predicates = { workspace = true }   # ✅ Available
```

**Problem:** Dependencies exist but no tests using them

**Example Missing Test:**
```rust
use assert_cmd::Command;

#[test]
fn test_cli_stats_command() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("stats")
        .assert()
        .success();
}
```

### Issue #2: SearchExecutor Has Only Mock Tests
**File:** `crates/crucible-cli/src/commands/search.rs:492-511`

**Current:**
- ✅ 2 unit tests with `MockSearchBackend`
- ❌ No integration test with real filesystem
- ❌ No binary file detection test
- ❌ No Unicode handling test

**Gap:** Mock tests don't catch real filesystem issues

### Issue #3: KilnStats Has No Tests
**File:** `crates/crucible-cli/src/commands/stats.rs`

**Current:**
- ✅ Trait abstraction exists (`KilnStatsService`)
- ✅ `execute_with_service` for dependency injection
- ❌ No tests at all (0 tests)

**Impact:** Stats command completely untested

### Issue #4: Config Command Partially Tested
**File:** `crates/crucible-cli/src/commands/config.rs`

**Current:**
- ✅ 19 unit tests for config loading/merging
- ❌ No test for `config init` file creation
- ❌ No test for `config show` output
- ❌ No test for invalid TOML handling

### Issue #5: REPL Components Tested Individually, Not Integrated
**Files:**
- `repl/completer.rs` - 7 tests ✅
- `repl/formatter.rs` - 5 tests ✅
- `repl/highlighter.rs` - 5 tests ✅
- `repl/history.rs` - 7 tests ✅
- `repl/command.rs` - 8 tests ✅
- `repl/mod.rs` - **0 integration tests** ❌

**Problem:** Components tested in isolation, not as a system

---

## Recommendations for MVP Confidence

### Priority 1: CLI Command Integration Tests (CRITICAL)
**Timeline:** Immediate
**Effort:** 2-3 days

**Create:** `/home/user/crucible/crates/crucible-cli/tests/cli_integration_tests.rs`

```rust
// Essential MVP tests
#[test] fn test_cli_help()
#[test] fn test_cli_stats_on_empty_kiln()
#[test] fn test_cli_stats_with_files()
#[test] fn test_cli_config_init()
#[test] fn test_cli_config_show()
#[test] fn test_cli_search_no_results()
#[test] fn test_cli_search_with_match()
#[test] fn test_cli_parse_markdown_file()
#[test] fn test_cli_invalid_command()
#[test] fn test_cli_missing_kiln_path()
```

### Priority 2: Stats Command Tests (HIGH)
**Timeline:** 1 day
**Effort:** Low

**Create:** Tests for `stats.rs`

```rust
#[test] fn test_stats_empty_directory()
#[test] fn test_stats_counts_markdown()
#[test] fn test_stats_calculates_size()
#[test] fn test_stats_recursive_subdirs()
#[test] fn test_stats_ignores_hidden_files()
```

### Priority 3: REPL Integration Tests (HIGH)
**Timeline:** 2-3 days
**Effort:** Medium

**Create:** `/home/user/crucible/crates/crucible-cli/tests/repl_integration_tests.rs`

```rust
#[tokio::test] async fn test_repl_basic_query()
#[tokio::test] async fn test_repl_help_command()
#[tokio::test] async fn test_repl_stats_command()
#[tokio::test] async fn test_repl_quit_command()
#[tokio::test] async fn test_repl_non_interactive_mode()
```

### Priority 4: Error Path Coverage (MEDIUM)
**Timeline:** 2 days
**Effort:** Medium

```rust
#[test] fn test_invalid_kiln_path_error()
#[test] fn test_corrupted_config_error()
#[test] fn test_permission_denied_error()
#[test] fn test_binary_file_in_search()
#[test] fn test_search_query_validation()
```

### Priority 5: End-to-End Workflow Tests (MEDIUM)
**Timeline:** 2-3 days
**Effort:** Medium

```rust
#[test] fn test_new_user_workflow()
#[test] fn test_search_and_view_workflow()
#[test] fn test_config_modification_workflow()
```

### Priority 6: Security Tests (MEDIUM)
**Based on:** `secure_filesystem.rs` (3 tests exist)

**Add:**
```rust
#[test] fn test_path_traversal_prevention()
#[test] fn test_symlink_attack_prevention()
#[test] fn test_query_injection_prevention()
```

---

## Test Infrastructure Gaps

### Missing Test Utilities
1. **Test Fixture Builder**
   - Need helper to create test kilns with markdown files
   - Need helper to create test configs
   - Need helper to create test databases

2. **Assertion Helpers**
   - Output format validators
   - Error message matchers
   - Performance threshold assertions

3. **Test Data**
   - Sample markdown files with various frontmatter
   - Sample config files (valid + invalid)
   - Large test datasets for performance tests

### Recommended Test Structure
```
crates/crucible-cli/
├── tests/
│   ├── cli_integration_tests.rs      # NEW - Command-line tests
│   ├── repl_integration_tests.rs     # NEW - REPL tests
│   ├── workflow_tests.rs             # NEW - End-to-end workflows
│   ├── error_handling_tests.rs       # NEW - Error scenarios
│   ├── fixtures/
│   │   ├── sample_kiln/             # Test markdown files
│   │   ├── configs/                 # Test config files
│   │   └── mod.rs                   # Fixture helpers
│   └── common/
│       └── mod.rs                   # Shared test utilities
```

---

## Coverage Metrics

### Current Estimated Coverage
- **Core Libraries:** ~75% (Good ✅)
- **CLI Commands:** ~15% (Poor ❌)
- **REPL:** ~30% (Poor ❌)
- **Integration:** ~10% (Critical ❌)
- **Overall:** ~40% (Insufficient for MVP ❌)

### Target Coverage for MVP
- **Core Libraries:** 75%+ (Maintain ✅)
- **CLI Commands:** 70%+ (Need +55%)
- **REPL:** 60%+ (Need +30%)
- **Integration:** 60%+ (Need +50%)
- **Overall:** 65%+ (Need +25%)

---

## Action Plan for MVP

### Week 1: Critical Path
1. ✅ Create `tests/` directory structure
2. ✅ Add CLI integration tests (10 essential tests)
3. ✅ Add stats command tests (5 tests)
4. ✅ Add config command integration tests (5 tests)

**Deliverable:** Basic CLI commands testable and verified

### Week 2: Core Functionality
1. ✅ Add search command integration tests (8 tests)
2. ✅ Add fuzzy command tests (5 tests)
3. ✅ Add error handling tests (10 tests)
4. ✅ Add REPL basic tests (8 tests)

**Deliverable:** Major features tested, error paths covered

### Week 3: Refinement
1. ✅ Add workflow tests (5 end-to-end scenarios)
2. ✅ Add performance baselines (3 tests)
3. ✅ Add security tests (5 tests)
4. ✅ Fix any discovered bugs

**Deliverable:** Production-ready CLI with high confidence

---

## Risk Assessment

### High Risk (No Tests)
- ❌ REPL mode (default behavior)
- ❌ Search command (core feature)
- ❌ Config init (first-run experience)
- ❌ Stats command (user-facing)

### Medium Risk (Partial Tests)
- ⚠️ Fuzzy search (unit tests only)
- ⚠️ Config loading (no file I/O tests)
- ⚠️ Output formatting (limited integration)

### Low Risk (Well Tested)
- ✅ Pipeline processing
- ✅ Merkle tree operations
- ✅ Parser functionality
- ✅ Property storage

---

## Conclusion

**Current State:** The codebase has good foundational library tests but **critically lacks CLI integration tests** needed for MVP confidence.

**Key Issue:** Zero end-to-end tests for the actual CLI interface means we cannot verify the user experience works as expected.

**Recommendation:** Prioritize creating the `tests/` directory and implementing Priority 1-3 items within the next week. This will provide the minimum viable test coverage to ship an MVP with confidence.

**Estimated Effort:**
- Minimum viable tests (P1-P2): ~4 days
- Recommended tests (P1-P3): ~7 days
- Complete coverage (P1-P6): ~14 days

**Next Steps:**
1. Create CLI integration test suite (Priority 1)
2. Run full test suite and fix any failures
3. Set up CI to prevent regressions
4. Document test coverage gaps as technical debt

---

**Generated by:** Claude Code
**Review Status:** Draft - Needs team review
**Last Updated:** 2025-11-17
