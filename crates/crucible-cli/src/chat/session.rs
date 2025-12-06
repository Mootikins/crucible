//! Session Orchestrator - Interactive Chat Loop
//!
//! Orchestrates the interactive chat session, handling user input, command execution,
//! message processing, and agent communication. Extracted from commands/chat.rs for
//! reusability and testability.

use anyhow::Result;
use colored::Colorize;
use reedline::{
    default_emacs_keybindings, DefaultPrompt, EditCommand, Emacs, KeyCode, KeyModifiers,
    Reedline, ReedlineEvent, Signal,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error};

use crate::acp::ContextEnricher;
use crate::chat::{AgentHandle, ChatMode, ChatModeDisplay, Display, ToolCallDisplay};
use crate::chat::handlers;
use crate::chat::slash_registry::{SlashCommandRegistry, SlashCommandRegistryBuilder};
use crate::core_facade::KilnContext;
use crucible_core::traits::registry::RegistryBuilder;

/// Default number of context results to include in enriched prompts
pub const DEFAULT_CONTEXT_SIZE: usize = 5;

/// Maximum allowed context size to prevent excessive memory usage
pub const MAX_CONTEXT_SIZE: usize = 1000;

/// Default number of search results to display
pub const DEFAULT_SEARCH_LIMIT: usize = 10;

/// Session configuration
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Initial chat mode (Plan/Act/AutoApprove)
    pub initial_mode: ChatMode,
    /// Enable context enrichment for messages
    pub context_enabled: bool,
    /// Number of context results to include (if context enabled)
    pub context_size: Option<usize>,
}

impl SessionConfig {
    /// Create a new session configuration
    pub fn new(initial_mode: ChatMode, context_enabled: bool, context_size: Option<usize>) -> Self {
        Self {
            initial_mode,
            context_enabled,
            context_size,
        }
    }

    /// Create default configuration (Plan mode, context enabled, 5 results)
    pub fn default() -> Self {
        Self {
            initial_mode: ChatMode::Plan,
            context_enabled: true,
            context_size: Some(DEFAULT_CONTEXT_SIZE),
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if let Some(size) = self.context_size {
            if size == 0 {
                anyhow::bail!("context_size must be greater than 0");
            }
            if size > MAX_CONTEXT_SIZE {
                anyhow::bail!("context_size must be <= {} (got {})", MAX_CONTEXT_SIZE, size);
            }
        }
        Ok(())
    }
}

/// Interactive chat session orchestrator
pub struct ChatSession {
    config: SessionConfig,
    core: Arc<KilnContext>,
    enricher: ContextEnricher,
    registry: SlashCommandRegistry,
    exit_flag: Arc<std::sync::atomic::AtomicBool>,
}

impl ChatSession {
    /// Create a new chat session
    pub fn new(config: SessionConfig, core: Arc<KilnContext>) -> Self {
        let context_size = config.context_size.unwrap_or(5);
        let enricher = ContextEnricher::new(core.clone(), Some(context_size));

        // Build the command registry with all static commands
        let exit_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let registry = SlashCommandRegistryBuilder::default()
            .command(
                "exit",
                Arc::new(handlers::ExitHandler::new(exit_flag.clone())),
                "Exit the chat session",
            )
            .command(
                "quit",
                Arc::new(handlers::ExitHandler::new(exit_flag.clone())),
                "Exit the chat session",
            )
            .command(
                "plan",
                Arc::new(handlers::ModeHandler),
                "Switch to Plan mode (read-only)",
            )
            .command(
                "act",
                Arc::new(handlers::ModeHandler),
                "Switch to Act mode (write-enabled)",
            )
            .command(
                "auto",
                Arc::new(handlers::ModeHandler),
                "Switch to AutoApprove mode",
            )
            .command(
                "mode",
                Arc::new(handlers::ModeCycleHandler),
                "Cycle to the next mode",
            )
            .command_with_hint(
                "search",
                Arc::new(handlers::SearchHandler),
                "Search the knowledge base",
                Some("query".to_string()),
            )
            .command(
                "help",
                Arc::new(handlers::HelpHandler),
                "Show available commands",
            )
            .build();

        Self {
            config,
            core,
            enricher,
            registry,
            exit_flag,
        }
    }

    /// Run the interactive session loop
    pub async fn run<A: AgentHandle>(&mut self, agent: &mut A) -> Result<()> {
        let mut current_mode = self.config.initial_mode;
        let mut last_ctrl_c: Option<Instant> = None;

        // Configure keybindings:
        // - Shift+Tab: silent mode cycle
        // - Ctrl+J: insert newline (multiline input)
        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding(
            KeyModifiers::SHIFT,
            KeyCode::BackTab,
            ReedlineEvent::ExecuteHostCommand("\x00mode".to_string()),
        );
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('j'),
            ReedlineEvent::Edit(vec![EditCommand::InsertNewline]),
        );
        let edit_mode = Box::new(Emacs::new(keybindings));
        let mut line_editor = Reedline::create().with_edit_mode(edit_mode);

        Display::welcome_banner(current_mode);

        loop {
            // Create simple prompt based on current mode
            let mode_icon = current_mode.icon();
            let prompt_indicator = format!("{} {} ", current_mode.display_name(), mode_icon);
            let prompt = DefaultPrompt::new(
                reedline::DefaultPromptSegment::Basic(prompt_indicator),
                reedline::DefaultPromptSegment::Empty,
            );

            match line_editor.read_line(&prompt) {
                Ok(Signal::Success(input)) => {
                    // Skip empty input
                    if input.trim().is_empty() {
                        continue;
                    }

                    // Check if this is a slash command or silent mode keybinding
                    if input.starts_with('/') || input == "\x00mode" {
                        let (command_name, args) = parse_slash_command(&input);

                        // Try to find handler in registry
                        if let Some(_handler) = self.registry.get_handler(command_name) {
                            // Special handling for plan/act/auto - pass mode name as args to ModeHandler
                            let effective_args = match command_name {
                                "plan" => "plan",
                                "act" => "act",
                                "auto" => "auto",
                                _ => args,
                            };

                            // Execute command inline
                            // Note: We handle commands directly here instead of using CommandHandler trait
                            // due to agent lifetime constraints with CliChatContext.
                            // Full ChatContext integration is deferred to a future phase.
                            let result: Result<()> = match command_name {
                                "exit" | "quit" => {
                                    Display::goodbye();
                                    self.exit_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                                    Ok(())
                                }
                                "plan" | "act" | "auto" => {
                                    // Handle mode switch
                                    let new_mode = match command_name {
                                        "plan" => ChatMode::Plan,
                                        "act" => ChatMode::Act,
                                        "auto" => ChatMode::AutoApprove,
                                        _ => unreachable!(),
                                    };
                                    agent.set_mode(new_mode).await?;
                                    current_mode = new_mode;
                                    Display::mode_change(new_mode);
                                    Ok(())
                                }
                                "mode" => {
                                    // Cycle mode
                                    current_mode = current_mode.cycle_next();
                                    agent.set_mode(current_mode).await?;
                                    // Silent for keybinding, display for command
                                    if input != "\x00mode" {
                                        Display::mode_change(current_mode);
                                    }
                                    Ok(())
                                }
                                "search" => {
                                    // Handle search
                                    if effective_args.is_empty() {
                                        Display::error("Search query required. Usage: /search <query>");
                                        Ok(())
                                    } else {
                                        print!("{} ", "🔍 Searching...".bright_cyan().dimmed());
                                        flush_stdout();

                                        match self.core.semantic_search(effective_args, DEFAULT_SEARCH_LIMIT).await {
                                            Ok(results) => {
                                                print!("\r{}\r", " ".repeat(20));
                                                flush_stdout();

                                                if results.is_empty() {
                                                    Display::no_results(effective_args);
                                                } else {
                                                    Display::search_results_header(effective_args, results.len());
                                                    for (i, result) in results.iter().enumerate() {
                                                        Display::search_result(
                                                            i,
                                                            &result.title,
                                                            result.similarity,
                                                            &result.snippet,
                                                        );
                                                    }
                                                }
                                                println!();
                                                Ok(())
                                            }
                                            Err(e) => {
                                                print!("\r{}\r", " ".repeat(20));
                                                flush_stdout();
                                                Display::search_error(&e.to_string());
                                                Ok(())
                                            }
                                        }
                                    }
                                }
                                "help" => {
                                    // Display help
                                    println!("\nAvailable Commands:");
                                    println!("{}", "=".repeat(40));
                                    for cmd in self.registry.list_all() {
                                        let hint = cmd
                                            .input_hint
                                            .as_ref()
                                            .map(|h| format!(" <{}>", h))
                                            .unwrap_or_default();
                                        println!("  /{}{:20} - {}", cmd.name, hint, cmd.description);
                                    }
                                    println!();
                                    Ok(())
                                }
                                _ => {
                                    Display::error(&format!("Command '{}' registered but not implemented", command_name));
                                    Ok(())
                                }
                            };

                            if let Err(e) = result {
                                Display::error(&e.to_string());
                            }

                            // Check if exit was requested
                            if self.exit_flag.load(std::sync::atomic::Ordering::SeqCst) {
                                break;
                            }

                            continue;
                        } else {
                            // Unknown command
                            Display::error(&format!("Unknown command: /{}", command_name));
                            continue;
                        }
                    }

                    // Handle regular message
                    self.handle_message(&input, agent).await?;
                }
                Ok(Signal::CtrlC) => {
                    use std::time::Duration;
                    if let Some(last) = last_ctrl_c {
                        if last.elapsed() < Duration::from_secs(2) {
                            println!();
                            Display::goodbye();
                            break;
                        }
                    }
                    last_ctrl_c = Some(Instant::now());
                    println!("\n{}", "Press Ctrl+C again to exit".yellow());
                    continue;
                }
                Ok(Signal::CtrlD) => {
                    println!();
                    Display::goodbye();
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


    /// Handle a regular message (not a command)
    async fn handle_message<A: AgentHandle>(&self, input: &str, agent: &mut A) -> Result<()> {

        // Prepare the message (with or without context enrichment)
        let message = if !self.config.context_enabled {
            input.to_string()
        } else {
            // Show context enrichment indicator
            print!(
                "{} ",
                "🔍 Finding relevant context...".bright_cyan().dimmed()
            );
            flush_stdout();

            let enriched_result = self.enricher.enrich_with_results(input).await;

            // Clear the enrichment indicator
            print!("\r{}\r", " ".repeat(35));
            flush_stdout();

            match enriched_result {
                Ok(result) => {
                    // Display the notes found to the user
                    if !result.notes_found.is_empty() {
                        println!(
                            "{} Found {} relevant notes:",
                            "📚".dimmed(),
                            result.notes_found.len()
                        );
                        for note in &result.notes_found {
                            println!("  {} {}", "→".dimmed(), note.title.bright_white());
                        }
                        println!();
                    }

                    result.prompt
                }
                Err(e) => {
                    debug!("Context enrichment failed: {}", e);
                    input.to_string()
                }
            }
        };

        // Show thinking indicator
        print!("{} ", "🤔 Thinking...".bright_blue().dimmed());
        flush_stdout();

        match agent.send_message(&message).await {
            Ok(response) => {
                // Clear the "thinking" indicator
                print!("\r{}\r", " ".repeat(20));
                flush_stdout();

                // Convert generic tool calls to display format
                // Take ownership to avoid clones (response not used after this)
                let display_tools: Vec<ToolCallDisplay> = response
                    .tool_calls
                    .into_iter()
                    .map(|t| ToolCallDisplay {
                        title: t.name,
                        arguments: t.arguments,
                    })
                    .collect();

                Display::agent_response(&response.content, &display_tools);
            }
            Err(e) => {
                // Clear the "thinking" indicator
                print!("\r{}\r", " ".repeat(20));
                flush_stdout();

                error!("Failed to send message: {}", e);
                Display::error(&e.to_string());
            }
        }

        Ok(())
    }
}

/// Parse a slash command into (command_name, args)
///
/// Strips leading `/` and splits on first space.
/// Returns ("mode", "") for the special "\x00mode" keybinding.
fn parse_slash_command(input: &str) -> (&str, &str) {
    // Handle silent mode keybinding
    if input == "\x00mode" {
        return ("mode", "");
    }

    // Strip leading `/`
    let input = input.strip_prefix('/').unwrap_or(input);

    // Split on first space
    if let Some(pos) = input.find(' ') {
        (&input[..pos], input[pos + 1..].trim())
    } else {
        (input, "")
    }
}

/// Helper to flush stdout without panicking
fn flush_stdout() {
    use std::io::Write;
    if let Err(e) = std::io::stdout().flush() {
        tracing::debug!("Failed to flush stdout: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SessionConfig tests
    #[test]
    fn test_session_config_new() {
        let config = SessionConfig::new(ChatMode::Plan, true, Some(10));
        assert_eq!(config.initial_mode, ChatMode::Plan);
        assert!(config.context_enabled);
        assert_eq!(config.context_size, Some(10));
    }

    #[test]
    fn test_session_config_default() {
        let config = SessionConfig::default();
        assert_eq!(config.initial_mode, ChatMode::Plan);
        assert!(config.context_enabled);
        assert_eq!(config.context_size, Some(5));
    }

    #[test]
    fn test_session_config_clone() {
        let config = SessionConfig::new(ChatMode::Act, false, None);
        let cloned = config.clone();
        assert_eq!(config.initial_mode, cloned.initial_mode);
        assert_eq!(config.context_enabled, cloned.context_enabled);
        assert_eq!(config.context_size, cloned.context_size);
    }

    #[test]
    fn test_session_config_validate_success() {
        let config = SessionConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_config_context_disabled_no_size() {
        let config = SessionConfig::new(ChatMode::Plan, false, None);
        assert!(!config.context_enabled);
        assert_eq!(config.context_size, None);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_config_all_modes() {
        let plan_config = SessionConfig::new(ChatMode::Plan, true, Some(5));
        assert_eq!(plan_config.initial_mode, ChatMode::Plan);

        let act_config = SessionConfig::new(ChatMode::Act, true, Some(5));
        assert_eq!(act_config.initial_mode, ChatMode::Act);

        let auto_config = SessionConfig::new(ChatMode::AutoApprove, true, Some(5));
        assert_eq!(auto_config.initial_mode, ChatMode::AutoApprove);
    }

    #[test]
    fn test_session_config_validate_zero_context_size() {
        let config = SessionConfig::new(ChatMode::Plan, true, Some(0));
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be greater than 0"));
    }

    #[test]
    fn test_session_config_validate_too_large_context_size() {
        let config = SessionConfig::new(ChatMode::Plan, true, Some(1001));
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be <= 1000"));
    }

    #[test]
    fn test_session_config_validate_max_context_size() {
        let config = SessionConfig::new(ChatMode::Plan, true, Some(1000));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_config_validate_min_context_size() {
        let config = SessionConfig::new(ChatMode::Plan, true, Some(1));
        assert!(config.validate().is_ok());
    }

    // ChatSession creation tests
    // Note: Full session tests require mock agent and core facade
    // These will be added when we implement the run() method
}
