use serde::{Deserialize, Serialize};

/// The client-facing status item. The daemon keeps the authored list; clients
/// only decide where and how it fits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusDisplayItem {
    pub id: String,
    pub text: String,
    pub priority: u8,
    pub color_group: String,
    pub action: Option<String>,
    pub pinned: bool,
    pub plugin: String,
    /// Who made the item. The TUI places each kind with its own statusline
    /// item; the web draws every kind in one slot.
    #[serde(default)]
    pub kind: StatusItemKind,
}

/// The source of a status item.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StatusItemKind {
    /// A plugin published it through `cru.statusline.publish` or
    /// `cru.plugin.set_status`. `sl.items` draws it.
    #[default]
    Published,
    /// The engine made it from the session's plugin approval knob and the
    /// plugin turn that runs now. `sl.plugin_turns` draws it.
    PluginTurns,
}
