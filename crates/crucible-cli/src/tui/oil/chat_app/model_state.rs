#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ModelListState {
    #[default]
    NotLoaded,
    Loading,
    Loaded,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct McpServerDisplay {
    pub name: String,
    pub prefix: String,
    pub tool_count: usize,
    pub connected: bool,
}

// `PluginStatusEntry` now lives in `crucible-core` so session-setup events
// (emitted by the daemon, consumed here) share the canonical type.
pub use crucible_core::types::PluginStatusEntry;
