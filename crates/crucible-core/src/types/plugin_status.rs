//! Plugin status entry emitted by the daemon after plugin discovery.
//!
//! Surfaced via the `plugins_discovered` session setup event so the TUI
//! plugin panel can render discovered plugins with their version and load
//! state.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStatusEntry {
    pub name: String,

    /// The version the plugin declares, or `None` while nothing has read it.
    ///
    /// Enumeration does not run a plugin's Lua, and the version lives in the
    /// spec table that only a load reads — so a discovered plugin has no
    /// version to report. This field carried a synthesized `"0.0.0"` there,
    /// which every front end drew as if it were a release.
    #[serde(default)]
    pub version: Option<String>,

    pub state: String,
    pub error: Option<String>,
}
