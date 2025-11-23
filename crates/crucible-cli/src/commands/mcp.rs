//! MCP Server Command
//!
//! Starts an MCP (Model Context Protocol) server that exposes Crucible's tools
//! via stdio transport. This allows AI agents to discover and use Crucible's
//! note management capabilities through the standard MCP protocol.

use anyhow::Result;
use crucible_tools::CrucibleMcpServer;
use crucible_core::traits::KnowledgeRepository;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_llm::embeddings::CoreProviderAdapter;
use rmcp::ServiceExt;
use std::sync::Arc;
use tracing::{info, debug};

use crate::config::CliConfig;
use crate::core_facade::CrucibleCoreFacade;

/// Execute the MCP server command
///
/// This starts an MCP server that:
/// - Exposes 12 Crucible tools (6 note + 3 search + 3 kiln operations)
/// - Communicates via stdio transport
/// - Blocks until the server is shut down (Ctrl+C or EOF)
///
/// The server is typically invoked by AI agents through the ACP protocol's
/// `mcp_servers` field in NewSessionRequest.
pub async fn execute(config: CliConfig) -> Result<()> {
    info!("Starting Crucible MCP server...");
    debug!("Kiln path: {}", config.kiln.path.display());

    // Initialize core facade
    let core = Arc::new(CrucibleCoreFacade::from_config(config).await?);

    // Get embedding config and create provider
    let embedding_config = core.config().to_embedding_config()?;
    debug!("Creating embedding provider: {}", embedding_config.model_name());
    let llm_provider = crucible_llm::embeddings::create_provider(embedding_config).await?;

    // Wrap in adapter to implement core EmbeddingProvider trait
    let embedding_provider = Arc::new(CoreProviderAdapter::new(llm_provider)) as Arc<dyn EmbeddingProvider>;

    // Create knowledge repository from storage
    let knowledge_repo = Arc::new(core.storage().clone()) as Arc<dyn KnowledgeRepository>;

    // Create MCP server
    let server = CrucibleMcpServer::new(
        core.kiln_root().to_string_lossy().to_string(),
        knowledge_repo,
        embedding_provider,
    );

    info!("MCP server initialized with 12 tools");
    info!("Server ready - waiting for stdio connection...");

    // Serve via stdio (blocks until shutdown)
    let _service = server.serve((tokio::io::stdin(), tokio::io::stdout())).await?;

    // Wait indefinitely - the service will handle shutdown on EOF or Ctrl+C
    info!("MCP server terminated");

    Ok(())
}
