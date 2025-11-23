//! Chat Command - ACP-based Natural Language Interface
//!
//! Provides an interactive chat interface using the Agent Client Protocol.
//! Supports toggleable plan (read-only) and act (write-enabled) modes.

use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info};

use crate::acp::{discover_agent, ContextEnricher, CrucibleAcpClient};
use crate::config::CliConfig;
use crate::core_facade::CrucibleCoreFacade;

/// Chat mode - can be toggled during session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMode {
    /// Plan mode - read-only, agent cannot modify files
    Plan,
    /// Act mode - write-enabled, agent can modify files
    Act,
}

impl ChatMode {
    /// Get the display name for this mode
    pub fn display_name(&self) -> &'static str {
        match self {
            ChatMode::Plan => "plan",
            ChatMode::Act => "act",
        }
    }

    /// Toggle to the other mode
    pub fn toggle(&self) -> Self {
        match self {
            ChatMode::Plan => ChatMode::Act,
            ChatMode::Act => ChatMode::Plan,
        }
    }

    /// Check if this mode allows writes
    pub fn is_read_only(&self) -> bool {
        matches!(self, ChatMode::Plan)
    }
}

/// Execute the chat command
///
/// # Arguments
/// * `config` - CLI configuration
/// * `agent_name` - Optional preferred agent name
/// * `query` - Optional one-shot query (if None, starts interactive mode)
/// * `read_only` - Initial mode: if true, starts in plan mode; if false, starts in act mode
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
    // Determine initial mode
    let initial_mode = if read_only {
        ChatMode::Plan
    } else {
        ChatMode::Act
    };

    info!("Starting chat command");
    info!("Initial mode: {}", initial_mode.display_name());

    // Initialize core facade
    info!("Initializing Crucible core...");
    let core = Arc::new(CrucibleCoreFacade::from_config(config).await?);
    info!("Core initialized successfully");

    // Discover agent
    info!("Discovering ACP agent...");
    let agent = discover_agent(agent_name.as_deref()).await?;
    info!("Using agent: {} ({})", agent.name, agent.command);

    // Create ACP client with kiln path for tool initialization
    let kiln_path = core.config().kiln.path.clone();
    let mut client = CrucibleAcpClient::new(agent, initial_mode.is_read_only())
        .with_kiln_path(kiln_path);

    // Spawn agent (tools will be initialized automatically)
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

        // Cleanup
        client.shutdown().await?;
    } else {
        // Interactive mode
        info!("Interactive chat mode");
        run_interactive_session(core, &mut client, initial_mode, no_context, context_size).await?;

        // Cleanup
        client.shutdown().await?;
    }

    Ok(())
}

/// Run an interactive chat session with mode toggling support
async fn run_interactive_session(
    core: Arc<CrucibleCoreFacade>,
    client: &mut CrucibleAcpClient,
    initial_mode: ChatMode,
    no_context: bool,
    context_size: Option<usize>,
) -> Result<()> {
    use colored::Colorize;
    use reedline::{DefaultPrompt, Reedline, Signal};

    let mut current_mode = initial_mode;
    let mut line_editor = Reedline::create();
    let enricher = ContextEnricher::new(core.clone(), context_size);

    println!("\n{}", "🤖 Crucible Chat".bright_blue().bold());
    println!("{}", "=================".bright_blue());
    println!("Mode: {} {}",
        current_mode.display_name().bright_cyan().bold(),
        if current_mode == ChatMode::Plan { "(read-only)" } else { "(write-enabled)" }.dimmed()
    );
    println!();
    println!("{}", "Commands:".bold());
    println!("  {} - Switch to plan mode (read-only)", "/plan".green());
    println!("  {} - Switch to act mode (write-enabled)", "/act".green());
    println!("  {} - Exit chat", "/exit".green());
    println!();

    loop {
        // Create simple prompt based on current mode
        let prompt_indicator = format!(
            "{} {} ",
            current_mode.display_name(),
            if current_mode == ChatMode::Plan { "📖" } else { "✏️" }
        );
        let prompt = DefaultPrompt::new(
            reedline::DefaultPromptSegment::Basic(prompt_indicator),
            reedline::DefaultPromptSegment::Empty,
        );

        // Read user input
        let sig = line_editor.read_line(&prompt);

        match sig {
            Ok(Signal::Success(buffer)) => {
                let input = buffer.trim();

                // Handle empty input
                if input.is_empty() {
                    continue;
                }

                // Handle commands
                if input == "/exit" || input == "/quit" {
                    println!("{}", "👋 Goodbye!".bright_blue());
                    break;
                } else if input == "/plan" {
                    current_mode = ChatMode::Plan;
                    println!("{} Mode switched to: {} (read-only)",
                        "→".bright_cyan(),
                        "plan".bright_cyan().bold()
                    );
                    // Note: In full implementation, would update client permissions here
                    continue;
                } else if input == "/act" {
                    current_mode = ChatMode::Act;
                    println!("{} Mode switched to: {} (write-enabled)",
                        "→".bright_cyan(),
                        "act".bright_cyan().bold()
                    );
                    // Note: In full implementation, would update client permissions here
                    continue;
                }

                // Prepare the message (with or without context enrichment)
                let message = if no_context {
                    input.to_string()
                } else {
                    match enricher.enrich(input).await {
                        Ok(enriched) => enriched,
                        Err(e) => {
                            error!("Context enrichment failed: {}", e);
                            info!("Falling back to original message");
                            input.to_string()
                        }
                    }
                };

                // Send to agent
                println!(); // Blank line before response
                match client.send_message(&message).await {
                    Ok(response) => {
                        // Print agent response
                        println!("{}", "╭─ Agent Response ──────────────────────────╮".bright_blue());
                        for line in response.lines() {
                            println!("│ {}", line);
                        }
                        println!("{}", "╰────────────────────────────────────────────╯".bright_blue());
                        println!(); // Blank line after response
                    }
                    Err(e) => {
                        error!("Failed to send message: {}", e);
                        println!("{} Error: {}", "✗".red(), e);
                    }
                }
            }
            Ok(Signal::CtrlC) => {
                println!("\n{}", "Interrupted. Type /exit to quit.".yellow());
                continue;
            }
            Ok(Signal::CtrlD) => {
                println!("\n{}", "👋 Goodbye!".bright_blue());
                break;
            }
            Err(err) => {
                error!("Error reading input: {}", err);
                break;
            }
        }
    }

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
