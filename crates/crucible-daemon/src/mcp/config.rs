//! Read configured MCP servers into display-oriented status entries.
//!
//! Mirrors the CLI's one-shot MCP list produced in
//! `crates/crucible-cli/src/commands/chat.rs` (at the `McpServerDisplay`
//! construction site). Runs inside `session.create`'s setup task (Task 1.2f)
//! so the TUI can receive the list as an event instead of computing it
//! locally. The CLI copy stays until Task 1.3.
//!
//! The daemon's MCP configuration is global — it is provided once to
//! `Server::bind_with_plugin_config` (see
//! `crates/crucible-daemon/src/server/mod.rs`) — so the helper takes
//! `Option<&McpConfig>` directly rather than re-reading a per-kiln file.
//! Upstream servers are not probed here; `connected` is reported as `false`
//! and the tool list is empty at setup time. Live connection state is
//! surfaced later through the existing MCP gateway RPCs.
//!
//! Uses [`crucible_core::types::McpServerInfo`] — the canonical event
//! payload type — not `McpServerDisplay` (the CLI-local display struct).
//! The two have different field names (`tools: Vec<String>` vs
//! `tool_count: usize`); the daemon is the authoritative source so it
//! publishes the richer event-stream shape.

use crucible_core::config::McpConfig;
use crucible_core::types::mcp_status::McpServerInfo;

/// Project an [`McpConfig`] into a list of [`McpServerInfo`] entries.
///
/// Returns an empty list when no MCP config is configured. `prefix` has any
/// trailing underscore stripped to match how CLI `McpServerDisplay` already
/// renders it.
pub fn read_mcp_servers(config: Option<&McpConfig>) -> Vec<McpServerInfo> {
    let Some(cfg) = config else {
        return Vec::new();
    };

    cfg.servers
        .iter()
        .map(|s| McpServerInfo {
            name: s.name.clone(),
            prefix: s.prefix.trim_end_matches('_').to_string(),
            tools: Vec::new(),
            connected: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::mcp::{TransportType, UpstreamServerConfig};

    fn stdio_server(name: &str, prefix: &str) -> UpstreamServerConfig {
        UpstreamServerConfig {
            name: name.to_string(),
            transport: TransportType::Stdio {
                command: "echo".to_string(),
                args: Vec::new(),
                env: Default::default(),
            },
            prefix: prefix.to_string(),
            allowed_tools: None,
            blocked_tools: None,
            auto_reconnect: false,
            timeout_secs: 30,
        }
    }

    #[test]
    fn read_mcp_servers_returns_empty_when_config_none() {
        let entries = read_mcp_servers(None);
        assert!(entries.is_empty());
    }

    #[test]
    fn read_mcp_servers_returns_empty_when_servers_empty() {
        let cfg = McpConfig {
            servers: Vec::new(),
        };
        let entries = read_mcp_servers(Some(&cfg));
        assert!(entries.is_empty());
    }

    #[test]
    fn read_mcp_servers_projects_all_configured_servers() {
        let cfg = McpConfig {
            servers: vec![
                stdio_server("github", "gh_"),
                stdio_server("filesystem", "fs_"),
            ],
        };

        let entries = read_mcp_servers(Some(&cfg));

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "github");
        assert_eq!(entries[0].prefix, "gh");
        assert!(entries[0].tools.is_empty());
        assert!(!entries[0].connected);
        assert_eq!(entries[1].name, "filesystem");
        assert_eq!(entries[1].prefix, "fs");
    }

    #[test]
    fn read_mcp_servers_strips_trailing_underscore_from_prefix() {
        let cfg = McpConfig {
            servers: vec![stdio_server("example", "ex_")],
        };

        let entries = read_mcp_servers(Some(&cfg));

        assert_eq!(entries[0].prefix, "ex");
    }

    #[test]
    fn read_mcp_servers_preserves_prefix_without_underscore() {
        let cfg = McpConfig {
            servers: vec![stdio_server("example", "exact")],
        };

        let entries = read_mcp_servers(Some(&cfg));

        assert_eq!(entries[0].prefix, "exact");
    }
}
