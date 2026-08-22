//! Tests for session events.

use super::helpers::truncate;
use super::*;
use std::path::PathBuf;

mod events;
mod types;

/// Cross-platform test path helper
pub(super) fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("crucible_test_{}", name))
}
