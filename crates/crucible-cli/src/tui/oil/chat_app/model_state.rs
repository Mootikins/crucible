#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ModelListState {
    #[default]
    NotLoaded,
    Loading,
    Loaded,
    Failed,
}

#[derive(Debug, Clone)]
pub struct McpServerDisplay {
    pub name: String,
    pub prefix: String,
    pub tool_count: usize,
    pub connected: bool,
}

/// One kiln the session draws knowledge from, as the startup banner shows it.
///
/// A session attaches a flat set of kilns, so the banner names every one of
/// them. The daemon owns the set; this is the projection the TUI prints.
#[derive(Debug, Clone)]
pub struct KilnSummary {
    pub name: String,
    pub path: String,
}

/// The TUI renders a tool count only. The projection collapses the tool list
/// at the boundary, so the rest of the TUI never sees the tool names. The
/// background MCP gateway task refreshes the connected state and the count
/// later.
impl From<crucible_core::types::mcp_status::McpServerInfo> for McpServerDisplay {
    fn from(info: crucible_core::types::mcp_status::McpServerInfo) -> Self {
        Self {
            name: info.name,
            prefix: info.prefix.trim_end_matches('_').to_string(),
            tool_count: info.tools.len(),
            connected: info.connected,
        }
    }
}

// `PluginStatusEntry` now lives in `crucible-core` so session-setup events
// (emitted by the daemon, consumed here) share the canonical type.
pub use crucible_core::types::PluginStatusEntry;
