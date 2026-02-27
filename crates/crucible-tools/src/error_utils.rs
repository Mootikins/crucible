//! Utility functions for handling tool error messages.
//!
//! This module re-exports `strip_tool_error_prefix` from `crucible_core::error_utils`
//! for backward compatibility. New code should import directly from crucible_core.

pub use crucible_core::error_utils::strip_tool_error_prefix;
