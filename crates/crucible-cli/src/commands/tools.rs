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
        } => list(permissions, format).await,
    }
}

async fn list(permissions: bool, format: OutputFormat) -> Result<()> {
    let client = daemon_client().await?;

    if permissions {
        list_permissions(&client).await
    } else {
        list_normal(&client, format).await
    }
}

/// The tools the daemon always has, independent of MCP servers and plugins.
///
/// One list, three renderings. It used to be written out separately in the JSON
/// arm and the text arm, so adding a tool meant remembering both.
const BUILTIN_TOOLS: [&str; 5] = ["read", "edit", "write", "bash", "delete"];

async fn list_normal(_client: &DaemonClient, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => {
            let tools: Vec<ToolOutput> = BUILTIN_TOOLS
                .iter()
                .map(|name| ToolOutput {
                    name: (*name).to_string(),
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&tools)?);
        }
        OutputFormat::Table => {
            let rows: Vec<Vec<String>> = BUILTIN_TOOLS
                .iter()
                .map(|name| vec![(*name).to_string()])
                .collect();
            println!("{}", crate::output::records_table(&["Tool"], &rows));
            println!("\nMCP server tools appear once a chat session is running: cru chat");
        }
        OutputFormat::Plain => {
            println!("Built-in Tools:");
            for name in BUILTIN_TOOLS {
                println!("  {name}");
            }
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
