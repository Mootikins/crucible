//! Wire types of the `ui.*` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.
//!
//! The replies stay `serde_json::Value`: `ui.config`'s snapshot is
//! Lua-declared theme, highlight, geometry and layout data, the same kind of
//! openness `lua.eval` has. Only the params, which the handlers used to read
//! by hand off a raw `&Request`, are typed here.

/// Params for `ui.config`.
///
/// `session_id` picks the expression values a caller sees riding along with
/// the snapshot (`crate::rpc::ui::style_payload` in `crucible-daemon`); an
/// absent id reads as the global, session-less snapshot.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UiConfigRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Params for `ui.set_theme`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UiSetThemeRequest {
    pub name: String,
}

/// Reply from `ui.set_theme`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UiSetThemeReply {
    /// The theme's own name, which may differ from `name` in case only.
    pub theme: String,
}
