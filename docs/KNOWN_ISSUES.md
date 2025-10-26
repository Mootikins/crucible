# Known Issues

**Last Updated:** 2025-10-26

---

## Test Failures

### 4 Config Tests Failing Due to Environment Variable Pollution

**Status:** Known Issue - Will be fixed in Phase 2
**Priority:** Medium (tests pass individually, fail in parallel)
**Affected Tests:**
- `crucible-cli::config::tests::test_llm_config_from_file`
- `crucible-cli::config::tests::test_database_path_derivation`
- `crucible-cli::config::tests::test_tools_path_derivation`
- `crucible-cli::config::tests::test_environment_variable_override`

**Root Cause:**
Tests use environment variables (`OBSIDIAN_KILN_PATH`, `CRUCIBLE_CHAT_MODEL`, etc.) for configuration. When tests run in parallel (default `cargo test` behavior), they interfere with each other through shared environment state.

**Symptoms:**
- ✅ Each test passes when run individually: `cargo test --lib -p crucible-cli test_database_path_derivation`
- ❌ Tests fail when run together: `cargo test --lib -p crucible-cli`
- Error: Path mismatches due to race conditions setting/reading env vars

**Workaround (Temporary):**
```bash
# Run tests serially to avoid pollution
cargo test --workspace -- --test-threads=1

# Or run only the failing package serially
cargo test -p crucible-cli --lib -- --test-threads=1
```

**Permanent Fix (Phase 2):**
Remove ALL environment variable configuration and use file-based config only.

**Implementation Plan:**
See: `docs/CONFIG_CONSOLIDATION_PLAN.md` → Step 0: Remove Environment Variable Configuration

**Key Changes:**
1. Remove env var overrides from `CliConfig::load()`
2. Add `ConfigBuilder` pattern for tests
3. Use CLI args for overrides (explicit, not env vars)
4. Provide migration tool: `cru config migrate-env-vars`

**Estimated Fix:** 4-6 hours (part of Phase 2)

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
