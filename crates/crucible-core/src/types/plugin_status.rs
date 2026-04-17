//! Plugin status entry emitted by the daemon after plugin discovery.
//!
//! Surfaced via the `plugins_discovered` session setup event so the TUI
//! plugin panel can render discovered plugins with their version and load
//! state.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStatusEntry {
    pub name: String,
    pub version: String,
    pub state: String,
    pub error: Option<String>,
}
