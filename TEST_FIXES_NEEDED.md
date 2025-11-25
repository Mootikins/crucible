# Test Fixes Needed for PR

## Pipeline Integration Tests (2 failures)

**Failing Tests:**
- `test_pipeline_detects_content_changes`
- `test_full_pipeline_with_embeddings`

**Root Cause:**
These tests are failing because of the database path change in Bug #4 fix. The fix changed `database_path()` to use a stable path (`kiln.db`) in production but PID-based paths (`kiln-<PID>.db`) in test mode when `CRUCIBLE_TEST_MODE=1` is set.

**Solution:**
Add `CRUCIBLE_TEST_MODE=1` environment variable to these test functions, or use the test helper that sets it automatically.

**Files to Fix:**
- `/home/moot/crucible-fix-process-pipeline/crates/crucible-pipeline/tests/pipeline_integration_tests.rs`

**Example Fix:**
```rust
#[tokio::test]
async fn test_pipeline_detects_content_changes() {
    // Set test mode to use PID-based database paths
    std::env::set_var("CRUCIBLE_TEST_MODE", "1");

    // ... rest of test
}
```

Or ensure the test creates a custom database path explicitly via the config builder.

## Related Changes

**File Modified:** `/home/moot/crucible-fix-process-pipeline/crates/crucible-cli/src/config.rs`
**Lines Changed:** 587-601

**Change Summary:**
```rust
// OLD: Always used PID suffix
let db_name = format!("kiln-{}.db", pid);

// NEW: Only use PID suffix in test mode
let db_name = if std::env::var("CRUCIBLE_TEST_MODE").is_ok() {
    let pid = std::process::id();
    format!("kiln-{}.db", pid)
} else {
    "kiln.db".to_string()
};
```

This change was necessary to fix Bug #4 (change detection persistence), but it affects tests that create pipelines without setting the test mode flag.
