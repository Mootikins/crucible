use anyhow::Result;
use crucible_daemon::DaemonClient;
use serde::Serialize;

use crate::cli::ToolsCommands;
use crate::common::daemon_client;
use crate::config::CliConfig;
use crate::formatting::OutputFormat;

#[derive(Debug, Serialize)]
pub struct ToolOutput {
    pub name: String,
}

pub async fn execute(_config: CliConfig, command: ToolsCommands) -> Result<()> {
    match command {
        ToolsCommands::List {
            permissions,
            format,
        } => list(permissions, &format).await,
    }
}

async fn list(permissions: bool, format: &str) -> Result<()> {
    let client = daemon_client().await?;

    if permissions {
        list_permissions(&client).await
    } else {
        list_normal(&client, format).await
    }
}

async fn list_normal(_client: &DaemonClient, format: &str) -> Result<()> {
    let output_format = OutputFormat::from(format);

    match output_format {
        OutputFormat::Json => {
            let tools = vec![
                ToolOutput {
                    name: "read".to_string(),
                },
                ToolOutput {
                    name: "edit".to_string(),
                },
                ToolOutput {
                    name: "write".to_string(),
                },
                ToolOutput {
                    name: "bash".to_string(),
                },
                ToolOutput {
                    name: "delete".to_string(),
                },
            ];
            println!("{}", serde_json::to_string_pretty(&tools)?);
        }
        _ => {
            println!("Built-in Tools:");
            println!("  read");
            println!("  edit");
            println!("  write");
            println!("  bash");
            println!("  delete");
            println!("\nMCP Server tools will appear here when a chat session is running");
            println!("Start a chat session first to discover tools: cru chat");
        }
    }
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
