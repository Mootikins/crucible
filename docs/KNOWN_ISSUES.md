# Known Issues

**Last Updated:** 2025-10-26

---

## Test Failures

### CLI Config Tests - FIXED ✅

**Status:** RESOLVED in Phase 2.0 (2025-10-26)
**Previously Failing Tests:** (all now PASS)
- ✅ `crucible-cli::config::tests::test_llm_config_from_file`
- ✅ `crucible-cli::config::tests::test_database_path_derivation`
- ✅ `crucible-cli::config::tests::test_tools_path_derivation`
- ✅ `crucible-cli::config::tests::test_builder_override` (renamed from test_environment_variable_override)

**Fix Applied:**
- Removed ALL environment variable overrides from `CliConfig::load()`
- Added `CliConfigBuilder` pattern for programmatic configuration
- Updated tests to use builder instead of env vars
- No more test pollution from parallel execution

**Result:** All CLI config tests pass with `cargo test --lib -p crucible-cli` (parallel execution)

---

### 5 Daemon Unit Tests Failing

**Status:** Pre-existing Issue - Will be resolved in Phase 2.6
**Priority:** Low (daemon config being refactored anyway)
**Affected Tests:**
- `crucible-daemon::config::tests::test_default_config`
- `crucible-daemon::coordinator::tests::test_config_update`
- `crucible-daemon::coordinator::tests::test_event_publishing`
- `crucible-daemon::coordinator::tests::test_daemon_health_tracking`
- `crucible-daemon::events::tests::test_event_statistics`

**Root Cause:**
`DaemonConfig::default()` creates invalid configuration:
- `watch_paths` is empty (validation requires at least one)
- `connection_string` is empty (validation requires non-empty)
- These fields would normally be set via `DaemonConfig::from_env()` or file loading

**Impact:**
- Does NOT affect functionality (daemon loads config from files/env, not defaults)
- Only affects unit tests that use `DaemonConfig::default()`
- NOT related to Phase 2.0 env var removal (daemon still has its own env var logic)

**Plan:**
Phase 2.6 will simplify/remove DaemonConfig entirely, replacing it with `crucible-config::Config`. These tests will be either:
1. Removed (if testing obsolete daemon-specific config)
2. Rewritten (if testing important daemon functionality with new config)

**Workaround:**
Not needed - tests are not blocking development. Daemon functionality works via file/env config loading.

---

## Compilation Warnings

### Dead Code Warnings in `search.rs`

**Status:** Low Priority
**File:** `crates/crucible-cli/src/commands/search.rs`
**Warnings:**
```
warning: constant `MAX_CONTENT_LENGTH` is never used
warning: constant `MAX_QUERY_LENGTH` is never used
warning: constant `MIN_QUERY_LENGTH` is never used
warning: function `read_file_with_utf8_recovery` is never used
```

**Impact:** None (warnings only, doesn't affect functionality)

**Fix:** Remove unused code or mark with `#[allow(dead_code)]` if intended for future use

---

## Dependency Warnings

### Rune Future Incompatibility

**Status:** External Dependency Issue
**Package:** `rune v0.13.4`
**Warning:**
```
warning: the following packages contain code that will be rejected by a future version of Rust: rune v0.13.4
note: to see what the problems were, use the option `--future-incompat-report`
```

**Impact:** Will break on future Rust versions

**Fix Options:**
1. Wait for rune upstream update
2. Pin Rust version temporarily
3. Investigate alternative scripting solutions

**Tracking:** Monitor rune releases at https://github.com/rune-rs/rune

---

## Archived Tests

### 38+ Tests Archived Due to Architecture Changes

**Status:** Expected - Will be restored in Phase 3
**Location:** `tests/archive/broken_tests_2025_10_26/`

**Reason:**
Tests depended on removed `crucible_services` architecture and over-engineered `DaemonConfig`.

**Plan:**
After Phase 2 (config consolidation), restore critical tests with simplified architecture.

**Priority List:**
See: `docs/TEST_RESTORATION_PLAN.md`

**High Priority Tests to Restore:**
1. Embedding pipeline tests
2. Semantic search tests
3. Event pipeline integration tests

---

## Documentation

### Related Documents
- [Config Consolidation Plan](./CONFIG_CONSOLIDATION_PLAN.md) - Phase 2 plan
- [Test Restoration Plan](./TEST_RESTORATION_PLAN.md) - Post-Phase 2 test restoration
- [Architecture Documentation](./ARCHITECTURE.md) - System architecture

---

## Resolution History

### Phase 1 (2025-10-26) - Completed ✅
**Goal:** Get to 0 test compilation errors
**Result:** 176 errors → 0 errors
**Actions:**
- Archived 38+ broken daemon tests
- Fixed module visibility issues
- Fixed struct field API changes
- Fixed trait signature mismatches

**Remaining Work:**
- Phase 2: Config consolidation (150 → ~50 structs)
- Phase 3: Restore critical tests
- Fix 4 env var pollution test failures
