// crates/crucible-mcp/src/main.rs
use crucible_mcp::{
    CrucibleMcpService, EmbeddingConfig, EmbeddingDatabase, create_provider,
    rune_tools::AsyncToolRegistry,
    obsidian_client::ObsidianClient,
};
use rmcp::{transport::stdio, ServiceExt};
use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

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

    tracing::info!("Starting MCP server with rmcp");
    tracing::info!("  Vault path: {}", vault_path);
    tracing::info!("  Database: {}", db_path);

    // Initialize database (wrap in Arc early for sharing with Rune tools)
    let database = std::sync::Arc::new(EmbeddingDatabase::new(&db_path).await?);
    tracing::info!("Database initialized successfully");

    // Sync metadata from Obsidian for all existing files in database
    tracing::info!("Syncing metadata from Obsidian plugin...");
    match crucible_mcp::tools::sync_metadata_from_obsidian(&database).await {
        Ok((synced_count, errors)) => {
            if errors.is_empty() {
                tracing::info!("Metadata sync successful: {} files updated", synced_count);
            } else {
                tracing::warn!(
                    "Metadata sync completed with errors: {} files updated, {} errors",
                    synced_count,
                    errors.len()
                );
                for error in errors.iter().take(5) {
                    tracing::warn!("  - {}", error);
                }
                if errors.len() > 5 {
                    tracing::warn!("  ... and {} more errors", errors.len() - 5);
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to sync metadata from Obsidian: {}", e);
            tracing::warn!("This may affect search accuracy. Make sure the Obsidian plugin is running.");
        }
    }

    // Initialize Rune tools (optional - gracefully degrade if fails)
    tracing::info!("Initializing Rune tools...");
    let tool_dir = env::var("RUNE_TOOL_DIR")
        .unwrap_or_else(|_| {
            let default_path = format!("{}/tools/examples", vault_path);
            tracing::info!("RUNE_TOOL_DIR not set, using: {}", default_path);
            default_path
        });

    let rune_registry = match ObsidianClient::new() {
        Ok(obsidian_client) => {
            let tool_path = PathBuf::from(&tool_dir);
            match AsyncToolRegistry::new_with_stdlib(
                tool_path.clone(),
                Arc::clone(&database),
                Arc::new(obsidian_client),
            ).await {
                Ok(async_registry) => {
                    tracing::info!("Rune tools loaded successfully:");
                    for tool_meta in async_registry.list_tools().await {
                        tracing::info!("  - {}", tool_meta.name);
                    }
                    Some(Arc::new(async_registry))
                }
                Err(e) => {
                    tracing::warn!("Failed to load Rune tools: {}", e);
                    tracing::warn!("Continuing without Rune tools (native tools still available)");
                    None
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to create Obsidian client for Rune tools: {}", e);
            tracing::warn!("Continuing without Rune tools (native tools still available)");
            None
        }
    };

    // Create the rmcp service (with or without Rune tools)
    let service = if let Some(registry) = rune_registry {
        tracing::info!("Creating service with Rune tools support");
        CrucibleMcpService::with_rune_tools(Arc::clone(&database), provider, registry)
    } else {
        tracing::info!("Creating service without Rune tools (native tools only)");
        CrucibleMcpService::new(Arc::clone(&database), provider)
    };
    tracing::info!("CrucibleMcpService created successfully");

    // Start the MCP server over stdio with rmcp
    tracing::info!("Starting stdio transport...");
    let server = service.serve(stdio()).await?;

    tracing::info!("MCP server started, waiting for requests...");
    server.waiting().await?;

    Ok(())
}
