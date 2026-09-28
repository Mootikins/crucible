//! Shared RPC request/response types used across client submodules.
//!
//! This module houses the common types (wire-format structs, generic
//! parameter shapes, and small helpers) that are referenced from more
//! than one of the split submodules (`agent`, `lua`, `session`,
//! `storage`, `subscription`).

/// A session event that the daemon sent to this client.
///
/// This is the wire type itself. A client reads the wire type once; it
/// does not keep a second copy with other field names.
pub type SessionEvent = crucible_core::protocol::SessionEventMessage;

// =========================================================================
// Generic RPC Request Types
// =========================================================================

/// Extract a string array from a JSON value at the given key.
pub(super) fn extract_string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
