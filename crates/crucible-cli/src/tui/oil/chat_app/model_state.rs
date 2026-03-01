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

#[derive(Debug, Clone)]
pub struct PluginStatusEntry {
    pub name: String,
    pub version: String,
    pub state: String,
    pub error: Option<String>,
}
