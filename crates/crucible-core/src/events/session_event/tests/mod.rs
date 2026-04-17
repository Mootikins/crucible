//! Tests for session events.

use super::helpers::truncate;
use super::*;
use std::path::PathBuf;

mod awaiting_input;
mod event_type;
mod pre_events;
mod serialization;
mod session_state;
mod types;

/// Cross-platform test path helper
pub(super) fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("crucible_test_{}", name))
}
