mod format;
mod parse;
mod roundtrip;

use std::path::PathBuf;

// Fixed timestamp for consistent test output: 2025-12-14T15:30:45.123 UTC
// Calculated using: datetime.datetime(2025, 12, 14, 15, 30, 45, 123000, tzinfo=datetime.timezone.utc).timestamp() * 1000
pub(super) const TEST_TIMESTAMP_MS: u64 = 1765726245123;

/// Cross-platform test path helper
pub(super) fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("crucible_test_{}", name))
}
