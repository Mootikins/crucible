// crates/crucible-mcp/src/main.rs
use crucible_mcp::McpServer;
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

    tracing::info!("Starting MCP server");
    tracing::info!("  Vault path: {}", vault_path);
    tracing::info!("  Database: {}", db_path);

    // Start the MCP server over stdio
    McpServer::start_stdio(&db_path).await?;

    Ok(())
}
