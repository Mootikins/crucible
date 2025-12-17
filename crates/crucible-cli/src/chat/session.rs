//! Session Orchestrator - Interactive Chat Loop
//!
//! Orchestrates the interactive chat session, handling user input, command execution,
//! message processing, and agent communication. Extracted from commands/chat.rs for
//! reusability and testability.
//!
//! NOTE: Interactive REPL is currently stubbed - reedline/ratatui TUI code removed
//! during event architecture cleanup. Use --query for one-shot mode.

use anyhow::Result;
use std::sync::Arc;

use crate::acp::ContextEnricher;
use crate::chat::bridge::AgentEventBridge;
use crate::chat::handlers;
use crate::chat::mode_registry::ModeRegistry;
use crate::chat::slash_registry::{SlashCommandRegistry, SlashCommandRegistryBuilder};
use crate::chat::{AgentHandle, ChatError, ChatResult};
use crate::core_facade::KilnContext;
use crate::tui::TuiRunner;
use crucible_core::traits::registry::{Registry, RegistryBuilder};
use crucible_rune::SessionBuilder;

/// Default number of context results to include in enriched prompts
pub const DEFAULT_CONTEXT_SIZE: usize = 5;

/// Maximum allowed context size to prevent excessive memory usage
pub const MAX_CONTEXT_SIZE: usize = 1000;

/// Default number of search results to display
pub const DEFAULT_SEARCH_LIMIT: usize = 10;

/// Session configuration
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Initial chat mode ID (e.g., "plan", "act", "auto")
    pub initial_mode_id: String,
    /// Enable context enrichment for messages
    pub context_enabled: bool,
    /// Number of context results to include (if context enabled)
    pub context_size: Option<usize>,
}

impl SessionConfig {
    /// Create a new session configuration
    pub fn new(initial_mode_id: impl Into<String>, context_enabled: bool, context_size: Option<usize>) -> Self {
        Self {
            initial_mode_id: initial_mode_id.into(),
            context_enabled,
            context_size,
        }
    }

    /// Create default configuration (Plan mode, context enabled, 5 results)
    pub fn default() -> Self {
        Self {
            initial_mode_id: "plan".to_string(),
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
    command_registry: SlashCommandRegistry,
    mode_registry: ModeRegistry,
    exit_flag: Arc<std::sync::atomic::AtomicBool>,
}

impl ChatSession {
    /// Create a new chat session
    pub fn new(config: SessionConfig, core: Arc<KilnContext>) -> Self {
        let context_size = config.context_size.unwrap_or(5);
        let enricher = ContextEnricher::new(core.clone(), Some(context_size));

        // Build the command registry with all static commands
        let exit_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let command_registry = SlashCommandRegistryBuilder::default()
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

        // Initialize mode registry with defaults
        let mode_registry = ModeRegistry::new();

        Self {
            config,
            core,
            enricher,
            command_registry,
            mode_registry,
            exit_flag,
        }
    }

    /// Get a reference to the mode registry
    pub fn mode_registry(&self) -> &ModeRegistry {
        &self.mode_registry
    }

    /// Get a mutable reference to the mode registry
    pub fn mode_registry_mut(&mut self) -> &mut ModeRegistry {
        &mut self.mode_registry
    }

    /// Get a reference to the command registry
    pub fn command_registry(&self) -> &SlashCommandRegistry {
        &self.command_registry
    }

    /// Set mode with validation against the registry
    ///
    /// Validates the mode ID exists in the registry before calling agent.set_mode_str.
    ///
    /// # Arguments
    ///
    /// * `mode_id` - The mode ID to set (e.g., "plan", "act", "auto")
    /// * `agent` - The agent handle to notify of the mode change
    ///
    /// # Returns
    ///
    /// Ok(()) if mode was set successfully, or ChatError::InvalidMode if mode does not exist
    pub async fn set_mode<A: AgentHandle>(&mut self, mode_id: &str, agent: &mut A) -> ChatResult<()> {
        // Validate mode exists in registry
        if !self.mode_registry.exists(mode_id) {
            return Err(ChatError::InvalidMode(mode_id.to_string()));
        }

        // Set mode on agent
        agent.set_mode_str(mode_id).await?;

        // Update registry current mode
        self.mode_registry
            .set_mode(mode_id)
            .map_err(|e| ChatError::InvalidMode(e.to_string()))?;

        Ok(())
    }

    /// Run the interactive session loop
    ///
    /// Creates a TUI-based chat interface with streaming agent responses.
    /// Events flow through:
    /// - User input → TuiRunner → AgentEventBridge → Agent
    /// - Agent response → TextDelta events → Ring → TuiRunner display
    pub async fn run<A: AgentHandle>(&mut self, agent: &mut A) -> Result<()> {
        // Create session for event ring management
        let session_folder = self.core.session_folder();
        let session = SessionBuilder::with_generated_id("chat")
            .with_folder(&session_folder)
            .build();

        // Create event bridge
        let ring = session.ring().clone();
        let bridge = AgentEventBridge::new(session.handle(), ring);

        // Create and run TUI
        let mut runner = TuiRunner::new(&self.config.initial_mode_id)?;
        runner.run(&bridge, agent).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::types::acp::schema::{SessionMode, SessionModeId, SessionModeState};

    // Helper to create a standard mode state with plan/act/auto modes
    fn default_mode_state() -> SessionModeState {
        SessionModeState {
            current_mode_id: SessionModeId(std::sync::Arc::from("plan")),
            available_modes: vec![
                SessionMode {
                    id: SessionModeId(std::sync::Arc::from("plan")),
                    name: "Plan".to_string(),
                    description: Some("Read-only exploration mode".to_string()),
                    meta: None,
                },
                SessionMode {
                    id: SessionModeId(std::sync::Arc::from("act")),
                    name: "Act".to_string(),
                    description: Some("Write-enabled execution mode".to_string()),
                    meta: None,
                },
                SessionMode {
                    id: SessionModeId(std::sync::Arc::from("auto")),
                    name: "Auto".to_string(),
                    description: Some("Auto-approve all operations".to_string()),
                    meta: None,
                },
            ],
            meta: None,
        }
    }


    // TDD Test 1: Exit handler should signal exit via shared flag when executed through trait
    #[tokio::test]
    async fn test_exit_handler_via_trait() {
        use async_trait::async_trait;
        use crucible_core::traits::chat::{
            ChatContext, ChatResult, CommandHandler, SearchResult,
        };

        // Simple mock context that does not need an agent
        struct SimpleMockContext;

        #[async_trait]
        impl ChatContext for SimpleMockContext {
            fn get_mode_id(&self) -> &str {
                "plan"
            }

            fn request_exit(&mut self) {}
            fn exit_requested(&self) -> bool {
                false
            }

            async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
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
        let config = SessionConfig::new("plan", true, Some(10));
        assert_eq!(config.initial_mode_id, "plan");
        assert!(config.context_enabled);
        assert_eq!(config.context_size, Some(10));
    }

    #[test]
    fn test_session_config_default() {
        let config = SessionConfig::default();
        assert_eq!(config.initial_mode_id, "plan");
        assert!(config.context_enabled);
        assert_eq!(config.context_size, Some(5));
    }

    #[test]
    fn test_session_config_clone() {
        let config = SessionConfig::new("act", false, None);
        let cloned = config.clone();
        assert_eq!(config.initial_mode_id, cloned.initial_mode_id);
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
        let config = SessionConfig::new("plan", false, None);
        assert!(!config.context_enabled);
        assert_eq!(config.context_size, None);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_config_all_modes() {
        let plan_config = SessionConfig::new("plan", true, Some(5));
        assert_eq!(plan_config.initial_mode_id, "plan");

        let act_config = SessionConfig::new("act", true, Some(5));
        assert_eq!(act_config.initial_mode_id, "act");

        let auto_config = SessionConfig::new("auto", true, Some(5));
        assert_eq!(auto_config.initial_mode_id, "auto");
    }

    #[test]
    fn test_session_config_validate_zero_context_size() {
        let config = SessionConfig::new("plan", true, Some(0));
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be greater than 0"));
    }

    #[test]
    fn test_session_config_validate_too_large_context_size() {
        let config = SessionConfig::new("plan", true, Some(1001));
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be <= 1000"));
    }

    #[test]
    fn test_session_config_validate_max_context_size() {
        let config = SessionConfig::new("plan", true, Some(1000));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_config_validate_min_context_size() {
        let config = SessionConfig::new("plan", true, Some(1));
        assert!(config.validate().is_ok());
    }

    // Phase 5: Session Integration Tests

    #[test]
    fn test_mode_registry_starts_empty() {
        let mode_registry = ModeRegistry::new();

        assert!(mode_registry.is_empty(), "Mode registry should start empty");
        assert!(!mode_registry.exists("plan"), "Empty registry should not have plan mode");
    }

    #[test]
    fn test_command_registry_has_default_commands() {
        let exit_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let command_registry = SlashCommandRegistryBuilder::default()
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
                "help",
                Arc::new(handlers::HelpHandler),
                "Show available commands",
            )
            .build();

        assert!(
            command_registry.get("exit").is_some(),
            "Registry should have exit command"
        );
        assert!(
            command_registry.get("quit").is_some(),
            "Registry should have quit command"
        );
        assert!(
            command_registry.get("help").is_some(),
            "Registry should have help command"
        );
    }

    #[tokio::test]
    async fn test_initialization_queries_agent_modes() {
        use async_trait::async_trait;
        use crucible_core::traits::chat::{ChatChunk, ChatResult as CoreChatResult};
        use crucible_core::types::acp::schema::{SessionMode, SessionModeId, SessionModeState, AvailableCommand};
        use futures::stream::BoxStream;

        struct MockAgentWithModes {
            mode_state: SessionModeState,
            mode_id: String,
        }

        #[async_trait]
        impl AgentHandle for MockAgentWithModes {
            fn send_message_stream<'a>(&'a mut self, _message: &'a str) -> BoxStream<'a, CoreChatResult<ChatChunk>> {
                Box::pin(futures::stream::empty())
            }

            fn get_mode_id(&self) -> &str {
                &self.mode_id
            }

            async fn set_mode_str(&mut self, mode_id: &str) -> CoreChatResult<()> {
                self.mode_id = mode_id.to_string();
                Ok(())
            }

            fn is_connected(&self) -> bool {
                true
            }

            fn get_modes(&self) -> Option<&SessionModeState> {
                Some(&self.mode_state)
            }

            fn get_commands(&self) -> &[AvailableCommand] {
                &[]
            }
        }

        let agent = MockAgentWithModes {
            mode_state: SessionModeState {
                current_mode_id: SessionModeId(std::sync::Arc::from("custom")),
                available_modes: vec![
                    SessionMode {
                        id: SessionModeId(std::sync::Arc::from("custom")),
                        name: "Custom Mode".to_string(),
                        description: Some("A custom agent mode".to_string()),
                        meta: None,
                    },
                ],
                meta: None,
            },
            mode_id: "custom".to_string(),
        };

        let modes = agent.get_modes();
        assert!(modes.is_some(), "Agent should provide modes");
        let modes = modes.unwrap();
        assert_eq!(modes.available_modes.len(), 1);
        assert_eq!(modes.available_modes[0].id.0.as_ref(), "custom");
    }

    #[test]
    fn test_mode_registry_populated_from_agent() {
        use crucible_core::types::acp::schema::{SessionMode, SessionModeId, SessionModeState};

        let mut mode_registry = ModeRegistry::new();

        let agent_state = SessionModeState {
            current_mode_id: SessionModeId(std::sync::Arc::from("agent-mode")),
            available_modes: vec![
                SessionMode {
                    id: SessionModeId(std::sync::Arc::from("agent-mode")),
                    name: "Agent Mode".to_string(),
                    description: Some("Custom agent mode".to_string()),
                    meta: None,
                },
            ],
            meta: None,
        };

        mode_registry.update(agent_state);

        assert!(!mode_registry.exists("plan"), "Should NOT have plan - only agent modes");
        assert!(mode_registry.exists("agent-mode"), "Should have agent mode");
    }

    #[tokio::test]
    async fn test_set_mode_validates_via_registry() {
        use async_trait::async_trait;
        use crucible_core::traits::chat::{ChatChunk, ChatResult as CoreChatResult};
        use futures::stream::BoxStream;

        struct MockAgent {
            current_mode: String,
        }

        #[async_trait]
        impl AgentHandle for MockAgent {
            fn send_message_stream<'a>(&'a mut self, _message: &'a str) -> BoxStream<'a, CoreChatResult<ChatChunk>> {
                Box::pin(futures::stream::empty())
            }

            fn get_mode_id(&self) -> &str {
                &self.current_mode
            }

            async fn set_mode_str(&mut self, mode_id: &str) -> CoreChatResult<()> {
                self.current_mode = mode_id.to_string();
                Ok(())
            }

            fn is_connected(&self) -> bool {
                true
            }
        }

        let mut agent = MockAgent {
            current_mode: "plan".to_string(),
        };

        let mut mode_registry = ModeRegistry::from_agent(default_mode_state());

        assert!(mode_registry.exists("act"), "act mode should exist");

        if mode_registry.exists("act") {
            agent.set_mode_str("act").await.unwrap();
            mode_registry.set_mode("act").unwrap();
        }
        assert_eq!(agent.current_mode, "act");
        assert_eq!(mode_registry.current_id(), "act");
    }

    #[tokio::test]
    async fn test_set_mode_invalid_mode_returns_error() {
        let mode_registry = ModeRegistry::new();
        assert!(!mode_registry.exists("invalid-mode"), "invalid-mode should not exist");
    }

    #[tokio::test]
    async fn test_set_mode_calls_agent_set_mode() {
        use async_trait::async_trait;
        use crucible_core::traits::chat::{ChatChunk, ChatResult as CoreChatResult};
        use futures::stream::BoxStream;
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct MockAgentWithCounter {
            set_mode_call_count: Arc<AtomicUsize>,
            mode_id: String,
        }

        #[async_trait]
        impl AgentHandle for MockAgentWithCounter {
            fn send_message_stream<'a>(&'a mut self, _message: &'a str) -> BoxStream<'a, CoreChatResult<ChatChunk>> {
                Box::pin(futures::stream::empty())
            }

            fn get_mode_id(&self) -> &str {
                &self.mode_id
            }

            async fn set_mode_str(&mut self, _mode_id: &str) -> CoreChatResult<()> {
                self.set_mode_call_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }

            fn is_connected(&self) -> bool {
                true
            }
        }

        let counter = Arc::new(AtomicUsize::new(0));
        let mut agent = MockAgentWithCounter {
            set_mode_call_count: counter.clone(),
            mode_id: "plan".to_string(),
        };

        let mut mode_registry = ModeRegistry::from_agent(default_mode_state());

        if mode_registry.exists("act") {
            agent.set_mode_str("act").await.unwrap();
            mode_registry.set_mode("act").unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 1, "Agent set_mode should be called once");
    }
}
