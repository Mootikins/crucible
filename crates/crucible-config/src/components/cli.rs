//! Simple CLI configuration

use serde::{Deserialize, Serialize};

/// Simple CLI configuration - only essential user settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfigComponent {
    /// Show progress bars for long operations
    #[serde(default = "default_true")]
    pub show_progress: bool,
    /// Confirm destructive operations
    #[serde(default = "default_true")]
    pub confirm_destructive: bool,
    /// Verbose logging
    #[serde(default)]
    pub verbose: bool,
}

fn default_true() -> bool { true }

impl Default for CliConfigComponent {
    fn default() -> Self {
        Self {
            show_progress: true,
            confirm_destructive: true,
            verbose: false,
        }
    }
}