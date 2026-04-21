//! Shared serde helper functions for `#[serde(default = "...")]` attributes.

/// Returns `true`, for use with `#[serde(default = "...")]` on boolean fields
/// that should default to enabled.
pub fn default_true() -> bool {
    true
}
