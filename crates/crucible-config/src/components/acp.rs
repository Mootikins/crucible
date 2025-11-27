//! Simple ACP (Agent Client Protocol) configuration

use serde::{Deserialize, Serialize};

/// ACP configuration - practical settings for agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpConfig {
    /// Default agent to use (opencode, claude, gemini, etc.)
    pub default_agent: Option<String>,
    /// Enable agent discovery
    #[serde(default = "default_true")]
    pub enable_discovery: bool,
    /// Session timeout in minutes
    #[serde(default = "default_session_timeout")]
    pub session_timeout_minutes: u64,
    /// Maximum message size in MB (prevents oversized requests)
    #[serde(default = "default_max_message_size")]
    pub max_message_size_mb: usize,
    /// Streaming response timeout in minutes (time for complete LLM response)
    /// Default is 15 minutes to accommodate complex reasoning tasks
    #[serde(default = "default_streaming_timeout")]
    pub streaming_timeout_minutes: u64,
}

fn default_true() -> bool { true }
fn default_session_timeout() -> u64 { 30 }
fn default_max_message_size() -> usize { 25 }
fn default_streaming_timeout() -> u64 { 15 }

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            default_agent: None, // Auto-discover first available
            enable_discovery: true,
            session_timeout_minutes: 30,
            max_message_size_mb: 25,
            streaming_timeout_minutes: 15,
        }
    }
}