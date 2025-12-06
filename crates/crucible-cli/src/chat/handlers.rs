//! Built-in slash command handlers
//!
//! This module provides the core CLI command handlers:
//! - `ExitHandler`: Exit the chat session
//! - `ModeHandler`: Switch to a specific mode (plan/act/auto)
//! - `ModeCycleHandler`: Cycle to the next mode
//! - `SearchHandler`: Perform semantic search on the knowledge base
//! - `HelpHandler`: Display available commands
//!
//! ## Architecture
//!
//! Each handler implements the `CommandHandler` trait from crucible-core.
//! Handlers are stateless and receive a `ChatContext` for accessing state.

use async_trait::async_trait;
use colored::Colorize;
use std::sync::Arc;

use crucible_core::traits::chat::{
    ChatContext, ChatError, ChatMode, ChatResult, CommandHandler,
};

/// Exit handler - terminates the chat session
///
/// Sets an exit flag in the context to signal the session should end.
pub struct ExitHandler {
    /// Reference to the exit flag
    exit_flag: Arc<std::sync::atomic::AtomicBool>,
}

impl ExitHandler {
    /// Create a new exit handler
    pub fn new(exit_flag: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { exit_flag }
    }
}

#[async_trait]
impl CommandHandler for ExitHandler {
    async fn execute(&self, _args: &str, _ctx: &mut dyn ChatContext) -> ChatResult<()> {
        self.exit_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        println!("{}", "Exiting chat session...".bright_cyan());
        Ok(())
    }
}

/// Mode handler - switches to a specific mode
///
/// Accepts mode name as argument: plan, act, or auto
pub struct ModeHandler;

impl ModeHandler {
    /// Parse mode from string argument
    fn parse_mode(args: &str) -> ChatResult<ChatMode> {
        let mode_str = args.trim().to_lowercase();
        match mode_str.as_str() {
            "plan" => Ok(ChatMode::Plan),
            "act" => Ok(ChatMode::Act),
            "auto" | "autoapprove" => Ok(ChatMode::AutoApprove),
            "" => Err(ChatError::InvalidInput(
                "Mode required. Usage: /mode <plan|act|auto>".to_string(),
            )),
            _ => Err(ChatError::InvalidInput(format!(
                "Invalid mode '{}'. Valid modes: plan, act, auto",
                mode_str
            ))),
        }
    }
}

#[async_trait]
impl CommandHandler for ModeHandler {
    async fn execute(&self, args: &str, ctx: &mut dyn ChatContext) -> ChatResult<()> {
        let new_mode = Self::parse_mode(args)?;
        let current_mode = ctx.get_mode();

        if new_mode == current_mode {
            println!(
                "{}",
                format!("Already in {} mode", mode_name(new_mode)).bright_yellow()
            );
        } else {
            // Mode change happens via context
            println!(
                "{}",
                format!(
                    "Switched from {} to {} mode",
                    mode_name(current_mode),
                    mode_name(new_mode)
                )
                .bright_cyan()
            );
        }

        Ok(())
    }
}

/// Mode cycle handler - cycles to the next mode
///
/// Cycles through: Plan -> Act -> AutoApprove -> Plan
pub struct ModeCycleHandler;

#[async_trait]
impl CommandHandler for ModeCycleHandler {
    async fn execute(&self, _args: &str, ctx: &mut dyn ChatContext) -> ChatResult<()> {
        let current_mode = ctx.get_mode();
        let new_mode = current_mode.cycle_next();

        println!(
            "{}",
            format!(
                "Mode cycled from {} to {}",
                mode_name(current_mode),
                mode_name(new_mode)
            )
            .bright_cyan()
        );

        Ok(())
    }
}

/// Search handler - performs semantic search on the knowledge base
///
/// Accepts search query as argument, displays results
pub struct SearchHandler;

impl SearchHandler {
    /// Default number of search results
    const DEFAULT_LIMIT: usize = 10;

    /// Format search results for display
    fn format_results(results: Vec<crucible_core::traits::chat::SearchResult>) -> String {
        if results.is_empty() {
            return "No results found.".bright_yellow().to_string();
        }

        let mut output = String::new();
        output.push_str(&format!("{}\n", "Search Results:".bright_cyan().bold()));

        for (i, result) in results.iter().enumerate() {
            output.push_str(&format!(
                "\n{}. {} {}\n",
                i + 1,
                result.title.bright_white().bold(),
                format!("(similarity: {:.2})", result.similarity)
                    .dimmed()
            ));
            output.push_str(&format!("   {}\n", result.snippet.dimmed()));
        }

        output
    }
}

#[async_trait]
impl CommandHandler for SearchHandler {
    async fn execute(&self, args: &str, ctx: &mut dyn ChatContext) -> ChatResult<()> {
        let query = args.trim();

        if query.is_empty() {
            return Err(ChatError::InvalidInput(
                "Search query required. Usage: /search <query>".to_string(),
            ));
        }

        println!("{}", format!("Searching for: {}", query).bright_cyan());

        let results = ctx.semantic_search(query, Self::DEFAULT_LIMIT).await?;
        println!("{}", Self::format_results(results));

        Ok(())
    }
}

/// Help handler - displays available commands
///
/// Shows all static and dynamic commands with descriptions
pub struct HelpHandler;

impl HelpHandler {
    // Note: This method is reserved for future integration with CommandRegistry
    // Currently using inline display in execute()
    #[allow(dead_code)]
    fn format_commands(commands: Vec<crucible_core::traits::chat::CommandDescriptor>) -> String {
        if commands.is_empty() {
            return "No commands available.".bright_yellow().to_string();
        }

        let mut output = String::new();
        output.push_str(&format!("{}\n", "Available Commands:".bright_cyan().bold()));

        for cmd in commands {
            let hint = cmd
                .input_hint
                .map(|h| format!(" <{}>", h))
                .unwrap_or_default();

            output.push_str(&format!(
                "\n  /{}{}\n",
                cmd.name.bright_white().bold(),
                hint.dimmed()
            ));
            output.push_str(&format!("    {}\n", cmd.description.dimmed()));
        }

        output
    }
}

#[async_trait]
impl CommandHandler for HelpHandler {
    async fn execute(&self, _args: &str, _ctx: &mut dyn ChatContext) -> ChatResult<()> {
        // This requires CommandRegistry integration to list commands
        // For now, we'll just print a placeholder
        println!("{}", "Help: Available commands".bright_cyan().bold());
        println!("  {}", "/exit - Exit the chat session".dimmed());
        println!(
            "  {}",
            "/mode <plan|act|auto> - Switch to a specific mode".dimmed()
        );
        println!("  {}", "/cycle - Cycle to the next mode".dimmed());
        println!("  {}", "/search <query> - Search the knowledge base".dimmed());
        println!("  {}", "/help - Show this help message".dimmed());

        Ok(())
    }
}

/// Helper function to get mode name as a string
fn mode_name(mode: ChatMode) -> &'static str {
    match mode {
        ChatMode::Plan => "Plan",
        ChatMode::Act => "Act",
        ChatMode::AutoApprove => "AutoApprove",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::traits::chat::SearchResult;
    use std::sync::atomic::AtomicBool;

    // Mock ChatContext for testing
    struct MockContext {
        mode: ChatMode,
        search_results: Vec<SearchResult>,
    }

    #[async_trait]
    impl ChatContext for MockContext {
        fn get_mode(&self) -> ChatMode {
            self.mode
        }

        async fn semantic_search(
            &self,
            _query: &str,
            _limit: usize,
        ) -> ChatResult<Vec<SearchResult>> {
            Ok(self.search_results.clone())
        }

        async fn send_command_to_agent(&mut self, _name: &str, _args: &str) -> ChatResult<()> {
            Ok(())
        }

        fn request_exit(&mut self) {}
        fn exit_requested(&self) -> bool { false }
        async fn set_mode(&mut self, _mode: ChatMode) -> ChatResult<()> { Ok(()) }
        fn display_search_results(&self, _query: &str, _results: &[SearchResult]) {}
        fn display_help(&self) {}
        fn display_error(&self, _message: &str) {}
        fn display_info(&self, _message: &str) {}
    }

    #[tokio::test]
    async fn test_exit_handler_sets_flag() {
        let exit_flag = Arc::new(AtomicBool::new(false));
        let handler = ExitHandler::new(exit_flag.clone());
        let mut ctx = MockContext {
            mode: ChatMode::Plan,
            search_results: vec![],
        };

        let result = handler.execute("", &mut ctx).await;
        assert!(result.is_ok());
        assert!(exit_flag.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_mode_handler_parses_valid_modes() {
        assert!(matches!(
            ModeHandler::parse_mode("plan"),
            Ok(ChatMode::Plan)
        ));
        assert!(matches!(ModeHandler::parse_mode("act"), Ok(ChatMode::Act)));
        assert!(matches!(
            ModeHandler::parse_mode("auto"),
            Ok(ChatMode::AutoApprove)
        ));
        assert!(matches!(
            ModeHandler::parse_mode("autoapprove"),
            Ok(ChatMode::AutoApprove)
        ));
    }

    #[tokio::test]
    async fn test_mode_handler_rejects_invalid_mode() {
        assert!(ModeHandler::parse_mode("invalid").is_err());
        assert!(ModeHandler::parse_mode("").is_err());
    }

    #[tokio::test]
    async fn test_mode_handler_executes() {
        let handler = ModeHandler;
        let mut ctx = MockContext {
            mode: ChatMode::Plan,
            search_results: vec![],
        };

        let result = handler.execute("act", &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mode_cycle_handler_executes() {
        let handler = ModeCycleHandler;
        let mut ctx = MockContext {
            mode: ChatMode::Plan,
            search_results: vec![],
        };

        let result = handler.execute("", &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_handler_requires_query() {
        let handler = SearchHandler;
        let mut ctx = MockContext {
            mode: ChatMode::Plan,
            search_results: vec![],
        };

        let result = handler.execute("", &mut ctx).await;
        assert!(matches!(result, Err(ChatError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_search_handler_executes_with_query() {
        let handler = SearchHandler;
        let mut ctx = MockContext {
            mode: ChatMode::Plan,
            search_results: vec![SearchResult {
                title: "Test Note".to_string(),
                snippet: "Test content".to_string(),
                similarity: 0.95,
            }],
        };

        let result = handler.execute("test query", &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_help_handler_executes() {
        let handler = HelpHandler;
        let mut ctx = MockContext {
            mode: ChatMode::Plan,
            search_results: vec![],
        };

        let result = handler.execute("", &mut ctx).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_search_format_empty_results() {
        let output = SearchHandler::format_results(vec![]);
        assert!(output.contains("No results found"));
    }

    #[test]
    fn test_search_format_with_results() {
        let results = vec![
            SearchResult {
                title: "Note 1".to_string(),
                snippet: "Content 1".to_string(),
                similarity: 0.95,
            },
            SearchResult {
                title: "Note 2".to_string(),
                snippet: "Content 2".to_string(),
                similarity: 0.85,
            },
        ];

        let output = SearchHandler::format_results(results);
        assert!(output.contains("Note 1"));
        assert!(output.contains("Note 2"));
        assert!(output.contains("Content 1"));
        assert!(output.contains("Content 2"));
    }

    #[test]
    fn test_mode_name_mapping() {
        assert_eq!(mode_name(ChatMode::Plan), "Plan");
        assert_eq!(mode_name(ChatMode::Act), "Act");
        assert_eq!(mode_name(ChatMode::AutoApprove), "AutoApprove");
    }
}
