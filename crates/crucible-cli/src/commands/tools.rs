use anyhow::Result;
use crucible_rpc::DaemonClient;

use crate::cli::ToolsCommands;
use crate::config::CliConfig;

pub async fn execute(_config: CliConfig, command: ToolsCommands) -> Result<()> {
    match command {
        ToolsCommands::List { permissions } => list(permissions).await,
    }
}

async fn list(permissions: bool) -> Result<()> {
    let client = DaemonClient::connect_or_start()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to daemon: {}", e))?;

    if permissions {
        list_permissions(&client).await
    } else {
        list_normal(&client).await
    }
}

async fn list_normal(_client: &DaemonClient) -> Result<()> {
    println!("Tools list (normal format)");
    Ok(())
}

async fn list_permissions(_client: &DaemonClient) -> Result<()> {
    println!("# Add these to [permissions].allow in crucible.toml");
    println!();

    println!("# Built-in Tools");
    println!("read:*");
    println!("edit:*");
    println!("write:*");
    println!("bash:*");
    println!("delete:*");
    println!();
    println!("# MCP Server tools will appear here when a chat session is running");
    println!("# Start a chat session first to discover tools: cru chat");

    Ok(())
}
