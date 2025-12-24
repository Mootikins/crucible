//! Simple ACP (Agent Client Protocol) configuration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ACP configuration - practical settings for agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpConfig {
    /// Default agent to use (opencode, claude, gemini, or custom profile name)
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
    /// Custom agent profiles with environment variable overrides
    #[serde(default)]
    pub agents: HashMap<String, AgentProfile>,
    /// Enable lazy agent selection (show splash to pick agent before creating)
    /// When false, agent is created immediately on startup
    #[serde(default = "default_true")]
    pub lazy_agent_selection: bool,
}

/// Configuration profile for an ACP agent
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentProfile {
    /// Base agent to extend (opencode, claude, gemini, etc.)
    pub extends: Option<String>,
    /// Custom command to run (overrides built-in command)
    pub command: Option<String>,
    /// Custom arguments (overrides built-in args)
    pub args: Option<Vec<String>>,
    /// Environment variables to pass to the agent process
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_true() -> bool {
    true
}
fn default_session_timeout() -> u64 {
    30
}
fn default_max_message_size() -> usize {
    25
}
fn default_streaming_timeout() -> u64 {
    15
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            default_agent: None, // Auto-discover first available
            enable_discovery: true,
            session_timeout_minutes: 30,
            max_message_size_mb: 25,
            streaming_timeout_minutes: 15,
            agents: HashMap::new(),
            lazy_agent_selection: true, // Show splash by default
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acp_config_with_agent_profiles() {
        // Should be able to parse config with agent profiles containing env vars
        let toml = r#"
            default_agent = "opencode-local"

            [agents.opencode-local]
            env.LOCAL_ENDPOINT = "http://localhost:11434/v1"
            env.OPENCODE_MODEL = "ollama/llama3.2"

            [agents.claude-proxy]
            extends = "claude"
            env.ANTHROPIC_BASE_URL = "http://localhost:4000"
        "#;

        let config: AcpConfig = toml::from_str(toml).expect("should parse");

        assert_eq!(config.default_agent, Some("opencode-local".to_string()));
        assert_eq!(config.agents.len(), 2);

        let opencode_local = config
            .agents
            .get("opencode-local")
            .expect("should have profile");
        assert_eq!(
            opencode_local.env.get("LOCAL_ENDPOINT"),
            Some(&"http://localhost:11434/v1".to_string())
        );
        assert_eq!(
            opencode_local.env.get("OPENCODE_MODEL"),
            Some(&"ollama/llama3.2".to_string())
        );

        let claude_proxy = config
            .agents
            .get("claude-proxy")
            .expect("should have profile");
        assert_eq!(claude_proxy.extends, Some("claude".to_string()));
        assert_eq!(
            claude_proxy.env.get("ANTHROPIC_BASE_URL"),
            Some(&"http://localhost:4000".to_string())
        );
    }

    #[test]
    fn test_agent_profile_with_custom_command() {
        let toml = r#"
            [agents.custom-agent]
            command = "/usr/local/bin/my-agent"
            args = ["--mode", "acp"]
            env.MY_API_KEY = "secret"
        "#;

        let config: AcpConfig = toml::from_str(toml).expect("should parse");

        let profile = config
            .agents
            .get("custom-agent")
            .expect("should have profile");
        assert_eq!(profile.command, Some("/usr/local/bin/my-agent".to_string()));
        assert_eq!(
            profile.args,
            Some(vec!["--mode".to_string(), "acp".to_string()])
        );
        assert_eq!(profile.env.get("MY_API_KEY"), Some(&"secret".to_string()));
    }

    #[test]
    fn test_acp_config_default_has_empty_agents() {
        let config = AcpConfig::default();
        assert!(config.agents.is_empty());
    }

    #[test]
    fn test_agent_profile_default() {
        let profile = AgentProfile::default();
        assert!(profile.extends.is_none());
        assert!(profile.command.is_none());
        assert!(profile.args.is_none());
        assert!(profile.env.is_empty());
    }

    // =============================================================================
    // Lazy Agent Selection Config Tests (TDD - RED phase)
    // =============================================================================

    #[test]
    fn test_lazy_agent_selection_defaults_to_true() {
        // By default, agent selection should be lazy (show splash, create agent after)
        let config = AcpConfig::default();
        assert!(config.lazy_agent_selection);
    }

    #[test]
    fn test_lazy_agent_selection_can_be_disabled() {
        let toml = r#"
            lazy_agent_selection = false
        "#;

        let config: AcpConfig = toml::from_str(toml).expect("should parse");
        assert!(!config.lazy_agent_selection);
    }

    #[test]
    fn test_lazy_agent_selection_explicit_true() {
        let toml = r#"
            lazy_agent_selection = true
        "#;

        let config: AcpConfig = toml::from_str(toml).expect("should parse");
        assert!(config.lazy_agent_selection);
    }
}
