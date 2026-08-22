use anyhow::Result;
use serde::Serialize;

use crucible_daemon::tools::surface::BuiltinTool;

use crate::cli::ToolsCommands;
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
        } => list(permissions, OutputFormat::for_stdout(format)),
    }
}

fn list(permissions: bool, format: OutputFormat) -> Result<()> {
    if permissions {
        list_permissions()
    } else {
        list_normal(format)
    }
}

/// The tools the daemon always has, independent of MCP servers and plugins.
///
/// Read from the daemon's own closed set, so a new `BuiltinTool` variant
/// appears here without a second list. The CLI used to keep five stale names.
fn builtin_tool_names() -> Vec<&'static str> {
    BuiltinTool::ALL
        .into_iter()
        .map(BuiltinTool::name)
        .collect()
}

fn list_normal(format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => {
            let tools: Vec<ToolOutput> = builtin_tool_names()
                .into_iter()
                .map(|name| ToolOutput {
                    name: name.to_string(),
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&tools)?);
        }
        OutputFormat::Table => {
            let rows: Vec<Vec<String>> = builtin_tool_names()
                .into_iter()
                .map(|name| vec![name.to_string()])
                .collect();
            println!("{}", crate::output::records_table(&["Tool"], &rows));
            println!("\nMCP server tools appear once a chat session is running: cru chat");
        }
        OutputFormat::Plain => {
            println!("Built-in Tools:");
            for name in builtin_tool_names() {
                println!("  {name}");
            }
            println!("\nMCP Server tools will appear here when a chat session is running");
            println!("Start a chat session first to discover tools: cru chat");
        }
    }
    Ok(())
}

fn list_permissions() -> Result<()> {
    println!("# Add these to [permissions].allow in crucible.toml");
    println!();

    println!("# Built-in Tools");
    for name in builtin_tool_names() {
        println!("{name}:*");
    }
    println!();
    println!("# MCP Server tools will appear here when a chat session is running");
    println!("# Start a chat session first to discover tools: cru chat");

    Ok(())
}
