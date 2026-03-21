//! Core configuration struct and validation methods.

use crate::components::{
    AcpConfig, ChatConfig, CliConfig, ContextConfig, DiscoveryPathsConfig, GatewayConfig,
    HandlersConfig, LlmConfig, McpConfig, PermissionConfig,
};
use crate::includes::IncludeConfig;
use crate::{EnrichmentConfig, ProfileConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::serde_helpers::default_true;

use super::errors::{ConfigError, ConfigValidationError};
use super::provider::EffectiveLlmConfig;
use super::server::{LoggingConfig, ScmConfig, ServerConfig, WebConfig};

/// A declarative schedule entry from `[[schedules]]` in config.
///
/// Each entry runs a Lua snippet at a fixed interval via `cru.schedule`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleEntry {
    /// Human-readable name for logging.
    pub name: String,
    /// Interval string: "1h", "30m", "5s", "1d", or bare seconds.
    pub every: String,
    /// Lua code to execute, optionally prefixed with "lua:".
    pub action: String,
    /// Whether this schedule is active (default: true).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Parse a human-readable duration string like "1h", "30m", "5s", "1d".
///
/// Supports suffixes `d` (days), `h` (hours), `m` (minutes), `s` (seconds),
/// or a bare number treated as seconds.
pub fn parse_duration_string(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix('d') {
        return n
            .parse::<u64>()
            .ok()
            .map(|n| std::time::Duration::from_secs(n * 86400));
    }
    if let Some(n) = s.strip_suffix('h') {
        return n
            .parse::<u64>()
            .ok()
            .map(|n| std::time::Duration::from_secs(n * 3600));
    }
    if let Some(n) = s.strip_suffix('m') {
        return n
            .parse::<u64>()
            .ok()
            .map(|n| std::time::Duration::from_secs(n * 60));
    }
    if let Some(n) = s.strip_suffix('s') {
        return n.parse::<u64>().ok().map(std::time::Duration::from_secs);
    }
    s.parse::<u64>().ok().map(std::time::Duration::from_secs)
}

#[cfg(test)]
mod duration_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parse_seconds_suffix() {
        assert_eq!(parse_duration_string("5s"), Some(Duration::from_secs(5)));
    }

    #[test]
    fn parse_minutes_suffix() {
        assert_eq!(
            parse_duration_string("30m"),
            Some(Duration::from_secs(1800))
        );
    }

    #[test]
    fn parse_hours_suffix() {
        assert_eq!(parse_duration_string("1h"), Some(Duration::from_secs(3600)));
    }

    #[test]
    fn parse_days_suffix() {
        assert_eq!(
            parse_duration_string("1d"),
            Some(Duration::from_secs(86400))
        );
    }

    #[test]
    fn parse_bare_number_as_seconds() {
        assert_eq!(parse_duration_string("120"), Some(Duration::from_secs(120)));
    }

    #[test]
    fn parse_with_whitespace() {
        assert_eq!(
            parse_duration_string("  2h  "),
            Some(Duration::from_secs(7200))
        );
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert_eq!(parse_duration_string("abc"), None);
        assert_eq!(parse_duration_string(""), None);
        assert_eq!(parse_duration_string("5x"), None);
    }

    #[test]
    fn parse_zero() {
        assert_eq!(parse_duration_string("0s"), Some(Duration::from_secs(0)));
        assert_eq!(parse_duration_string("0"), Some(Duration::from_secs(0)));
    }
}

/// Main configuration structure for the Crucible system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Include configuration for external files.
    ///
    /// Allows separating configuration into multiple files:
    /// ```toml
    /// [include]
    /// gateway = "mcps.toml"  # MCP server configurations
    /// discovery = "discovery.toml"  # Discovery paths
    /// ```
    ///
    /// Paths are relative to the main config file's directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<IncludeConfig>,

    /// Current active profile name.
    #[serde(default)]
    pub profile: Option<String>,

    /// Available profiles configuration.
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,

    /// Enrichment configuration (includes embedding provider).
    pub enrichment: Option<EnrichmentConfig>,

    /// CLI configuration.
    #[serde(default)]
    pub cli: Option<CliConfig>,

    /// ACP (Agent Client Protocol) configuration.
    #[serde(default)]
    pub acp: Option<AcpConfig>,

    /// Chat configuration.
    #[serde(default)]
    pub chat: Option<ChatConfig>,

    /// LLM provider configuration with named instances.
    #[serde(default)]
    pub llm: Option<LlmConfig>,

    /// Server configuration.
    pub server: Option<ServerConfig>,

    /// Web UI server configuration.
    #[serde(default)]
    pub web: Option<WebConfig>,

    /// SCM (git) integration configuration.
    #[serde(default)]
    pub scm: Option<ScmConfig>,

    /// Logging configuration.
    pub logging: Option<LoggingConfig>,

    /// Discovery paths configuration.
    #[serde(default)]
    pub discovery: Option<DiscoveryPathsConfig>,

    /// Gateway configuration for upstream MCP servers (legacy alias for mcp).
    #[serde(default)]
    pub gateway: Option<GatewayConfig>,

    /// MCP upstream server configuration.
    #[serde(default)]
    pub mcp: Option<McpConfig>,

    /// Handlers configuration.
    #[serde(default)]
    pub handlers: Option<HandlersConfig>,

    /// Context configuration (project rules, etc.)
    #[serde(default)]
    pub context: Option<ContextConfig>,

    /// Permission configuration for tool access control.
    #[serde(default)]
    pub permissions: Option<PermissionConfig>,

    /// Declarative schedules that run Lua snippets at fixed intervals.
    #[serde(default)]
    pub schedules: Vec<ScheduleEntry>,

    /// Custom configuration values.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            include: None,
            profile: Some("default".to_string()),
            profiles: HashMap::from([("default".to_string(), ProfileConfig::default())]),
            enrichment: None,
            cli: Some(CliConfig::default()),
            acp: Some(AcpConfig::default()),
            chat: Some(ChatConfig::default()),
            llm: None,
            server: None,
            web: None,
            scm: None,
            logging: None,
            discovery: None,
            gateway: None,
            mcp: None,
            handlers: None,
            context: None,
            permissions: None,
            schedules: Vec::new(),
            custom: HashMap::new(),
        }
    }
}

impl Config {
    /// Create a new empty configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the active profile configuration.
    pub fn active_profile(&self) -> Result<&ProfileConfig, ConfigError> {
        let profile_name = self.profile.as_deref().unwrap_or("default");
        self.profiles
            .get(profile_name)
            .ok_or_else(|| ConfigError::MissingValue {
                field: format!("profile.{}", profile_name),
            })
    }

    /// Get the effective enrichment configuration.
    pub fn enrichment_config(&self) -> Result<EnrichmentConfig, ConfigError> {
        if let Some(config) = &self.enrichment {
            return Ok(config.clone());
        }

        // Fall back to profile configuration
        let profile = self.active_profile()?;
        if let Some(config) = &profile.enrichment {
            return Ok(config.clone());
        }

        Err(ConfigError::MissingValue {
            field: "enrichment".to_string(),
        })
    }

    /// Get the effective server configuration.
    pub fn server(&self) -> Result<ServerConfig, ConfigError> {
        if let Some(server) = &self.server {
            return Ok(server.clone());
        }

        // Fall back to profile configuration
        let profile = self.active_profile()?;
        if let Some(server) = &profile.server {
            return Ok(server.clone());
        }

        // Return default server configuration
        Ok(ServerConfig::default())
    }

    /// Get the effective logging configuration.
    pub fn logging(&self) -> LoggingConfig {
        if let Some(logging) = &self.logging {
            return logging.clone();
        }

        // Fall back to profile configuration
        if let Ok(profile) = self.active_profile() {
            if let Some(logging) = &profile.logging {
                return logging.clone();
            }
        }

        // Return default logging configuration
        LoggingConfig::default()
    }

    /// Get a custom configuration value.
    pub fn get<T>(&self, key: &str) -> Result<Option<T>, ConfigError>
    where
        T: for<'de> Deserialize<'de>,
    {
        if let Some(value) = self.custom.get(key) {
            let typed = serde_json::from_value(value.clone())?;
            Ok(Some(typed))
        } else {
            Ok(None)
        }
    }

    /// Set a custom configuration value.
    pub fn set<T>(&mut self, key: String, value: T) -> Result<(), ConfigError>
    where
        T: Serialize,
    {
        let json_value = serde_json::to_value(value)?;
        self.custom.insert(key, json_value);
        Ok(())
    }

    /// Get the kiln path from configuration.
    pub fn kiln_path(&self) -> Result<String, ConfigError> {
        self.get::<String>("kiln_path")?
            .ok_or_else(|| ConfigError::MissingValue {
                field: "kiln_path".to_string(),
            })
    }

    /// Get the kiln path or return None if not set.
    pub fn kiln_path_opt(&self) -> Option<String> {
        self.get::<String>("kiln_path").ok().flatten()
    }

    /// Get the effective CLI configuration.
    pub fn cli_config(&self) -> Result<CliConfig, ConfigError> {
        if let Some(config) = &self.cli {
            return Ok(config.clone());
        }

        // Fall back to default
        Ok(CliConfig::default())
    }

    /// Get the effective ACP configuration.
    pub fn acp_config(&self) -> Result<AcpConfig, ConfigError> {
        if let Some(config) = &self.acp {
            return Ok(config.clone());
        }

        // Fall back to default
        Ok(AcpConfig::default())
    }

    /// Get the effective chat configuration.
    pub fn chat_config(&self) -> Result<ChatConfig, ConfigError> {
        if let Some(config) = &self.chat {
            return Ok(config.clone());
        }

        // Fall back to default
        Ok(ChatConfig::default())
    }

    /// Get the effective discovery configuration.
    pub fn discovery_config(&self) -> Option<&DiscoveryPathsConfig> {
        self.discovery.as_ref()
    }

    /// Get the effective gateway configuration.
    pub fn gateway_config(&self) -> Option<&GatewayConfig> {
        self.gateway.as_ref()
    }

    /// Get the effective MCP configuration.
    pub fn mcp_config(&self) -> Option<&McpConfig> {
        self.mcp.as_ref()
    }

    /// Get the effective handlers configuration.
    pub fn handlers_config(&self) -> Option<&HandlersConfig> {
        self.handlers.as_ref()
    }

    /// Get the effective context configuration.
    pub fn context_config(&self) -> Option<&ContextConfig> {
        self.context.as_ref()
    }

    /// Get the effective LLM configuration.
    pub fn llm_config(&self) -> Option<&LlmConfig> {
        self.llm.as_ref()
    }

    /// Get the effective LLM provider for chat.
    pub fn effective_llm_provider(&self) -> Result<EffectiveLlmConfig, ConfigError> {
        if let Some(llm) = &self.llm {
            if let Some((key, provider)) = llm.default_provider() {
                return Ok(EffectiveLlmConfig {
                    key: key.clone(),
                    provider_type: provider.provider_type,
                    endpoint: provider.endpoint(),
                    model: provider.model(),
                    temperature: provider.temperature(),
                    max_tokens: provider.max_tokens(),
                    timeout_secs: provider.timeout_secs(),
                    api_key: provider.api_key(),
                });
            }
        }

        Err(ConfigError::MissingValue {
            field: "llm.default".to_string(),
        })
    }

    /// Validate gateway configuration
    pub fn validate_gateway(&self) -> Result<(), ConfigValidationError> {
        if let Some(gateway) = &self.gateway {
            let mut errors = Vec::new();

            for server in &gateway.servers {
                // Validate server name is not empty
                if server.name.is_empty() {
                    errors.push("Gateway server name cannot be empty".to_string());
                }

                match &server.transport {
                    crate::components::gateway::TransportType::Stdio { command, .. } => {
                        if command.is_empty() {
                            errors.push(format!(
                                "Gateway server '{}': stdio command cannot be empty",
                                server.name
                            ));
                        }
                    }
                    crate::components::gateway::TransportType::Sse { url, .. } => {
                        if url.is_empty() {
                            errors.push(format!(
                                "Gateway server '{}': SSE url cannot be empty",
                                server.name
                            ));
                        }
                        if !url.starts_with("http://") && !url.starts_with("https://") {
                            errors.push(format!(
                                "Gateway server '{}': SSE url must start with http:// or https://",
                                server.name
                            ));
                        }
                    }
                }

                // Validate prefix/suffix if present
                if let Some(prefix) = &server.prefix {
                    if prefix.is_empty() {
                        errors.push(format!(
                            "Gateway server '{}': prefix cannot be empty string",
                            server.name
                        ));
                    }
                }
            }

            if !errors.is_empty() {
                return Err(ConfigValidationError::Multiple { errors });
            }
        }

        Ok(())
    }

    /// Validate handlers configuration (checks pattern validity)
    pub fn validate_handlers(&self) -> Result<(), ConfigValidationError> {
        if let Some(handlers) = &self.handlers {
            let mut errors = Vec::new();

            // Validate patterns are valid glob patterns
            // For now, just check they're not empty if present
            let mut check_pattern = |name: &str, pattern: &Option<String>| {
                if let Some(p) = pattern {
                    if p.is_empty() {
                        errors.push(format!(
                            "Handler '{}': pattern cannot be empty string",
                            name
                        ));
                    }
                }
            };

            check_pattern("test_filter", &handlers.builtin.test_filter.pattern);
            check_pattern("toon_transform", &handlers.builtin.toon_transform.pattern);
            check_pattern(
                "recipe_enrichment",
                &handlers.builtin.recipe_enrichment.pattern,
            );
            check_pattern("tool_selector", &handlers.builtin.tool_selector.pattern);

            if !errors.is_empty() {
                return Err(ConfigValidationError::Multiple { errors });
            }
        }

        Ok(())
    }

    /// Validate discovery configuration (checks path format validity)
    pub fn validate_discovery(&self) -> Result<(), ConfigValidationError> {
        if let Some(discovery) = &self.discovery {
            let mut errors = Vec::new();

            // Validate all type configs
            for (type_name, type_config) in &discovery.type_configs {
                for (idx, path) in type_config.additional_paths.iter().enumerate() {
                    let path_str = path.to_string_lossy();
                    // Check for empty paths
                    if path_str.trim().is_empty() {
                        errors.push(format!(
                            "Discovery '{}': additional_paths[{}] cannot be empty",
                            type_name, idx
                        ));
                    }
                }
            }

            // Validate flat format configs
            let mut check_type_config =
                |type_name: &str, config: &crate::components::TypeDiscoveryConfig| {
                    for (idx, path) in config.additional_paths.iter().enumerate() {
                        let path_str = path.to_string_lossy();
                        if path_str.trim().is_empty() {
                            errors.push(format!(
                                "Discovery '{}': additional_paths[{}] cannot be empty",
                                type_name, idx
                            ));
                        }
                    }
                };

            if let Some(handlers) = &discovery.handlers {
                check_type_config("handlers", handlers);
            }
            if let Some(tools) = &discovery.tools {
                check_type_config("tools", tools);
            }
            if let Some(events) = &discovery.events {
                check_type_config("events", events);
            }

            if !errors.is_empty() {
                return Err(ConfigValidationError::Multiple { errors });
            }
        }

        Ok(())
    }

    /// Validate all configuration sections
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        self.validate_gateway()?;
        self.validate_handlers()?;
        self.validate_discovery()?;
        Ok(())
    }
}

/// Standalone config for `~/.config/crucible/plugins.toml`.
///
/// This is NOT part of `Config` (crucible.toml). It lives in a separate file
/// so users can declare git-hosted plugins independently of the main config.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginsConfig {
    /// Declared plugins to bootstrap on daemon startup.
    #[serde(default)]
    pub plugin: Vec<PluginEntry>,
}

/// A single plugin declaration in `plugins.toml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginEntry {
    /// Git URL or GitHub shorthand (e.g. "user/repo").
    pub url: String,
    /// Branch to clone. Defaults to the repo's default branch.
    #[serde(default)]
    pub branch: Option<String>,
    /// Pin to a specific tag or commit hash after cloning.
    #[serde(default)]
    pub pin: Option<String>,
    /// Whether this plugin is enabled. Disabled plugins are skipped during bootstrap.
    #[serde(default = "default_true")]
    pub enabled: bool,
}
