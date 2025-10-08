// crates/crucible-mcp/src/main.rs
use crucible_mcp::{EmbeddingConfig, McpServer, create_provider};
use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};
use tracing_subscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create log file in same directory as executable
    let log_path = env::current_exe()?.parent().unwrap().join("mcp_server.log");

    let _log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    writeln!(io::stderr(), "[MCP] Logging to: {:?}", log_path)?;

    // Initialize logging to file (stderr goes to Claude Desktop's logs)
    tracing_subscriber::fmt()
        .with_writer(move || {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .unwrap()
        })
        .init();

    // Get vault path from environment variable (required)
    let vault_path = env::var("OBSIDIAN_VAULT_PATH").unwrap_or_else(|_| {
        tracing::warn!("OBSIDIAN_VAULT_PATH not set, using default");
        "C:/Users/Matthew/Documents/test-vault".to_string()
    });

    // Store database in vault's .crucible directory
    let db_path = format!("{}/.crucible/embeddings.db", vault_path);

    // Create .crucible directory if it doesn't exist
    let crucible_dir = format!("{}/.crucible", vault_path);
    std::fs::create_dir_all(&crucible_dir)?;

    // Load embedding configuration from environment
    let embedding_config = EmbeddingConfig::from_env().map_err(|e| {
        tracing::error!("Failed to load embedding configuration: {}", e);
        anyhow::anyhow!("Failed to load embedding configuration: {}", e)
    })?;

    // Log the embedding configuration
    tracing::info!("Loading embedding provider:");
    tracing::info!("  Provider: {:?}", embedding_config.provider);
    tracing::info!("  Model: {}", embedding_config.model);
    tracing::info!("  Endpoint: {}", embedding_config.endpoint);

    // Create the embedding provider
    let provider = create_provider(embedding_config).await.map_err(|e| {
        tracing::error!("Failed to create embedding provider: {}", e);
        anyhow::anyhow!("Failed to create embedding provider: {}", e)
    })?;

    tracing::info!("Embedding provider initialized successfully");

    tracing::info!("Starting MCP server");
    tracing::info!("  Vault path: {}", vault_path);
    tracing::info!("  Database: {}", db_path);

    // Start the MCP server over stdio with the embedding provider
    McpServer::start_stdio(&db_path, provider).await?;

    Ok(())
}
