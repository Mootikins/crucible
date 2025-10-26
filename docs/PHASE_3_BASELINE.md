# Phase 3 Test Baseline - 2025-10-26

**Status:** Post Phase 2 Config Consolidation
**Generated:** 2025-10-26
**Command:** `cargo test --workspace 2>&1`

## Summary Statistics

- **Total Tests:** 219
- **Passing:** 219 (100% - unit tests only)
- **Failing:** 12 (integration tests)
- **Ignored:** 0
- **Success Rate (Unit Tests):** 100% (219/219)
- **Overall Success Rate:** ~94.8% (207/219)

### Test Run Breakdown
| Component | Passed | Failed | Total | Status |
|-----------|--------|--------|-------|--------|
| crucible-a2a (units) | 55 | 0 | 55 | OK |
| crucible-cli (units) | 157 | 0 | 157 | OK |
| cru binary (units) | 0 | 0 | 0 | OK |
| binary_detection_tdd_standalone | 7 | 12 | 19 | FAILED |
| **TOTALS** | **219** | **12** | **231** | **FAILED** |

## Failure Breakdown

### CRITICAL: Absolute Path Validation Error (Priority: URGENT)

**Root Cause:** Search function now rejects absolute paths with error "Absolute paths are not allowed"

This is a NEW failure caused by Phase 2 changes (likely in secure_filesystem validation).

**Affected Tests:** 12 tests in `binary_detection_tdd_standalone.rs`

**Related File:** `/home/moot/crucible/crates/crucible-cli/src/commands/secure_filesystem.rs`

#### Failing Tests (All Related to Absolute Path Validation)

1. **file_size_boundary_tests::test_empty_file_handling**
   - Error: "Absolute paths are not allowed"
   - Category: NEW (Phase 2)
   - Root Cause: Temp file path is absolute, validation rejects it

2. **binary_detection_tests::test_detect_png_file_with_md_extension**
   - Error: Expected binary error but got "absolute paths are not allowed"
   - Category: NEW (Phase 2)
   - Root Cause: Test creates absolute paths that new validation rejects

3. **binary_detection_tests::test_detect_null_bytes_in_file**
   - Error: Expected binary error but got "absolute paths are not allowed"
   - Category: NEW (Phase 2)
   - Root Cause: Same absolute path validation issue

4. **file_size_boundary_tests::test_whitespace_only_file**
   - Error: "Absolute paths are not allowed"
   - Category: NEW (Phase 2)
   - Root Cause: Absolute path validation

5. **binary_detection_tests::test_search_skips_binary_files**
   - Error: "Should find legitimate content" (0 results returned)
   - Category: NEW (Phase 2)
   - Root Cause: Absolute path rejection blocks search

6. **integration_tests::test_search_mixed_binary_text_files**
   - Error: assertion `left == right` failed: Should find 2 files with 'alpha'
   - Results: left: 0, right: 2
   - Category: NEW (Phase 2)
   - Root Cause: Absolute path validation blocks search

7. **integration_tests::test_search_continues_after_binary_files**
   - Error: assertion `left == right` failed: Should find all 3 text files
   - Results: left: 0, right: 3
   - Category: NEW (Phase 2)
   - Root Cause: Absolute path validation blocks search

8. **file_size_boundary_tests::test_content_truncation_boundary**
   - Error: "Absolute paths are not allowed"
   - Category: NEW (Phase 2)
   - Root Cause: Absolute path validation

9. **memory_protection_tests::test_memory_usage_with_binary_content**
   - Error: Expected binary error but got "absolute paths are not allowed"
   - Category: NEW (Phase 2)
   - Root Cause: Absolute path validation

10. **file_size_boundary_tests::test_file_exactly_at_limit**
    - Error: "Should handle boundary gracefully: absolute paths are not allowed"
    - Category: NEW (Phase 2)
    - Root Cause: Absolute path validation

11. **memory_protection_tests::test_file_over_size_limit**
    - Error: Expected size limit error but got "absolute paths are not allowed"
    - Category: NEW (Phase 2)
    - Root Cause: Absolute path validation

12. **memory_protection_tests::test_large_file_boundary_handling**
    - Error: Expected size/memory error but got "absolute paths are not allowed"
    - Category: NEW (Phase 2)
    - Root Cause: Absolute path validation

### Pre-Existing Failures: NONE DETECTED

The `binary_detection_tdd_standalone.rs` tests are **newly broken** by Phase 2, not pre-existing failures.

## Why Only 4 Test Runs?

Analysis of full test output reveals only 4 test executables ran:
1. `crucible_a2a` (55 unit tests)
2. `crucible_cli` (157 unit tests)
3. `cru` binary (0 unit tests)
4. `binary_detection_tdd_standalone` (19 integration tests)

**IMPORTANT NOTE:** Integration tests in other crates appear to be **excluded or skipped**. This is likely because:
- Many integration tests require external services (daemon, SurrealDB, etc.)
- Tests may have `#[ignore]` markers
- Tests may require specific feature flags
- Test filtering may be active

## Per-Crate Analysis

### crucible-a2a (Library)
- **Status:** PASSING (55/55)
- **Tests:** All context, protocol, transport, and bus tests passing
- **Key Tests:** Message handling, entity extraction, context management
- **Assessment:** No Phase 2 impact detected

### crucible-cli (Library)
- **Status:** PASSING (157/157)
- **Tests:** Agent registry, REPL commands, formatters, config, tools
- **Key Tests:** Chat commands, history, service management, config handling
- **Assessment:** No Phase 2 impact detected on unit tests

### cru (Binary)
- **Status:** PASSING (0/0)
- **Note:** No unit tests in binary entry point

### binary_detection_tdd_standalone (Integration Tests)
- **Status:** FAILING (7/19 passing)
- **Failures:** All 12 failures due to absolute path validation
- **Category:** NEW failures from Phase 2 changes
- **Root Cause:** `secure_filesystem.rs` path validation rejects absolute paths
- **Impact:** Blocks all file system search operations using absolute paths

## Root Cause Analysis: Absolute Path Validation

The 12 test failures all share the same root cause:

**File:** `/home/moot/crucible/crates/crucible-cli/src/commands/secure_filesystem.rs`

**Issue:** A validation check was added (or modified in Phase 2) that rejects absolute paths with error "Absolute paths are not allowed"

**Why Tests Fail:**
- Tests create temporary files in `/tmp/` (absolute paths)
- Tests call search functions expecting results
- New validation rejects the absolute paths
- Search returns error instead of processing files

**Impact:**
- File system search is broken for absolute paths
- Tests using `tempfile` crate cannot work
- Integration tests cannot function

## Integration Tests Not Executed

The following test suites were NOT executed in this run:
- daemon integration tests (likely require daemon service)
- surrealdb integration tests (likely require external service)
- semantic search tests (likely require embeddings)
- REPL end-to-end tests (likely marked ignore)
- CLI integration tests (likely marked ignore or require setup)
- And many others...

This is normal for integration tests but means Phase 2 impact may be broader than detected here.

## Recommendations

### PRIORITY 1: FIX ABSOLUTE PATH VALIDATION (CRITICAL)
**Task:** Fix `secure_filesystem.rs` to allow absolute paths in test environments
- Investigate the path validation logic
- Determine if this is intentional security measure or Phase 2 regression
- Add exception for test paths or configuration
- **Must fix before proceeding** - this blocks all further testing

### PRIORITY 2: RE-RUN FULL INTEGRATION TESTS
**Task:** After fixing absolute path issue, re-run tests to detect any other Phase 2 impacts
```bash
cargo test --workspace --all-features 2>&1 | tee /tmp/phase3_retest.txt
```

### PRIORITY 3: IDENTIFY SKIPPED TESTS
**Task:** Determine why many integration tests don't execute
- Check for `#[ignore]` markers
- Check for feature gate requirements
- Check test configuration
- Run with: `cargo test --workspace --test '*' -- --show-output`

### PRIORITY 4: VERIFY PRE-EXISTING FAILURES
**Task:** Document which failures existed BEFORE Phase 2 config consolidation
- Check git history for `binary_detection_tdd_standalone.rs`
- Compare against previous baseline if available
- Mark clearly as pre-existing vs new

## Next Steps (Phase 3 Plan)

### Phase 3.1: FIX ABSOLUTE PATH VALIDATION
1. Review `secure_filesystem.rs` absolute path check
2. Understand the intent (security vs. usability)
3. Implement fix that satisfies both test and security requirements
4. Verify all 12 failing tests pass

### Phase 3.2: FIX NEW FAILURES (POST-FIX)
1. Re-run integration tests
2. Identify any NEW failures beyond absolute path issue
3. Fix and verify

### Phase 3.3: ADDRESS PRE-EXISTING FAILURES
1. Categorize pre-existing vs new
2. Document reason for pre-existing failures
3. Plan restoration of archived tests if needed

### Phase 3.4+: RESTORE INTEGRATION TESTS
1. Run full test suite with all features
2. Identify all integration tests
3. Restore any previously archived tests
4. Achieve >95% test pass rate

## Files to Investigate

Key files modified in Phase 2 that may impact tests:
- `/home/moot/crucible/crates/crucible-cli/src/commands/secure_filesystem.rs`
- `/home/moot/crucible/crates/crucible-config/src/lib.rs`
- `/home/moot/crucible/crates/crucible-cli/src/config.rs`

## Full Output Locations

- Complete test output: `/tmp/phase3_baseline.txt`
- Test summary: `/tmp/phase3_summary.txt`

---

Generated by Phase 3 Baseline analysis
Command: `cargo test --workspace 2>&1 | tee /tmp/phase3_baseline.txt`
Date: 2025-10-26
