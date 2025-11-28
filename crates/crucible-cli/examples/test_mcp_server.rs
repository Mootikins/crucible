#!/usr/bin/env cargo
//! Quick CLI test for Crucible MCP server
//!
//! Usage: cargo run --example test_mcp_server

use anyhow::Result;
use rmcp::{
    service::ServiceExt,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<()> {
    eprintln!("🚀 Testing Crucible MCP Server\n");

    // Spawn the MCP server and connect
    eprintln!("📝 Spawning server and connecting...");

    // Use the cru binary from PATH or current target
    let cru_path =
        std::env::var("CRUCIBLE_BIN").unwrap_or_else(|_| "./target/release/cru".to_string());

    eprintln!("    Using binary: {}", cru_path);

    let service = match ()
        .serve(TokioChildProcess::new(Command::new(&cru_path).configure(
            |cmd| {
                cmd.arg("mcp");
                // Server will use default config at ~/.config/crucible/config.toml
                // Note: stderr is piped by TokioChildProcess, don't override
            },
        ))?)
        .await
    {
        Ok(svc) => {
            eprintln!("    Service connected successfully");
            svc
        }
        Err(e) => {
            eprintln!("    ❌ Failed to connect: {}", e);
            return Err(e.into());
        }
    };

    // Get server info
    let server_info = service.peer_info();
    if let Some(info) = server_info {
        eprintln!("✅ Connected to server: {}\n", info.server_info.name);
    } else {
        eprintln!("✅ Connected to server\n");
    }

    // List tools
    eprintln!("📋 Listing tools...");
    let tools_response = service.list_tools(Default::default()).await?;

    eprintln!("✅ Found {} tools:\n", tools_response.tools.len());

    for tool in &tools_response.tools {
        eprintln!("  • {}", tool.name);
        if let Some(desc) = &tool.description {
            eprintln!("    {}", desc);
        }
    }

    // Verify count
    if tools_response.tools.len() == 12 {
        eprintln!("\n✅ SUCCESS: All 12 tools discovered!");
        std::process::exit(0);
    } else {
        eprintln!(
            "\n❌ FAILURE: Expected 12 tools, found {}",
            tools_response.tools.len()
        );
        std::process::exit(1);
    }
}
