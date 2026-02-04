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
use crucible_core::enrichment::EmbeddingProvider;
use crucible_llm::embeddings::CoreProviderAdapter;
use crucible_tools::mcp_gateway::McpGatewayManager;
use crucible_tools::{ExtendedMcpServer, ExtendedMcpService};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::config::CliConfig;
use crate::core_facade::KilnContext;
#[cfg(not(feature = "storage-surrealdb"))]
use crate::factories;

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
    // Apply kiln_path override if provided
    let config = if let Some(kiln_path) = args.kiln_path {
        CliConfig {
            kiln_path,
            ..config
        }
    } else {
        config
    };

    info!("Starting Crucible MCP server...");
    debug!("Kiln path: {}", config.kiln_path.display());
    debug!("Transport: {}", if args.stdio { "stdio" } else { "SSE" });

    // Initialize core facade
    #[cfg(feature = "storage-surrealdb")]
    let core = Arc::new(KilnContext::from_config(config.clone()).await?);
    #[cfg(not(feature = "storage-surrealdb"))]
    let core = {
        let storage_handle = factories::get_storage(&config).await?;
        Arc::new(KilnContext::from_storage_handle(
            storage_handle,
            config.clone(),
        ))
    };

    // Get embedding config and create provider
    let embedding_config = core.config().embedding.to_provider_config();
    let llm_provider = crucible_llm::embeddings::create_provider(embedding_config).await?;

    // Wrap in adapter to implement core EmbeddingProvider trait
    let embedding_provider =
        Arc::new(CoreProviderAdapter::new(llm_provider)) as Arc<dyn EmbeddingProvider>;

    // Create knowledge repository from storage
    let knowledge_repo = core
        .storage_handle()
        .as_knowledge_repository()
        .ok_or_else(|| anyhow::anyhow!("MCP server requires SurrealDB storage"))?;

    // Determine Just directory
    let just_dir = args
        .just_dir
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Create extended MCP server
    let server = if args.no_just {
        // Kiln-only mode
        ExtendedMcpServer::kiln_only(
            core.kiln_root().to_string_lossy().to_string(),
            knowledge_repo,
            embedding_provider,
        )
    } else {
        // Full mode with Just
        ExtendedMcpServer::new(
            core.kiln_root().to_string_lossy().to_string(),
            knowledge_repo,
            embedding_provider,
            &just_dir,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create ExtendedMcpServer: {}", e))?
    };

    let server = if let Some(mcp_config) = &config.mcp {
        if !mcp_config.servers.is_empty() {
            info!(
                "Initializing MCP gateway with {} upstream servers",
                mcp_config.servers.len()
            );
            match McpGatewayManager::from_config(mcp_config).await {
                Ok(gateway) => {
                    let gateway_tools = gateway.tool_count();
                    info!(
                        "Gateway loaded {} tools from upstream servers",
                        gateway_tools
                    );
                    server.with_gateway(gateway)
                }
                Err(e) => {
                    warn!(
                        "Failed to initialize gateway: {}. Continuing without gateway.",
                        e
                    );
                    server
                }
            }
        } else {
            server
        }
    } else {
        server
    };

    let tool_count = server.tool_count().await;
    info!("MCP server initialized with {} tools", tool_count);

    // Wrap in service for MCP protocol handling
    let service = ExtendedMcpService::new(server).await;

    // Serve based on transport mode
    if args.stdio {
        info!("Server ready - waiting for stdio connection...");
        service.serve_stdio().await?;
    } else {
        // SSE mode
        let addr: SocketAddr = format!("127.0.0.1:{}", args.port).parse()?;
        info!("Starting SSE server on http://{}", addr);
        info!("SSE endpoint: http://{}/sse", addr);
        info!("Message endpoint: http://{}/message", addr);
        info!("MCP server running. Press Ctrl+C to stop.");

        service.serve_sse(addr).await?;
        info!("Shutdown signal received");
    }

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
