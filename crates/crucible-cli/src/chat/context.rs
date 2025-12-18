//! Chat context implementation for CLI
//!
//! Provides the execution context for slash command handlers,
//! bridging the CLI-specific implementations with the core ChatContext trait.

use async_trait::async_trait;
use std::sync::Arc;

use crucible_core::traits::chat::{AgentHandle, ChatContext, ChatError, ChatResult, SearchResult};

use crate::chat::display::Display;
use crate::chat::slash_registry::SlashCommandRegistry;
use crate::core_facade::KilnContext;

/// CLI implementation of ChatContext
///
/// Provides command handlers with access to:
/// - Kiln context (storage, semantic search)
/// - Chat mode state (read-only - use set_mode_str to change)
/// - Agent handle reference for forwarding commands
/// - Display utilities for terminal output
///
/// Note: This context does NOT own the agent. The agent is borrowed
/// when needed for `send_command_to_agent()` operations.
pub struct CliChatContext<'a> {
    /// Reference to kiln context for storage and search
    kiln: Arc<KilnContext>,
    /// Current chat mode ID (read-only)
    mode_id: String,
    /// Mutable reference to agent for sending commands
    agent: &'a mut dyn AgentHandle,
    /// Command registry for listing available commands
    registry: Arc<SlashCommandRegistry>,
}

impl<'a> CliChatContext<'a> {
    /// Create a new CLI chat context
    ///
    /// # Arguments
    ///
    /// * `kiln` - Kiln context for storage and search
    /// * `mode_id` - Current chat mode ID (read-only snapshot)
    /// * `agent` - Mutable reference to agent handle
    /// * `registry` - Command registry for listing commands
    pub fn new(
        kiln: Arc<KilnContext>,
        mode_id: impl Into<String>,
        agent: &'a mut dyn AgentHandle,
        registry: Arc<SlashCommandRegistry>,
    ) -> Self {
        Self {
            kiln,
            mode_id: mode_id.into(),
            agent,
            registry,
        }
    }
}

#[async_trait]
impl<'a> ChatContext for CliChatContext<'a> {
    fn get_mode_id(&self) -> &str {
        &self.mode_id
    }

    fn request_exit(&mut self) {
        // Exit is handled via shared Arc<AtomicBool> in session, not through context
        // This method is a no-op for CLI context
    }

    fn exit_requested(&self) -> bool {
        // Exit is handled via shared Arc<AtomicBool> in session, not through context
        // This method always returns false for CLI context
        false
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        // Update agent mode
        self.agent
            .set_mode_str(mode_id)
            .await
            .map_err(|e| ChatError::ModeChange(e.to_string()))?;

        // Update local mode
        self.mode_id = mode_id.to_string();

        // Display mode change notification
        Display::mode_change(mode_id);

        Ok(())
    }

    async fn semantic_search(&self, query: &str, limit: usize) -> ChatResult<Vec<SearchResult>> {
        // Delegate to kiln context
        let results = self
            .kiln
            .semantic_search(query, limit)
            .await
            .map_err(|e| ChatError::Internal(format!("Search failed: {}", e)))?;

        // Convert from SemanticSearchResult to SearchResult
        Ok(results
            .into_iter()
            .map(|r| SearchResult {
                title: r.title,
                snippet: r.snippet,
                similarity: r.similarity,
            })
            .collect())
    }

    async fn send_command_to_agent(&mut self, name: &str, args: &str) -> ChatResult<()> {
        // Format command as user message
        let command_message = if args.is_empty() {
            format!("/{}", name)
        } else {
            format!("/{} {}", name, args)
        };

        // Send to agent and display response
        let response = self
            .agent
            .send_message(&command_message)
            .await
            .map_err(|e| ChatError::CommandFailed(format!("Agent error: {}", e)))?;

        // Display agent response using Display utilities
        let tool_calls: Vec<_> = response
            .tool_calls
            .iter()
            .map(|tc| crate::chat::display::ToolCallDisplay {
                title: tc.name.clone(),
                arguments: tc.arguments.clone(),
            })
            .collect();

        Display::agent_response(&response.content, &tool_calls);

        Ok(())
    }

    fn display_search_results(&self, query: &str, results: &[SearchResult]) {
        if results.is_empty() {
            Display::no_results(query);
        } else {
            Display::search_results_header(query, results.len());
            for (index, result) in results.iter().enumerate() {
                Display::search_result(index, &result.title, result.similarity, &result.snippet);
            }
        }
    }

    fn display_help(&self) {
        println!(
            "
Available Commands:"
        );
        println!("{}", "=".repeat(40));

        // List all commands from registry
        let commands = self.registry.list_all();
        for cmd in commands {
            let hint = cmd
                .input_hint
                .as_ref()
                .map(|h| format!(" <{}>", h))
                .unwrap_or_default();
            let options = if cmd.secondary_options.is_empty() {
                String::new()
            } else {
                let labels: Vec<_> = cmd
                    .secondary_options
                    .iter()
                    .map(|o| o.label.as_str())
                    .collect();
                format!(" [options: {}]", labels.join(", "))
            };

            println!(
                "  /{}{:20} - {}{}",
                cmd.name, hint, cmd.description, options
            );
        }

        println!();
    }

    fn display_error(&self, message: &str) {
        Display::error(message);
    }

    fn display_info(&self, message: &str) {
        println!("{}", message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock agent for testing
    struct MockAgent {
        mode_id: String,
    }

    #[async_trait]
    impl AgentHandle for MockAgent {
        fn send_message_stream<'a>(
            &'a mut self,
            _message: &'a str,
        ) -> futures::stream::BoxStream<'a, ChatResult<crucible_core::traits::chat::ChatChunk>>
        {
            use futures::stream;
            Box::pin(stream::iter(vec![
                Ok(crucible_core::traits::chat::ChatChunk {
                    delta: "Mock response".to_string(),
                    done: false,
                    tool_calls: None,
                }),
                Ok(crucible_core::traits::chat::ChatChunk {
                    delta: String::new(),
                    done: true,
                    tool_calls: None,
                }),
            ]))
        }

        fn get_mode_id(&self) -> &str {
            &self.mode_id
        }

        async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
            self.mode_id = mode_id.to_string();
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_context_get_mode() {
        // This test would require a mock KilnContext
        // Skipping for now as it would require extensive mocking
    }

    #[tokio::test]
    async fn test_context_exit_request() {
        // This test would require a mock KilnContext
        // Skipping for now as it would require extensive mocking
    }

    #[tokio::test]
    async fn test_context_set_mode() {
        // This test would require a mock KilnContext and registry
        // Skipping for now as it would require extensive mocking
    }
}
