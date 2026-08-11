//! Read configured MCP servers into display-oriented status entries.
//!
//! Runs inside `session.create`'s setup task, so the TUI receives the list as
//! a `mcp_servers_ready` event instead of computing it locally. The CLI still
//! builds its own `McpServerDisplay` list for the one-shot path
//! (`crates/crucible-cli/src/commands/chat.rs`); that copy is what this
//! replaces.
//!
//! The daemon's MCP configuration is global — it is provided once to
//! `Server::bind_with_plugin_config` (see
//! `crates/crucible-daemon/src/server/mod.rs`) — so the helper takes
//! `Option<&McpConfig>` directly rather than re-reading a per-kiln file.
//!
//! Two projections, differing only in whether live gateway state is available:
//! [`project_mcp_servers`] merges the config with the tool names the gateway
//! currently sees, and [`read_mcp_servers`] is that same function with an empty
//! map — which is exactly the old "not probed, `connected: false`" shape. The
//! merge exists because the TUI used to open its own MCP connections to learn
//! the tool counts (one stdio child process per configured upstream, per `cru
//! chat` launch) while the daemon already held a connected gateway. Publishing
//! it here is what makes that fork deletable.
//!
//! Uses [`crucible_core::types::McpServerInfo`] — the canonical event
//! payload type — not `McpServerDisplay` (the CLI-local display struct).
//! The two have different field names (`tools: Vec<String>` vs
//! `tool_count: usize`); the daemon is the authoritative source so it
//! publishes the richer event-stream shape.

use crucible_core::config::McpConfig;
use crucible_core::types::mcp_status::McpServerInfo;

/// Merge the configured server list with live gateway state.
///
/// The config is authoritative for *which* servers exist — a server that failed
/// to connect must still be listed so the UI can show it greyed out rather than
/// silently omitting it. The gateway is authoritative for `tools` and
/// `connected`. A configured server absent from `live` reports
/// `connected: false` with no tools.
///
/// `prefix` has any trailing underscore stripped, to match how the CLI's
/// `McpServerDisplay` already renders it.
pub fn project_mcp_servers(
    config: Option<&McpConfig>,
    live: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<McpServerInfo> {
    let Some(cfg) = config else {
        return Vec::new();
    };

    cfg.servers
        .iter()
        .map(|s| {
            let tools = live.get(&s.name).cloned().unwrap_or_default();
            McpServerInfo {
                name: s.name.clone(),
                prefix: s.prefix.trim_end_matches('_').to_string(),
                // Reads `tools` before it is moved into the struct, so the
                // field order here is load-bearing.
                connected: !tools.is_empty(),
                tools,
            }
        })
        .collect()
}

/// Project an [`McpConfig`] with no live gateway state available.
///
/// Equivalent to [`project_mcp_servers`] with an empty map: every configured
/// server is listed, none connected, no tools. That is the shape the setup event
/// carried before the gateway was consulted.
pub fn read_mcp_servers(config: Option<&McpConfig>) -> Vec<McpServerInfo> {
    project_mcp_servers(config, &std::collections::HashMap::new())
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

    fn live(pairs: &[(&str, &[&str])]) -> std::collections::HashMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(name, tools)| {
                (
                    (*name).to_string(),
                    tools.iter().map(|t| (*t).to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn project_reports_tools_and_connected_for_a_live_upstream() {
        let cfg = McpConfig {
            servers: vec![stdio_server("github", "gh_")],
        };

        let entries = project_mcp_servers(
            Some(&cfg),
            &live(&[("github", &["gh_search", "gh_issues"])]),
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tools, vec!["gh_search", "gh_issues"]);
        assert!(entries[0].connected);
    }

    #[test]
    fn project_keeps_a_configured_server_that_failed_to_connect() {
        let cfg = McpConfig {
            servers: vec![
                stdio_server("github", "gh_"),
                stdio_server("filesystem", "fs_"),
            ],
        };

        let entries = project_mcp_servers(Some(&cfg), &live(&[("github", &["gh_search"])]));

        // Both listed: omitting the broken one would make it invisible rather
        // than visibly disconnected.
        assert_eq!(entries.len(), 2);
        assert!(entries[0].connected);
        assert_eq!(entries[1].name, "filesystem");
        assert!(!entries[1].connected);
        assert!(entries[1].tools.is_empty());
    }

    #[test]
    fn project_ignores_a_live_upstream_that_is_not_configured() {
        let cfg = McpConfig {
            servers: vec![stdio_server("github", "gh_")],
        };

        // Config is authoritative for membership: a stale gateway entry must
        // not invent a server the operator never configured.
        let entries = project_mcp_servers(
            Some(&cfg),
            &live(&[("github", &["gh_search"]), ("ghost", &["spooky"])]),
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "github");
    }
}
