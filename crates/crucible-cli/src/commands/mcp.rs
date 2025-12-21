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
use crucible_rune::RuneDiscoveryConfig;
use crucible_tools::{ExtendedMcpServer, ExtendedMcpService};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};

use crate::config::CliConfig;
use crate::core_facade::KilnContext;

/// MCP server command arguments
#[derive(Parser, Debug)]
pub struct McpArgs {
    /// Use stdio transport instead of SSE (default: SSE)
    #[arg(long)]
    pub stdio: bool,

    /// SSE server port (default: 3847)
    #[arg(long, default_value = "3847")]
    pub port: u16,

    /// Override kiln path (default: CRUCIBLE_KILN_PATH)
    #[arg(long)]
    pub kiln_path: Option<PathBuf>,

    /// Override justfile directory (default: PWD)
    #[arg(long)]
    pub just_dir: Option<PathBuf>,

    /// Disable Just tools
    #[arg(long)]
    pub no_just: bool,

    /// Disable Rune tools
    #[arg(long)]
    pub no_rune: bool,

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
            no_rune: false,
            log_file: None,
        }
    }
}

/// Execute the MCP server command
///
/// This starts an MCP server that:
/// - Exposes 12 Crucible kiln tools (6 note + 3 search + 3 kiln operations)
/// - Exposes Just recipe tools (if justfile exists and --no-just not set)
/// - Exposes Rune script tools (from ~/.crucible/runes/ and {kiln}/runes/)
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
    let core = Arc::new(KilnContext::from_config(config.clone()).await?);

    // Get embedding config and create provider
    let embedding_config = core.config().embedding.to_provider_config();
    let llm_provider = crucible_llm::embeddings::create_provider(embedding_config).await?;

    // Wrap in adapter to implement core EmbeddingProvider trait
    let embedding_provider =
        Arc::new(CoreProviderAdapter::new(llm_provider)) as Arc<dyn EmbeddingProvider>;

    // Create knowledge repository from storage
    let knowledge_repo = core.storage().as_knowledge_repository();

    // Determine Just directory
    let just_dir = args
        .just_dir
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Create plugin discovery config
    // Load order matches agent discovery (later sources override earlier by tool name):
    // 1. ~/.config/crucible/plugins/ - Global personal plugins
    // 2. KILN_DIR/.crucible/plugins/ - Kiln-specific personal plugins (gitignored)
    // 3. KILN_DIR/plugins/ - Kiln-tracked shared plugins (versioned)
    let rune_config = if args.no_rune {
        RuneDiscoveryConfig::default()
    } else {
        let mut dirs = vec![];

        // 1. Global plugins directory: ~/.config/crucible/plugins/
        if let Some(config_dir) = dirs::config_dir() {
            let global_plugins = config_dir.join("crucible").join("plugins");
            if global_plugins.exists() {
                dirs.push(global_plugins);
            }
        }

        // 2. Kiln hidden: KILN_DIR/.crucible/plugins/
        let kiln_hidden_plugins = core.kiln_root().join(".crucible").join("plugins");
        if kiln_hidden_plugins.exists() {
            dirs.push(kiln_hidden_plugins);
        }

        // 3. Kiln visible: KILN_DIR/plugins/
        let kiln_plugins = core.kiln_root().join("plugins");
        if kiln_plugins.exists() {
            dirs.push(kiln_plugins);
        }

        RuneDiscoveryConfig {
            tool_directories: dirs,
            extensions: vec!["rn".to_string(), "rune".to_string()],
            recursive: true,
        }
    };

    // Create extended MCP server
    let server = if args.no_just && args.no_rune {
        // Kiln-only mode
        ExtendedMcpServer::kiln_only(
            core.kiln_root().to_string_lossy().to_string(),
            knowledge_repo,
            embedding_provider,
        )
    } else {
        // Full mode with Just and/or Rune
        ExtendedMcpServer::new(
            core.kiln_root().to_string_lossy().to_string(),
            knowledge_repo,
            embedding_provider,
            &just_dir,
            rune_config,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create ExtendedMcpServer: {}", e))?
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
