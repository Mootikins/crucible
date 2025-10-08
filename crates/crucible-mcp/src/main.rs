// crates/crucible-mcp/src/main.rs
use crucible_mcp::McpServer;
use std::env;
use tracing_subscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().collect();
    let db_path = args.get(1).map(|s| s.as_str()).unwrap_or("crucible.db");

    tracing::info!("Starting MCP server with database: {}", db_path);

    // Start the MCP server over stdio
    McpServer::start_stdio(db_path).await?;

    Ok(())
}
