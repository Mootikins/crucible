//! Session Orchestrator - Interactive Chat Loop
//!
//! Orchestrates the interactive chat session, handling user input, command execution,
//! message processing, and agent communication. Extracted from commands/chat.rs for
//! reusability and testability.
//!
//! NOTE: Interactive REPL is currently stubbed - reedline/ratatui TUI code removed
//! during event architecture cleanup. Use --query for one-shot mode.

use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

use crate::acp::ContextEnricher;
use crate::chat::handlers;
use crate::chat::slash_registry::{SlashCommandRegistry, SlashCommandRegistryBuilder};
use crate::chat::{AgentHandle, ChatMode};
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
                anyhow::bail!(
                    "context_size must be <= {} (got {})",
                    MAX_CONTEXT_SIZE,
                    size
                );
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
            .command_with_hint(
                "commit",
                Arc::new(handlers::CommitHandler),
                "Smart git commit workflow (smart/quick/review/wip)",
                Some("mode [message]".to_string()),
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
    ///
    /// NOTE: Interactive REPL is currently stubbed pending event architecture integration.
    /// Use `cru chat --query "..."` for one-shot queries.
    pub async fn run<A: AgentHandle>(&mut self, _agent: &mut A) -> Result<()> {
        eprintln!(
            "{}",
            "Interactive chat not yet implemented. Use --query for one-shot mode.".yellow()
        );
        eprintln!(
            "{}",
            "Example: cru chat --query \"What files mention authentication?\"".dimmed()
        );
        Ok(())
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    // TDD Test 1: Exit handler should signal exit via shared flag when executed through trait
    #[tokio::test]
    async fn test_exit_handler_via_trait() {
        use async_trait::async_trait;
        use crucible_core::traits::chat::{
            ChatContext, ChatMode as CoreMode, ChatResult, CommandHandler, SearchResult,
        };

        // Simple mock context that doesn't need an agent
        struct SimpleMockContext;

        #[async_trait]
        impl ChatContext for SimpleMockContext {
            fn get_mode(&self) -> CoreMode {
                CoreMode::Plan
            }

            fn request_exit(&mut self) {}
            fn exit_requested(&self) -> bool {
                false
            }

            async fn set_mode(&mut self, _mode: CoreMode) -> ChatResult<()> {
                Ok(())
            }

            async fn semantic_search(
                &self,
                _query: &str,
                _limit: usize,
            ) -> ChatResult<Vec<SearchResult>> {
                Ok(vec![])
            }

            async fn send_command_to_agent(&mut self, _name: &str, _args: &str) -> ChatResult<()> {
                Ok(())
            }

            fn display_search_results(&self, _query: &str, _results: &[SearchResult]) {}
            fn display_help(&self) {}
            fn display_error(&self, _message: &str) {}
            fn display_info(&self, _message: &str) {}
        }

        // Setup: Create exit flag and handler
        let exit_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handler = handlers::ExitHandler::new(exit_flag.clone());
        let mut ctx = SimpleMockContext;

        // Execute handler through the CommandHandler trait
        let result = handler.execute("", &mut ctx).await;

        // Assert: Should succeed and set the exit flag
        assert!(result.is_ok(), "Handler should execute successfully");
        assert!(
            exit_flag.load(std::sync::atomic::Ordering::SeqCst),
            "Exit flag should be set"
        );
    }

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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be greater than 0"));
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
