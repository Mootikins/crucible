//! MCP Server Command
//!
//! Starts an MCP (Model Context Protocol) server that exposes Crucible's tools.
//! Supports both SSE (default) and stdio transports.
//!
//! ## Transport Options
//!
//! - **SSE (default)**: HTTP-based Server-Sent Events on port 3847
//!   - Logs to stdout (visible in terminal)
//!   - Multiple clients can connect
//!   - Easy to debug with browser DevTools
//!
//! - **Stdio**: Traditional stdin/stdout transport
//!   - Logs to file (`~/.crucible/logs/mcp.log`)
//!   - Single client connection
//!   - Used by AI agents via subprocess

use anyhow::Result;
use clap::Parser;
use crucible_rpc::DaemonClient;
use std::path::PathBuf;
use tracing::info;

use crate::config::CliConfig;

/// MCP server command arguments
#[derive(Parser, Debug)]
pub struct McpArgs {
    /// Use stdio transport instead of SSE (default: SSE)
    #[arg(long)]
    pub stdio: bool,

    /// SSE server port (default: 3847)
    #[arg(long, default_value = "3847")]
    pub port: u16,

    /// Override kiln path
    #[arg(long)]
    pub kiln_path: Option<PathBuf>,

    /// Override justfile directory (default: PWD)
    #[arg(long)]
    pub just_dir: Option<PathBuf>,

    /// Disable Just tools
    #[arg(long)]
    pub no_just: bool,

    /// Log file path (default: ~/.crucible/logs/mcp.log for stdio mode)
    #[arg(long)]
    pub log_file: Option<PathBuf>,
}

impl Default for McpArgs {
    fn default() -> Self {
        Self {
            stdio: false,
            port: 3847,
            kiln_path: None,
            just_dir: None,
            no_just: false,
            log_file: None,
        }
    }
}

/// Execute the MCP server command
///
/// This starts an MCP server that:
/// - Exposes 12 Crucible kiln tools (6 note + 3 search + 3 kiln operations)
/// - Exposes Just recipe tools (if justfile exists and --no-just not set)
/// - Exposes Lua script tools (from ~/.crucible/plugins/ and {kiln}/plugins/)
/// - All responses are TOON-formatted for token efficiency
///
/// ## Transport Modes
///
/// - **SSE (default)**: Starts HTTP server on specified port
/// - **Stdio**: Uses stdin/stdout, logs to file
pub async fn execute(config: CliConfig, args: McpArgs) -> Result<()> {
    // Determine kiln path (override or config default)
    let kiln_path = args.kiln_path.unwrap_or(config.kiln_path.clone());
    let kiln_path_str = kiln_path.to_string_lossy().to_string();

    // Determine transport
    let transport = if args.stdio {
        Some("stdio")
    } else {
        Some("sse")
    };

    // Determine Just directory
    let just_dir = args
        .just_dir
        .map(|d| d.to_string_lossy().to_string())
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
        });

    info!("Starting Crucible MCP server...");
    info!("Kiln path: {}", kiln_path.display());
    info!("Transport: {}", if args.stdio { "stdio" } else { "SSE" });

    // Connect to daemon
    let client = DaemonClient::connect_or_start()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to daemon: {}", e))?;

    // Start MCP server via daemon RPC
    client
        .mcp_start(
            &kiln_path_str,
            transport,
            Some(args.port),
            args.no_just,
            just_dir.as_deref(),
        )
        .await?;

    // Display server info and wait
    if args.stdio {
        info!("Server ready - waiting for stdio connection...");
    } else {
        let addr = format!("127.0.0.1:{}", args.port);
        info!("Starting SSE server on http://{}", addr);
        info!("SSE endpoint: http://{}/sse", addr);
        info!("Message endpoint: http://{}/message", addr);
        info!("MCP server running. Press Ctrl+C to stop.");
    }

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;

    // Stop MCP server on shutdown
    info!("Shutdown signal received");
    let _ = client.mcp_stop().await;

    info!("MCP server terminated");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_args_default() {
        let args = McpArgs::default();
        assert!(!args.stdio);
        assert_eq!(args.port, 3847);
        assert!(args.kiln_path.is_none());
        assert!(args.just_dir.is_none());
        assert!(!args.no_just);
        assert!(args.log_file.is_none());
    }

    #[test]
    fn test_mcp_args_with_stdio() {
        let args = McpArgs {
            stdio: true,
            ..Default::default()
        };
        assert!(args.stdio);
        assert_eq!(args.port, 3847);
    }

    #[test]
    fn test_mcp_args_with_custom_port() {
        let args = McpArgs {
            port: 8080,
            ..Default::default()
        };
        assert_eq!(args.port, 8080);
    }

    #[test]
    fn test_mcp_args_with_kiln_path() {
        let args = McpArgs {
            kiln_path: Some(PathBuf::from("/custom/kiln")),
            ..Default::default()
        };
        assert_eq!(args.kiln_path, Some(PathBuf::from("/custom/kiln")));
    }

    #[test]
    fn test_mcp_args_no_just() {
        let args = McpArgs {
            no_just: true,
            ..Default::default()
        };
        assert!(args.no_just);
    }
}
