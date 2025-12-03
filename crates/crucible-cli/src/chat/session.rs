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
use crate::chat::{ChatAgent, ChatMode, ChatModeDisplay, Command, CommandParser, Display, ToolCallDisplay};
use crate::core_facade::CrucibleCoreFacade;

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
            context_size: Some(5),
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if let Some(size) = self.context_size {
            if size == 0 {
                anyhow::bail!("context_size must be greater than 0");
            }
            if size > 1000 {
                anyhow::bail!("context_size must be <= 1000 (got {})", size);
            }
        }
        Ok(())
    }
}

/// Interactive chat session orchestrator
pub struct ChatSession {
    config: SessionConfig,
    core: Arc<CrucibleCoreFacade>,
    enricher: ContextEnricher,
}

impl ChatSession {
    /// Create a new chat session
    pub fn new(config: SessionConfig, core: Arc<CrucibleCoreFacade>) -> Self {
        let context_size = config.context_size.unwrap_or(5);
        let enricher = ContextEnricher::new(core.clone(), Some(context_size));

        Self {
            config,
            core,
            enricher,
        }
    }

    /// Run the interactive session loop
    pub async fn run<A: ChatAgent>(&mut self, agent: &mut A) -> Result<()> {
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

                    // Parse and handle commands
                    if let Some(command) = CommandParser::parse(&input) {
                        if self.handle_command(command, &mut current_mode, agent).await? {
                            break; // Exit command
                        }
                        continue;
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

    /// Handle a parsed command
    ///
    /// Returns true if the session should exit.
    async fn handle_command<A: ChatAgent>(
        &self,
        command: Command,
        current_mode: &mut ChatMode,
        agent: &mut A,
    ) -> Result<bool> {

        match command {
            Command::Exit => {
                Display::goodbye();
                return Ok(true); // Signal to exit
            }
            Command::Plan => {
                *current_mode = ChatMode::Plan;
                agent.set_mode(*current_mode).await?;
                Display::mode_change(*current_mode);
            }
            Command::Act => {
                *current_mode = ChatMode::Act;
                agent.set_mode(*current_mode).await?;
                Display::mode_change(*current_mode);
            }
            Command::Auto => {
                *current_mode = ChatMode::AutoApprove;
                agent.set_mode(*current_mode).await?;
                Display::mode_change(*current_mode);
            }
            Command::Mode => {
                *current_mode = current_mode.cycle_next();
                agent.set_mode(*current_mode).await?;
                Display::mode_change(*current_mode);
            }
            Command::SilentMode => {
                // Cycle mode without visual output - prompt updates on next iteration
                *current_mode = current_mode.cycle_next();
                agent.set_mode(*current_mode).await?;
            }
            Command::Search(query) => {
                // Show searching indicator
                print!("{} ", "🔍 Searching...".bright_cyan().dimmed());
                flush_stdout();

                match self.core.semantic_search(&query, 10).await {
                    Ok(results) => {
                        // Clear searching indicator
                        print!("\r{}\r", " ".repeat(20));
                        flush_stdout();

                        if results.is_empty() {
                            Display::no_results(&query);
                        } else {
                            Display::search_results_header(&query, results.len());
                            for (i, result) in results.iter().enumerate() {
                                Display::search_result(
                                    i,
                                    &result.title,
                                    result.similarity,
                                    &result.snippet,
                                );
                            }
                        }
                    }
                    Err(e) => {
                        // Clear searching indicator
                        print!("\r{}\r", " ".repeat(20));
                        flush_stdout();

                        Display::search_error(&e.to_string());
                    }
                }
                println!();
            }
        }

        Ok(false) // Continue session
    }

    /// Handle a regular message (not a command)
    async fn handle_message<A: ChatAgent>(&self, input: &str, agent: &mut A) -> Result<()> {

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
                let display_tools: Vec<ToolCallDisplay> = response
                    .tool_calls
                    .iter()
                    .map(|t| ToolCallDisplay {
                        title: t.name.clone(),
                        arguments: t.arguments.clone(),
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
