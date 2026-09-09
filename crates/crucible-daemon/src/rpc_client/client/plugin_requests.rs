//! Wire request types for the `plugin.*` and `project.*` RPC methods.
//!
//! Both sides of the wire use them: the client serializes the struct and the
//! daemon's handler deserializes THE SAME struct (gate A6). They used to be
//! `#[derive(Serialize)] struct …Params` declared inside the client function,
//! with the server naming the fields again in `require_param!`.

/// Request for `plugin.publications`. An absent `key` asks for every key.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PluginPublicationsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Request for `plugin.options`.
///
/// `ui` is the frontend asking ("tui" or "web"); it drives the per-frontend
/// hide flags. Absent means "web", which is what the handler substituted.
/// An absent `plugin` asks for every plugin's tree.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PluginOptionsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

/// Request for `plugin.option_get`, `plugin.option_set` and
/// `plugin.option_execute` — one path through one plugin's settings tree.
///
/// `value` is read by `option_set` only; the other two never send it, and an
/// absent one is `null`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginOptionCallRequest {
    pub plugin: String,
    /// Defaulted, not required, so an absent `path` reaches the handler's own
    /// "`path` is required" answer instead of a serde "missing field" — the
    /// message callers have always seen for this mistake.
    #[serde(default)]
    pub path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub value: serde_json::Value,
}

/// Request for `plugin.run_command`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginRunCommandRequest {
    pub name: String,
    /// Whatever the command's Lua `fn` expects. `null` when the caller sends
    /// nothing.
    #[serde(default)]
    pub args: serde_json::Value,
}

/// Request for `plugin.install`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginInstallRequest {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
}

/// Request for `plugin.remove`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginRemoveRequest {
    pub name: String,
    #[serde(default)]
    pub purge: bool,
}
