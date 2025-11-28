//! Simple chat configuration

use serde::{Deserialize, Serialize};

/// Simple chat configuration - only essential user settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Default chat model (can be overridden by agents)
    pub model: Option<String>,
    /// Enable markdown rendering
    #[serde(default = "default_true")]
    pub enable_markdown: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            model: None, // Use agent default
            enable_markdown: true,
        }
    }
}
