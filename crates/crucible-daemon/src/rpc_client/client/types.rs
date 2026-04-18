//! Shared RPC request/response types used across client submodules.
//!
//! This module houses the common types (wire-format structs, generic
//! parameter shapes, and small helpers) that are referenced from more
//! than one of the split submodules (`agent`, `lua`, `session`,
//! `storage`, `subscription`).

/// Session event received from daemon
#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub session_id: String,
    pub event_type: String,
    pub data: serde_json::Value,
}

/// Daemon capabilities returned by `daemon.capabilities` RPC
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DaemonCapabilities {
    pub version: String,
    #[serde(default)]
    pub build_sha: Option<String>,
    pub protocol_version: String,
    pub capabilities: CapabilityFlags,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CapabilityFlags {
    pub kilns: bool,
    pub sessions: bool,
    pub agents: bool,
    pub events: bool,
    pub thinking_budget: bool,
    pub model_switching: bool,
}

// =========================================================================
// Generic RPC Request Types
// =========================================================================

/// Empty request for methods that take no parameters.
#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct EmptyParams {}

/// Request for methods that take only a kiln path.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KilnPathRequest {
    pub kiln: String,
}

/// Request for methods that take only a filesystem path.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PathRequest {
    pub path: String,
}

/// Request for methods that take only a name.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NameRequest {
    pub name: String,
}

/// Request for `skills.list`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillsListRequest {
    pub kiln_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_filter: Option<String>,
}

/// Request for `skills.get`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillsGetRequest {
    pub name: String,
    pub kiln_path: String,
}

/// Request for `skills.search`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillsSearchRequest {
    pub query: String,
    pub kiln_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionCheck {
    Match,
    Mismatch { client: String, daemon: String },
}

impl VersionCheck {
    pub fn is_match(&self) -> bool {
        matches!(self, Self::Match)
    }
}

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
