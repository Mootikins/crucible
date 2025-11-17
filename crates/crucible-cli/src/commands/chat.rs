//! Chat Command - ACP-based Natural Language Interface
//!
//! Provides an interactive chat interface using the Agent Client Protocol.

use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info};

use crate::acp::{discover_agent, ContextEnricher, CrucibleAcpClient};
use crate::config::CliConfig;
use crate::core_facade::CrucibleCoreFacade;

/// Execute the chat command
///
/// # Arguments
/// * `config` - CLI configuration
/// * `agent_name` - Optional preferred agent name
/// * `query` - Optional one-shot query (if None, starts interactive mode)
/// * `read_only` - If true, agent cannot write files (chat mode); if false, agent can write (act mode)
/// * `no_context` - If true, skip context enrichment
/// * `context_size` - Number of context results to include
pub async fn execute(
    config: CliConfig,
    agent_name: Option<String>,
    query: Option<String>,
    read_only: bool,
    no_context: bool,
    context_size: Option<usize>,
) -> Result<()> {
    info!("Starting chat command");
    info!(
        "Mode: {}",
        if read_only {
            "Read-only (chat)"
        } else {
            "Write-enabled (act)"
        }
    );

    // Initialize core facade
    info!("Initializing Crucible core...");
    let core = Arc::new(CrucibleCoreFacade::from_config(config).await?);
    info!("Core initialized successfully");

    // Discover agent
    info!("Discovering ACP agent...");
    let agent = discover_agent(agent_name.as_deref()).await?;
    info!("Using agent: {} ({})", agent.name, agent.command);

    // Create ACP client
    let client = CrucibleAcpClient::new(agent, read_only);

    // Spawn agent
    client.spawn().await?;

    // Handle query
    if let Some(query_text) = query {
        // One-shot mode
        info!("One-shot query mode");

        let prompt = if no_context {
            info!("Context enrichment disabled");
            query_text
        } else {
            // Enrich with context
            info!("Enriching query with context...");
            let enricher = ContextEnricher::new(core.clone(), context_size);
            enricher.enrich(&query_text).await?
        };

        // Start chat with enriched prompt
        client.start_chat(&prompt).await?;
    } else {
        // Interactive mode
        info!("Interactive chat mode");
        println!("\n🤖 Crucible Chat");
        println!("================");
        println!("Mode: {}", if read_only { "Read-only (chat)" } else { "Write-enabled (act)" });
        println!("Type your message and press Enter. Type 'exit' to quit.\n");

        // TODO: Implement interactive loop with rustyline/reedline
        // For MVP, just show what would happen
        println!("🚧 Interactive chat loop - MVP placeholder");
        println!("In full implementation, this would:");
        println!("  1. Accept user input with line editing");
        println!("  2. Enrich each query with context");
        println!("  3. Send to agent and stream responses");
        println!("  4. Handle file operations based on read_only mode");
    }

    // Cleanup
    client.shutdown().await?;

    Ok(())
}

/// Helper function to print a prompt with context highlighting
fn print_enriched_prompt(prompt: &str) {
    use colored::Colorize;

    println!("\n{}", "╭─ Enriched Prompt ─────────────────────────╮".blue());
    for line in prompt.lines() {
        if line.starts_with("# ") {
            println!("{}", line.bright_blue().bold());
        } else if line.starts_with("## ") {
            println!("{}", line.cyan().bold());
        } else {
            println!("{}", line);
        }
    }
    println!("{}", "╰────────────────────────────────────────────╯".blue());
}
