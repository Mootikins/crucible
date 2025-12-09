//! Gateway configuration for upstream MCP servers

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for the MCP Gateway
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Configured upstream MCP servers
    #[serde(default)]
    pub servers: Vec<UpstreamServerConfig>,
}

/// Configuration for a single upstream MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamServerConfig {
    /// Unique name for this upstream (e.g., "github", "filesystem")
    pub name: String,

    /// Transport type and configuration
    pub transport: TransportType,

    /// Optional prefix to add to tool names (e.g., "gh_")
    #[serde(default)]
    pub prefix: Option<String>,

    /// Whitelist of allowed tools (if None, all tools allowed)
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// Blacklist of blocked tools
    #[serde(default)]
    pub blocked_tools: Option<Vec<String>>,

    /// Whether to auto-reconnect on disconnection
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,
}

fn default_auto_reconnect() -> bool {
    true
}

/// Transport type for connecting to upstream MCP servers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TransportType {
    /// Stdio transport (spawn subprocess)
    Stdio {
        /// Command to execute
        command: String,
        /// Command arguments
        #[serde(default)]
        args: Vec<String>,
        /// Environment variables to set
        #[serde(default)]
        env: HashMap<String, String>,
    },

    /// SSE transport (HTTP+Server-Sent Events)
    Sse {
        /// URL to connect to
        url: String,
        /// Optional authorization header
        #[serde(default)]
        auth_header: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_config_default() {
        let config = GatewayConfig::default();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_gateway_config_parse_toml() {
        let toml_content = r#"
[[servers]]
name = "github"
prefix = "gh_"
auto_reconnect = true

[servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[servers.transport.env]
GITHUB_TOKEN = "test-token"

[[servers]]
name = "filesystem"
prefix = "fs_"

[servers.transport]
type = "sse"
url = "http://localhost:3000/sse"
auth_header = "Bearer secret"
"#;

        let config: GatewayConfig = toml::from_str(toml_content).unwrap();

        assert_eq!(config.servers.len(), 2);

        // Check first server (stdio)
        let gh = &config.servers[0];
        assert_eq!(gh.name, "github");
        assert_eq!(gh.prefix, Some("gh_".to_string()));
        assert!(gh.auto_reconnect);

        match &gh.transport {
            TransportType::Stdio { command, args, env } => {
                assert_eq!(command, "npx");
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], "-y");
                assert_eq!(env.get("GITHUB_TOKEN"), Some(&"test-token".to_string()));
            }
            _ => panic!("Expected Stdio transport"),
        }

        // Check second server (sse)
        let fs = &config.servers[1];
        assert_eq!(fs.name, "filesystem");
        assert_eq!(fs.prefix, Some("fs_".to_string()));

        match &fs.transport {
            TransportType::Sse { url, auth_header } => {
                assert_eq!(url, "http://localhost:3000/sse");
                assert_eq!(auth_header, &Some("Bearer secret".to_string()));
            }
            _ => panic!("Expected SSE transport"),
        }
    }

    #[test]
    fn test_upstream_server_config_with_filters() {
        let toml_content = r#"
name = "github"
prefix = "gh_"
allowed_tools = ["search_*", "get_*"]
blocked_tools = ["delete_*"]

[transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
"#;

        let config: UpstreamServerConfig = toml::from_str(toml_content).unwrap();

        assert_eq!(config.name, "github");
        assert_eq!(config.prefix, Some("gh_".to_string()));
        assert_eq!(
            config.allowed_tools,
            Some(vec!["search_*".to_string(), "get_*".to_string()])
        );
        assert_eq!(config.blocked_tools, Some(vec!["delete_*".to_string()]));
    }

    #[test]
    fn test_transport_stdio_minimal() {
        let toml_content = r#"
type = "stdio"
command = "mcp-server"
"#;

        let transport: TransportType = toml::from_str(toml_content).unwrap();

        match transport {
            TransportType::Stdio { command, args, env } => {
                assert_eq!(command, "mcp-server");
                assert!(args.is_empty());
                assert!(env.is_empty());
            }
            _ => panic!("Expected Stdio transport"),
        }
    }

    #[test]
    fn test_transport_sse_minimal() {
        let toml_content = r#"
type = "sse"
url = "http://localhost:3000/sse"
"#;

        let transport: TransportType = toml::from_str(toml_content).unwrap();

        match transport {
            TransportType::Sse { url, auth_header } => {
                assert_eq!(url, "http://localhost:3000/sse");
                assert!(auth_header.is_none());
            }
            _ => panic!("Expected SSE transport"),
        }
    }

    #[test]
    fn test_upstream_server_config_auto_reconnect_default() {
        let toml_content = r#"
name = "test"

[transport]
type = "stdio"
command = "test"
"#;

        let config: UpstreamServerConfig = toml::from_str(toml_content).unwrap();
        assert!(config.auto_reconnect); // Default should be true
    }

    #[test]
    fn test_gateway_config_serialization() {
        let config = GatewayConfig {
            servers: vec![UpstreamServerConfig {
                name: "test".to_string(),
                transport: TransportType::Stdio {
                    command: "test-cmd".to_string(),
                    args: vec!["arg1".to_string()],
                    env: HashMap::from([("KEY".to_string(), "value".to_string())]),
                },
                prefix: Some("test_".to_string()),
                allowed_tools: None,
                blocked_tools: None,
                auto_reconnect: true,
            }],
        };

        let toml_str = toml::to_string(&config).unwrap();
        let parsed: GatewayConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.servers.len(), 1);
        assert_eq!(parsed.servers[0].name, "test");
    }
}
