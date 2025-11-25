# Bug Fix Progress - Session 2025-11-24

## Context
After fixing Bugs #1-3 (backslash escaping, env var support, error messages), manual testing revealed 2 critical bugs in the pipeline.

## Manual Testing Results

Completed 22 tests across 8 suites:
- **Test Suite 1**: Basic Functionality - 2/3 PASS, **1 FAIL** (change detection broken)
- **Test Suite 2**: Verbose Flag - 2/2 PASS ✅
- **Test Suite 3**: Dry-Run Flag - 3/3 PASS ✅
- **Test Suite 4**: Watch Mode - 1/5 PASS, **1 FAIL**, 3 SKIPPED (watch detection broken)
- **Test Suite 5-6**: SKIPPED (dependent on broken features)
- **Test Suite 7**: Error Handling - 1/2 PASS ✅
- **Test Suite 8**: SKIPPED (dependent on change detection)

## Critical Bugs Found

### Bug #4: Change Detection Not Working (CRITICAL)
**Symptom:** Files reprocessed every time instead of being skipped when unchanged

**Root Cause:**
1. `get_file_state()` query uses incorrect parameter binding
2. `store_file_state()` uses UPDATE instead of CREATE, so records never get created
3. Query always returns empty, pipeline thinks files are new every time

**Files Affected:**
- `/home/moot/crucible-fix-process-pipeline/crates/crucible-surrealdb/src/change_detection_store.rs`

**Current Fix Status:** IN PROGRESS - 75% complete
- ✅ Added 2 failing tests (RED phase complete)
- ✅ Fixed `get_file_state()` to query by relative_path field with string interpolation
- 🔧 **BLOCKED**: Fixing `store_file_state()` to use DELETE+CREATE pattern
  - Changed from UPDATE to CREATE
  - Issue: SurrealDB parameter binding doesn't support `$named` with positional arrays
  - **Next Step**: Change query to use positional parameters `$1, $2, $3, $4` instead

**Current Error:**
```
Query returned error: Expected a datetime but cannot convert NONE into a datetime
```

This means parameters aren't being bound correctly. Need to:
1. Change query from `$relative_path, $file_hash, $modified_time, $file_size`
2. To: `$1, $2, $3, $4` (positional parameters)

**Code Changes Made:**
```rust
// Line 99 in change_detection_store.rs - FIXED
let query = format!("SELECT * FROM file_state WHERE relative_path = '{}' LIMIT 1",
    relative_path.replace("'", "\\'"));

// Lines 136-148 - IN PROGRESS (needs positional params)
let delete_query = format!("DELETE file_state:`{}`", record_id);
self.client.query(&delete_query, &[]).await.ok();

let query = format!(r#"
    CREATE file_state:`{}` CONTENT {{
        relative_path: $1,
        file_hash: $2,
        modified_time: type::datetime($3),
        file_size: $4
    }}
"#, record_id);  // <-- Need to update to use $1, $2, $3, $4
```

### Bug #5: Watch Mode Not Detecting Changes (CRITICAL)
**Symptom:** Watch mode starts but never triggers reprocessing when files change

**Root Cause:**
1. Event sender setup has Arc/mutability conflict
2. Factory creates `Arc<dyn FileWatcher>` but process command needs mutable access
3. Event sender never gets properly connected
4. Watch handle discarded with underscore prefix

**Files Affected:**
- `/home/moot/crucible-fix-process-pipeline/crates/crucible-cli/src/commands/process.rs` (lines 152-242)
- `/home/moot/crucible-fix-process-pipeline/crates/crucible-watch/src/manager.rs`
- `/home/moot/crucible-fix-process-pipeline/crates/crucible-watch/src/traits.rs`

**Current Fix Status:** NOT STARTED (waiting for Bug #4)
- ⏳ RED phase: Write failing test
- ⏳ GREEN phase: Refactor watch API to accept event sender during watch() call
- ⏳ VERIFY phase: Test watch detects changes

**Planned Fix:**
1. Change `watch()` method to accept optional event sender parameter
2. Remove `set_event_sender()` mutable method
3. Keep watch handle alive (remove underscore)
4. Add shutdown signal to stop watch cleanly

## Next Steps (In Order)

1. **IMMEDIATE**: Fix Bug #4 parameter binding
   - Change `store_file_state()` query to use `$1, $2, $3, $4` positional params
   - Run test to verify records get created and retrieved
   - If passes, run full change_detection_store test suite
   - Run manual Test 1.2 to verify idempotency

2. **After Bug #4 Fixed**: Implement Bug #5 fixes
   - Write failing test for watch mode
   - Refactor watch API
   - Update process command
   - Verify watch detects changes

3. **Final Verification**:
   - Re-run full manual test suite (22 tests)
   - Update BUGS.md to mark #4 and #5 as FIXED
   - Commit changes with TDD summary

## Files Modified So Far

### `/home/moot/crucible-fix-process-pipeline/crates/crucible-surrealdb/src/change_detection_store.rs`
- Lines 99-104: Fixed get_file_state() query to use string interpolation
- Lines 136-160: Partially fixed store_file_state() to use DELETE+CREATE (needs positional params)
- Lines 231-309: Added 2 new failing tests

## Test Files Added
- `test_store_and_retrieve_file_state()` - Tests store/retrieve round-trip
- `test_skip_unchanged_files()` - Tests end-to-end change detection

## Estimated Completion
- Bug #4: 30 minutes (90% done, just parameter fix needed)
- Bug #5: 2 hours (API refactoring across multiple files)
- Manual testing: 30 minutes
- **Total remaining**: ~3 hours

## Important Notes
- All previous bugs (#1-3) remain fixed and tested
- Bug #4 fix is very close - just parameter binding issue
- Bug #5 is architectural but fix is straightforward
- No regressions detected in passing tests
