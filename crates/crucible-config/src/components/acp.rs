//! ACP (Agent Client Protocol) component configuration
//!
//! Configuration for agent client protocol, session management, and agent discovery.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// ACP component configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpComponentConfig {
    pub enabled: bool,
    pub session: SessionConfig,
    pub protocols: ProtocolConfig,
    pub agents: AgentConfig,
    pub discovery: DiscoveryConfig,
}

/// Session management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub persistence_enabled: bool,
    pub max_session_duration_minutes: u64,
    pub session_timeout_minutes: u64,
    pub storage_path: Option<PathBuf>,
    pub cleanup_policy: CleanupPolicy,
}

/// Protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    pub version: String,
    pub max_message_size_mb: usize,
    pub heartbeat_interval_seconds: u64,
    pub timeout_seconds: u64,
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub default_agent: Option<String>,
    pub allowed_agents: Vec<String>,
    pub auto_discovery: bool,
    pub discovery_paths: Vec<PathBuf>,
}

/// Agent discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub scan_interval_seconds: u64,
    pub discovery_paths: Vec<PathBuf>,
    pub agent_patterns: Vec<String>,
}

/// Session cleanup policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CleanupPolicy {
    #[serde(rename = "never")]
    Never,
    #[serde(rename = "on_startup")]
    OnStartup,
    #[serde(rename = "periodic")]
    Periodic { interval_hours: u64 },
    #[serde(rename = "on_timeout")]
    OnTimeout,
}

impl Default for AcpComponentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            session: SessionConfig::default(),
            protocols: ProtocolConfig::default(),
            agents: AgentConfig::default(),
            discovery: DiscoveryConfig::default(),
        }
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            persistence_enabled: true,
            max_session_duration_minutes: 120, // 2 hours
            session_timeout_minutes: 30,      // 30 minutes
            storage_path: Some(PathBuf::from("./sessions")),
            cleanup_policy: CleanupPolicy::Periodic { interval_hours: 24 },
        }
    }
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            version: "0.6".to_string(),
            max_message_size_mb: 10,
            heartbeat_interval_seconds: 30,
            timeout_seconds: 60,
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            default_agent: Some("claude".to_string()),
            allowed_agents: vec![
                "claude".to_string(),
                "gemini".to_string(),
                "opencode".to_string(),
            ],
            auto_discovery: true,
            discovery_paths: vec![
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
            ],
        }
    }
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            scan_interval_seconds: 300, // 5 minutes
            discovery_paths: vec![
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("./bin"),
            ],
            agent_patterns: vec![
                "*claude*".to_string(),
                "*gemini*".to_string(),
                "*opencode*".to_string(),
            ],
        }
    }
}