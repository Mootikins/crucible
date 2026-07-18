//! Chat framework abstraction traits
//!
//! Following SOLID principles, this module defines backend-agnostic chat abstractions.
//!
//! ## Architecture
//!
//! - **AgentHandle**: Runtime handle to an active agent (ACP, internal, direct LLM)
//! - **CommandHandler**: Trait for implementing slash commands
//! - **ChatContext**: Execution context for command handlers
//!
//! ## Mode Handling
//!
//! Modes are now handled via string IDs (e.g., "plan", "act", "auto") with
//! `SessionModeState` providing the list of available modes from the agent.
//!
//! ## Naming Convention
//!
//! - **AgentCard**: Static definition (prompt + metadata) - see `agent::types`
//! - **AgentHandle**: Runtime handle to active agent - this module
//!
//! ## Design Principles
//!
//! **Dependency Inversion**: Core defines interfaces, implementations live in CLI/agent crates
//! **Interface Segregation**: Separate traits for distinct capabilities
//! **Protocol Independence**: Abstracts over ACP, internal agents, direct LLM APIs

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

use crate::types::acp::schema::SessionModeState;

/// Result type for chat operations
pub type ChatResult<T> = Result<T, ChatError>;

/// Chat operation errors
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum ChatError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Communication error: {0}")]
    Communication(String),

    #[error("Mode change error: {0}")]
    ModeChange(String),

    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Agent not available: {0}")]
    AgentUnavailable(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Invalid mode: {0}")]
    InvalidMode(String),

    #[error("Operation not supported: {0}")]
    NotSupported(String),
}

/// Metadata about a note found during Precognition enrichment.
/// Carried through RPC so TUI/web can display which notes informed the response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrecognitionNoteInfo {
    pub title: String,
    pub kiln_label: Option<String>,
}

/// Result from a completed tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolResult {
    /// Tool name that completed
    pub name: String,
    /// Result content (may be truncated for display)
    pub result: String,
    /// Error message if tool failed
    pub error: Option<String>,
    /// LLM-assigned call ID for matching results to the correct tool call
    #[serde(default)]
    pub call_id: Option<String>,
    /// Tool signaled the agent loop should end after this batch.
    /// The loop only honors termination when *every* result in the batch
    /// sets this — one tool can't unilaterally cut another's work short.
    ///
    /// **Producer scope (v1):** today this is only set by Lua
    /// `pre_tool_call` handlers returning `{ handled = true,
    /// terminate = true }`. The native `ToolExecutor::execute_tool` trait
    /// returns `serde_json::Value` and has no way to signal terminate —
    /// non-Lua tools always send `terminate: false`.
    ///
    /// **Consumer scope (v1):** the conjunctive check fires at
    /// `TurnEvent::ToolBatchEnd`. The genai agent loop emits that event
    /// after every tool batch. The ACP delegation path
    /// (`crucible-daemon/src/acp_handle.rs`) does not yet emit
    /// `ToolBatchEnd`, so this flag has no effect for
    /// `cru chat -a claude / opencode / gemini` sessions. Wire
    /// `ToolBatchEnd` through the ACP adapter when an ACP-side use case
    /// appears.
    #[serde(default)]
    pub terminate: bool,
}

/// Runtime handle to an active agent.
///
/// `AgentHandle` is a supertrait of [`Agent`](crate::turn::Agent): every
/// handle must also expose the lean `Agent` surface (`capabilities`,
/// `turn`, `cancel`, `switch_model`). Setters below are the "wide"
/// surface specific to interactive sessions; over time they migrate
/// to inherent methods on the concrete handle types, leaving this
/// trait as a narrow extension of `Agent`.
#[async_trait]
pub trait AgentHandle: crate::turn::Agent + Send + Sync {
    /// Dispatch a user message to the underlying agent without
    /// consuming its response stream.
    ///
    /// Used by clients that observe the response through a side channel
    /// (e.g. the live TUI, which subscribes to SessionEvents directly).
    /// Concrete impls are responsible for the actual dispatch.
    async fn send_message_fire_and_forget(&mut self, message: String) -> ChatResult<()>;

    fn get_modes(&self) -> Option<&SessionModeState> {
        None
    }

    fn get_mode_id(&self) -> &str {
        "plan"
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()>;

    /// Clear conversation history
    ///
    /// Resets the agent's conversation context, removing all previous messages.
    /// UI state should be cleared separately.
    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }

    /// Switch to a different model
    ///
    /// This may require recreating the underlying agent/connection.
    /// The implementation should preserve conversation history if possible.
    ///
    /// # Arguments
    /// * `model_id` - The model identifier (e.g., "llama3.2", "gpt-4", "claude-3-opus")
    ///
    /// # Returns
    /// * `Ok(())` if the switch was successful
    /// * `Err(ChatError::NotSupported)` if the agent doesn't support model switching
    async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
        Err(ChatError::NotSupported("switch_model".into()))
    }

    /// Get the current model identifier
    ///
    /// Returns the currently active model, if known.
    fn current_model(&self) -> Option<&str> {
        None
    }

    /// Fetch available models from the provider.
    ///
    /// Async so the daemon proxy can query its models API. Default:
    /// empty list (agent doesn't expose model discovery).
    async fn fetch_available_models(&mut self) -> Vec<String> {
        Vec::new()
    }

    /// Set the thinking budget for reasoning models.
    ///
    /// Values: -1 = unlimited, 0 = disabled, >0 = max tokens
    async fn set_thinking_budget(&mut self, _budget: i64) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_thinking_budget".into()))
    }

    /// Get the current thinking budget.
    fn get_thinking_budget(&self) -> Option<i64> {
        None
    }

    async fn set_system_prompt(&mut self, _prompt: &str) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_system_prompt".into()))
    }

    fn get_system_prompt(&self) -> Option<String> {
        None
    }

    /// Downcast to [`Undoable`] for agents that support undoing turns.
    /// Default: not supported.
    fn as_undoable(&self) -> Option<&dyn super::Undoable> {
        None
    }

    /// Mutable counterpart to [`Self::as_undoable`]. Returns `None` for
    /// agents that don't implement [`Undoable`].
    fn as_undoable_mut(&mut self) -> Option<&mut dyn super::Undoable> {
        None
    }

    /// Cancel the current agent operation
    ///
    /// Propagates cancellation to the backend (e.g., daemon RPC).
    /// Default is a no-op for agents that don't support remote cancellation.
    async fn cancel(&self) -> ChatResult<()> {
        Ok(())
    }

    /// Set the temperature for response generation.
    ///
    /// Values: 0.0 = deterministic, 1.0 = balanced, 2.0 = maximum randomness
    async fn set_temperature(&mut self, _temperature: f64) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_temperature".into()))
    }

    /// Get the current temperature setting.
    fn get_temperature(&self) -> Option<f64> {
        None
    }

    /// Set the maximum tokens for response generation.
    ///
    /// Values: None = provider default, Some(n) = limit to n tokens
    async fn set_max_tokens(&mut self, _max_tokens: Option<u32>) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_max_tokens".into()))
    }

    /// Get the current max tokens setting.
    fn get_max_tokens(&self) -> Option<u32> {
        None
    }

    /// Set maximum tool-call iterations per turn. None = unlimited.
    async fn set_max_iterations(&mut self, _max_iterations: Option<u32>) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_max_iterations".into()))
    }

    /// Get the current max iterations setting.
    fn get_max_iterations(&self) -> Option<u32> {
        None
    }

    /// Set execution timeout in seconds per turn. None = no timeout.
    async fn set_execution_timeout(&mut self, _timeout_secs: Option<u64>) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_execution_timeout".into()))
    }

    /// Get the current execution timeout setting.
    fn get_execution_timeout(&self) -> Option<u64> {
        None
    }

    /// Set the context token budget. None = no limit.
    async fn set_context_budget(&mut self, _budget: Option<usize>) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_context_budget".into()))
    }

    /// Get the current context token budget.
    fn get_context_budget(&self) -> Option<usize> {
        None
    }

    /// Set the context truncation strategy.
    async fn set_context_strategy(
        &mut self,
        _strategy: crate::session::ContextStrategy,
    ) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_context_strategy".into()))
    }

    /// Get the current context truncation strategy.
    fn get_context_strategy(&self) -> crate::session::ContextStrategy {
        crate::session::ContextStrategy::default()
    }

    /// Set the sliding window size (message pairs to keep). None = default (10).
    async fn set_context_window(&mut self, _window: Option<usize>) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_context_window".into()))
    }

    /// Get the current sliding window size.
    fn get_context_window(&self) -> Option<usize> {
        None
    }

    /// Set output validation mode for agent text responses.
    async fn set_output_validation(
        &mut self,
        _validation: crate::session::OutputValidation,
    ) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_output_validation".into()))
    }

    /// Get the current output validation mode.
    fn get_output_validation(&self) -> &crate::session::OutputValidation {
        // Return a static reference to None variant
        static NONE: crate::session::OutputValidation = crate::session::OutputValidation::None;
        &NONE
    }

    /// Set maximum retry count when output validation fails.
    async fn set_validation_retries(&mut self, _retries: u32) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_validation_retries".into()))
    }

    /// Get the current validation retry count.
    fn get_validation_retries(&self) -> u32 {
        3
    }

    /// Set the auto-compaction threshold (fraction of `context_budget`).
    /// `None` resets to the daemon default; `Some(0.0)` explicitly disables.
    async fn set_autocompact_threshold(&mut self, _threshold: Option<f32>) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_autocompact_threshold".into()))
    }

    /// Get the current auto-compaction threshold. `None` indicates the
    /// daemon default is in effect.
    fn get_autocompact_threshold(&self) -> Option<f32> {
        None
    }

    /// Set the maximum number of Precognition search results.
    async fn set_precognition_results(&mut self, _count: usize) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_precognition_results".into()))
    }

    /// Get the current Precognition search results count.
    fn get_precognition_results(&self) -> usize {
        5
    }

    /// Respond to an interaction request
    ///
    /// Sends the user's response to an interaction request (Ask, Permission, etc.)
    /// back to the agent/daemon for processing.
    ///
    /// # Arguments
    /// * `request_id` - The ID of the interaction request being responded to
    /// * `response` - The user's response
    ///
    /// # Returns
    /// * `Ok(())` if the response was sent successfully
    /// * `Err(ChatError::NotSupported)` if the agent doesn't support interactions
    async fn interaction_respond(
        &mut self,
        _request_id: String,
        _response: crate::interaction::InteractionResponse,
    ) -> ChatResult<()> {
        Err(ChatError::NotSupported("interaction_respond".into()))
    }

    /// Take the interaction event receiver (if available)
    ///
    /// Returns a receiver for out-of-band interaction events. This receiver
    /// delivers `InteractionRequested` events that arrive outside of message
    /// streaming (e.g., from Lua handlers, daemon triggers).
    ///
    /// This method should be called once at startup. Subsequent calls return `None`.
    /// The caller should poll this receiver in their event loop to handle interactions.
    ///
    /// # Returns
    /// * `Some(receiver)` - On first call, if interactions are supported
    /// * `None` - On subsequent calls or if interactions are not supported
    fn take_interaction_receiver(
        &mut self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<crate::interaction::InteractionEvent>> {
        None
    }

    /// The daemon session ID, if this agent is backed by a daemon session.
    fn session_id(&self) -> Option<&str> {
        None
    }
}

/// Blanket `Agent` impl for boxed `AgentHandle` trait objects. Forwards
/// each method to the underlying handle so callers holding a
/// `Box<dyn AgentHandle + Send + Sync>` can drive it through `Agent`
/// without downcasting.
#[async_trait]
impl crate::turn::Agent for Box<dyn AgentHandle + Send + Sync> {
    fn capabilities(&self) -> crate::turn::AgentCapabilities {
        (**self).capabilities()
    }

    async fn turn<'a>(
        &'a mut self,
        ctx: crate::turn::TurnContext,
    ) -> Result<BoxStream<'a, crate::turn::TurnEvent>, crate::turn::AgentError> {
        (**self).turn(ctx).await
    }

    async fn cancel(&self) -> Result<(), crate::turn::AgentError> {
        crate::turn::Agent::cancel(&**self).await
    }

    async fn switch_model(&mut self, model_id: &str) -> Result<(), crate::turn::NotSupported> {
        crate::turn::Agent::switch_model(&mut **self, model_id).await
    }
}

/// Blanket implementation for boxed trait objects
///
/// This allows `Box<dyn AgentHandle + Send + Sync>` to be used anywhere
/// an `AgentHandle` is expected, enabling factory patterns that return
/// type-erased agents.
#[async_trait]
impl AgentHandle for Box<dyn AgentHandle + Send + Sync> {
    async fn send_message_fire_and_forget(&mut self, message: String) -> ChatResult<()> {
        (**self).send_message_fire_and_forget(message).await
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        (**self).get_modes()
    }

    fn get_mode_id(&self) -> &str {
        (**self).get_mode_id()
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        (**self).set_mode_str(mode_id).await
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        (**self).clear_history().await
    }

    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        AgentHandle::switch_model(&mut **self, model_id).await
    }

    fn current_model(&self) -> Option<&str> {
        (**self).current_model()
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        (**self).fetch_available_models().await
    }

    async fn set_thinking_budget(&mut self, budget: i64) -> ChatResult<()> {
        (**self).set_thinking_budget(budget).await
    }

    fn get_thinking_budget(&self) -> Option<i64> {
        (**self).get_thinking_budget()
    }

    async fn set_system_prompt(&mut self, prompt: &str) -> ChatResult<()> {
        (**self).set_system_prompt(prompt).await
    }

    fn get_system_prompt(&self) -> Option<String> {
        (**self).get_system_prompt()
    }

    fn as_undoable(&self) -> Option<&dyn super::Undoable> {
        (**self).as_undoable()
    }

    fn as_undoable_mut(&mut self) -> Option<&mut dyn super::Undoable> {
        (**self).as_undoable_mut()
    }

    async fn cancel(&self) -> ChatResult<()> {
        AgentHandle::cancel(&**self).await
    }

    async fn set_temperature(&mut self, temperature: f64) -> ChatResult<()> {
        (**self).set_temperature(temperature).await
    }

    fn get_temperature(&self) -> Option<f64> {
        (**self).get_temperature()
    }

    async fn set_max_tokens(&mut self, max_tokens: Option<u32>) -> ChatResult<()> {
        (**self).set_max_tokens(max_tokens).await
    }

    fn get_max_tokens(&self) -> Option<u32> {
        (**self).get_max_tokens()
    }

    async fn set_max_iterations(&mut self, max_iterations: Option<u32>) -> ChatResult<()> {
        (**self).set_max_iterations(max_iterations).await
    }

    fn get_max_iterations(&self) -> Option<u32> {
        (**self).get_max_iterations()
    }

    async fn set_execution_timeout(&mut self, timeout_secs: Option<u64>) -> ChatResult<()> {
        (**self).set_execution_timeout(timeout_secs).await
    }

    fn get_execution_timeout(&self) -> Option<u64> {
        (**self).get_execution_timeout()
    }

    async fn set_context_budget(&mut self, budget: Option<usize>) -> ChatResult<()> {
        (**self).set_context_budget(budget).await
    }

    fn get_context_budget(&self) -> Option<usize> {
        (**self).get_context_budget()
    }

    async fn set_context_strategy(
        &mut self,
        strategy: crate::session::ContextStrategy,
    ) -> ChatResult<()> {
        (**self).set_context_strategy(strategy).await
    }

    fn get_context_strategy(&self) -> crate::session::ContextStrategy {
        (**self).get_context_strategy()
    }

    async fn set_context_window(&mut self, window: Option<usize>) -> ChatResult<()> {
        (**self).set_context_window(window).await
    }

    fn get_context_window(&self) -> Option<usize> {
        (**self).get_context_window()
    }

    async fn set_output_validation(
        &mut self,
        validation: crate::session::OutputValidation,
    ) -> ChatResult<()> {
        (**self).set_output_validation(validation).await
    }

    fn get_output_validation(&self) -> &crate::session::OutputValidation {
        (**self).get_output_validation()
    }

    async fn set_validation_retries(&mut self, retries: u32) -> ChatResult<()> {
        (**self).set_validation_retries(retries).await
    }

    fn get_validation_retries(&self) -> u32 {
        (**self).get_validation_retries()
    }

    async fn set_precognition_results(&mut self, count: usize) -> ChatResult<()> {
        (**self).set_precognition_results(count).await
    }

    fn get_precognition_results(&self) -> usize {
        (**self).get_precognition_results()
    }

    async fn interaction_respond(
        &mut self,
        request_id: String,
        response: crate::interaction::InteractionResponse,
    ) -> ChatResult<()> {
        (**self).interaction_respond(request_id, response).await
    }

    fn take_interaction_receiver(
        &mut self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<crate::interaction::InteractionEvent>> {
        (**self).take_interaction_receiver()
    }

    fn session_id(&self) -> Option<&str> {
        (**self).session_id()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub name: String,
    pub arguments: Option<serde_json::Value>,
    pub id: Option<String>,
}

// Mode ID Helper Functions

pub fn is_read_only(mode_id: &str) -> bool {
    mode_id == "plan"
}

pub fn mode_display_name(mode_id: &str) -> &'static str {
    match mode_id {
        "normal" => "Normal",
        "plan" => "Plan",
        "auto" => "Auto",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_read_only() {
        assert!(is_read_only("plan"));
        assert!(!is_read_only("normal"));
    }
}
