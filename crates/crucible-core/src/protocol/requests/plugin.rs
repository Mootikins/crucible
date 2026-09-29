//! Wire types of the `plugin` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

/// Request for `plugin.publications`. An absent `key` asks for every key.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "openapi", into_params(parameter_in = Query))]
pub struct PluginPublicationsRequest {
    /// Narrow the reply to one contribution kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Request for `surface.list` and `surface.get`.
///
/// Both take the same shape. `list` ignores `name`; `get` requires it. A `plugin`
/// narrows either, because two plugins may declare a surface of the same name
/// and a client asking for one should not be handed the other.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SurfaceRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// What `surface.list` answers: every declared surface, rows included.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SurfaceListReply {
    pub surfaces: Vec<crate::types::Surface>,
}

/// What `surface.get` answers: one surface, or `null` when nothing declares
/// it. Always written, so `null` means "not found", never "unknown".
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SurfaceGetReply {
    #[serde(default)]
    pub surface: Option<crate::types::Surface>,
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginRunCommandRequest {
    pub name: String,
    /// Whatever the command's Lua `fn` expects. `null` when the caller sends
    /// nothing; the command then gets an empty table.
    #[serde(default)]
    pub args: serde_json::Value,
    /// The session the user ran the command from, when there is one. The
    /// command reads it as `ctx.session_id`, so a command that acts on "this
    /// session" does not need the user to type an id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Request for `plugin.install`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginInstallRequest {
    /// The plugin URL, for example `user/repo` or a full git URL.
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
}

/// One row of the `spec` array in `plugin.list`'s response: a merged spec
/// entry, the highest rank that wrote it, and whether the operator's own
/// entry names a git source (`declared`). `cru plugin list` reads the git
/// rows; the daemon builds them from the spec store.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginSpecRow {
    #[serde(flatten)]
    pub entry: crate::config::SpecEntry,
    pub rank: crate::config::SpecRank,
    pub declared: bool,
}

/// Request for `webhook.receive`: one webhook delivery that the HTTP edge
/// already checked.
///
/// `body` is the text that the sender wrote. The signature covers those
/// bytes, so the daemon does not parse or change them.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WebhookReceiveRequest {
    /// The name of the webhook, from its route.
    pub name: String,
    pub headers: serde_json::Map<String, serde_json::Value>,
    pub body: String,
}

/// Request for `plugin.remove`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginRemoveRequest {
    pub name: String,
    #[serde(default)]
    pub purge: bool,
}
