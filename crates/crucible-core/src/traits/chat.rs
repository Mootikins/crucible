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
    /// Which kiln the note came from, by registry name.
    ///
    /// Was `kiln_label: Option<String>`, filled from the kiln directory's
    /// basename. This payload is persisted into `session.jsonl` and broadcast
    /// to the web and TUI, so that basename outlived the turn and reached two
    /// UIs. The key is renamed as well as retyped: a transcript recorded before
    /// this change holds a basename under the old key, and it must be dropped
    /// on read rather than parsed as if it were a name.
    #[serde(default)]
    pub kiln: Option<crate::config::KilnName>,
    /// Search relevance score from the vector index. Defaults for payloads
    /// recorded before the field existed.
    #[serde(default)]
    pub score: f64,
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
    /// `TurnEvent::ToolBatchEnd`, which both agent paths now emit — the
    /// genai loop after every tool batch, the ACP delegation path
    /// (`crucible-daemon/src/acp_handle.rs`) once per turn after the last
    /// tool call is announced. On the ACP side that is only for a turn that
    /// actually announced a call: a text-only turn emits no batch-end (it
    /// would claim a batch that never existed), and a turn whose only tool
    /// evidence is a completion update for a call the agent never announced
    /// has nothing to close — the handle drops that result rather than
    /// naming a tool that was never introduced, so "a batch existed" and "a
    /// result was reported" cannot disagree.
    ///
    /// The flag still has no effect on `cru chat -a claude / opencode /
    /// gemini`, for a different reason: it is produced by the scheduler's
    /// tool-dispatch path, and an `owns_history` agent never takes it. Such
    /// an agent executes the tool in its own process and the scheduler only
    /// passes the call through (`agent_manager/messaging/stream.rs`), so no
    /// `ChatToolResult` — and therefore no `terminate` — is ever produced
    /// for it. Reaching a delegated agent's tools needs a signal on the ACP
    /// wire, not another event here.
    #[serde(default)]
    pub terminate: bool,
}

impl ChatToolResult {
    /// A failed call: no result text, the error message, and the call id
    /// the model assigned, so the model can match the failure to its request.
    pub fn error(
        name: impl Into<String>,
        call_id: impl Into<String>,
        msg: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            result: String::new(),
            error: Some(msg.into()),
            call_id: Some(call_id.into()),
            terminate: false,
        }
    }
}

/// The session-scoped knobs every agent handle must answer.
///
/// Each knob is required. A client or a provider that omits one does not
/// compile. Before this split the knobs had defaults that returned `None`
/// or `NotSupported`, so a new handle compiled with every knob unwired and
/// nothing reported it (see "Session-scoped vs TUI-local" in AGENTS.md).
///
/// A handle that does not support a knob says so: the setter returns
/// `ChatError::NotSupported` and the getter returns its empty value.
/// [`impl_unsupported_session_knobs!`](crate::impl_unsupported_session_knobs)
/// writes that impl for a test double.
#[async_trait]
pub trait SessionKnobs: Send + Sync {
    /// Switch to a different model.
    ///
    /// This may recreate the underlying agent or connection. The
    /// implementation keeps the conversation history when it can.
    /// Returns `Err(ChatError::NotSupported)` when the agent cannot switch.
    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()>;

    /// The current model identifier, if known.
    fn current_model(&self) -> Option<&str>;

    /// Fetch the available models from the provider. Async so the daemon
    /// proxy can query its models API. Empty when the agent has no model
    /// discovery.
    async fn fetch_available_models(&mut self) -> Vec<String>;

    /// The mode ids this session may enter, in declaration order.
    ///
    /// Modes are declared in Lua, so a client cannot know them at compile
    /// time. Empty means "ask nobody": the caller keeps the list it already
    /// had rather than dropping to zero modes.
    async fn fetch_available_modes(&mut self) -> Vec<String>;

    /// Set the thinking budget for reasoning models.
    ///
    /// Values: -1 = unlimited, 0 = disabled, >0 = max tokens
    async fn set_thinking_budget(&mut self, budget: i64) -> ChatResult<()>;

    /// Get the current thinking budget.
    fn get_thinking_budget(&self) -> Option<i64>;

    async fn set_system_prompt(&mut self, prompt: &str) -> ChatResult<()>;

    fn get_system_prompt(&self) -> Option<String>;

    /// Set the temperature for response generation.
    ///
    /// Values: 0.0 = deterministic, 1.0 = balanced, 2.0 = maximum randomness
    async fn set_temperature(&mut self, temperature: f64) -> ChatResult<()>;

    /// Get the current temperature setting.
    fn get_temperature(&self) -> Option<f64>;

    /// Set the maximum tokens for response generation.
    ///
    /// Values: None = provider default, Some(n) = limit to n tokens
    async fn set_max_tokens(&mut self, max_tokens: Option<u32>) -> ChatResult<()>;

    /// Get the current max tokens setting.
    fn get_max_tokens(&self) -> Option<u32>;

    /// Set maximum tool-call iterations per turn. None = unlimited.
    async fn set_max_iterations(&mut self, max_iterations: Option<u32>) -> ChatResult<()>;

    /// Get the current max iterations setting.
    fn get_max_iterations(&self) -> Option<u32>;

    /// Set execution timeout in seconds per turn. None = no timeout.
    async fn set_execution_timeout(&mut self, timeout_secs: Option<u64>) -> ChatResult<()>;

    /// Get the current execution timeout setting.
    fn get_execution_timeout(&self) -> Option<u64>;

    /// Set the context token budget. None = no limit.
    async fn set_context_budget(&mut self, budget: Option<usize>) -> ChatResult<()>;

    /// Get the current context token budget.
    fn get_context_budget(&self) -> Option<usize>;

    /// Set the context truncation strategy.
    async fn set_context_strategy(
        &mut self,
        strategy: crate::session::ContextStrategy,
    ) -> ChatResult<()>;

    /// Get the current context truncation strategy.
    fn get_context_strategy(&self) -> crate::session::ContextStrategy;

    /// Set the sliding window size (message pairs to keep). None = default (10).
    async fn set_context_window(&mut self, window: Option<usize>) -> ChatResult<()>;

    /// Get the current sliding window size.
    fn get_context_window(&self) -> Option<usize>;

    /// Set output validation mode for agent text responses.
    async fn set_output_validation(
        &mut self,
        validation: crate::session::OutputValidation,
    ) -> ChatResult<()>;

    /// Get the current output validation mode.
    fn get_output_validation(&self) -> &crate::session::OutputValidation;

    /// Set maximum retry count when output validation fails.
    async fn set_validation_retries(&mut self, retries: u32) -> ChatResult<()>;

    /// Get the current validation retry count.
    fn get_validation_retries(&self) -> u32;

    /// Set the auto-compaction threshold (fraction of `context_budget`).
    /// `None` resets to the daemon default; `Some(0.0)` explicitly disables.
    async fn set_autocompact_threshold(&mut self, threshold: Option<f32>) -> ChatResult<()>;

    /// Get the current auto-compaction threshold. `None` indicates the
    /// daemon default is in effect.
    fn get_autocompact_threshold(&self) -> Option<f32>;

    /// Turn Precognition (auto-RAG context injection) on or off for this
    /// session. Session-scoped, not display state: every client attached to
    /// the session sees the change.
    async fn set_precognition(&mut self, enabled: bool) -> ChatResult<()>;

    /// Whether Precognition is currently enabled. `AgentConfig` defaults
    /// to on.
    fn get_precognition(&self) -> bool;

    /// Set the maximum number of Precognition search results.
    async fn set_precognition_results(&mut self, count: usize) -> ChatResult<()>;

    /// Get the current Precognition search results count.
    fn get_precognition_results(&self) -> usize;
}

/// The empty answer for every knob: each setter returns
/// `ChatError::NotSupported`, each getter returns its empty value.
///
/// For a test double that exercises no knob. A handle that answers one
/// knob writes the whole impl by hand, so the compiler sees every choice.
#[macro_export]
macro_rules! impl_unsupported_session_knobs {
    ($ty:ty) => {
        #[async_trait::async_trait]
        impl $crate::traits::chat::SessionKnobs for $ty {
            async fn switch_model(
                &mut self,
                _model_id: &str,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "switch_model".into(),
                ))
            }
            fn current_model(&self) -> Option<&str> {
                None
            }
            async fn fetch_available_models(&mut self) -> Vec<String> {
                Vec::new()
            }
            async fn fetch_available_modes(&mut self) -> Vec<String> {
                Vec::new()
            }
            async fn set_thinking_budget(
                &mut self,
                _budget: i64,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_thinking_budget".into(),
                ))
            }
            fn get_thinking_budget(&self) -> Option<i64> {
                None
            }
            async fn set_system_prompt(
                &mut self,
                _prompt: &str,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_system_prompt".into(),
                ))
            }
            fn get_system_prompt(&self) -> Option<String> {
                None
            }
            async fn set_temperature(
                &mut self,
                _temperature: f64,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_temperature".into(),
                ))
            }
            fn get_temperature(&self) -> Option<f64> {
                None
            }
            async fn set_max_tokens(
                &mut self,
                _max_tokens: Option<u32>,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_max_tokens".into(),
                ))
            }
            fn get_max_tokens(&self) -> Option<u32> {
                None
            }
            async fn set_max_iterations(
                &mut self,
                _max_iterations: Option<u32>,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_max_iterations".into(),
                ))
            }
            fn get_max_iterations(&self) -> Option<u32> {
                None
            }
            async fn set_execution_timeout(
                &mut self,
                _timeout_secs: Option<u64>,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_execution_timeout".into(),
                ))
            }
            fn get_execution_timeout(&self) -> Option<u64> {
                None
            }
            async fn set_context_budget(
                &mut self,
                _budget: Option<usize>,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_context_budget".into(),
                ))
            }
            fn get_context_budget(&self) -> Option<usize> {
                None
            }
            async fn set_context_strategy(
                &mut self,
                _strategy: $crate::session::ContextStrategy,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_context_strategy".into(),
                ))
            }
            fn get_context_strategy(&self) -> $crate::session::ContextStrategy {
                $crate::session::ContextStrategy::default()
            }
            async fn set_context_window(
                &mut self,
                _window: Option<usize>,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_context_window".into(),
                ))
            }
            fn get_context_window(&self) -> Option<usize> {
                None
            }
            async fn set_output_validation(
                &mut self,
                _validation: $crate::session::OutputValidation,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_output_validation".into(),
                ))
            }
            fn get_output_validation(&self) -> &$crate::session::OutputValidation {
                &$crate::session::OutputValidation::None
            }
            async fn set_validation_retries(
                &mut self,
                _retries: u32,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_validation_retries".into(),
                ))
            }
            fn get_validation_retries(&self) -> u32 {
                3
            }
            async fn set_autocompact_threshold(
                &mut self,
                _threshold: Option<f32>,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_autocompact_threshold".into(),
                ))
            }
            fn get_autocompact_threshold(&self) -> Option<f32> {
                None
            }
            async fn set_precognition(
                &mut self,
                _enabled: bool,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_precognition".into(),
                ))
            }
            fn get_precognition(&self) -> bool {
                true
            }
            async fn set_precognition_results(
                &mut self,
                _count: usize,
            ) -> $crate::traits::chat::ChatResult<()> {
                Err($crate::traits::chat::ChatError::NotSupported(
                    "set_precognition_results".into(),
                ))
            }
            fn get_precognition_results(&self) -> usize {
                5
            }
        }
    };
}

/// Runtime handle to an active agent.
///
/// `AgentHandle` is a supertrait of [`Agent`](crate::turn::Agent): every
/// handle must also expose the lean `Agent` surface (`capabilities`,
/// `turn`, `cancel`, `switch_model`). The session knobs live in
/// [`SessionKnobs`], a second supertrait, and every knob is required.
/// What stays here is the message path, the mode, and the optional
/// capabilities (undo, interactions, a daemon session id) whose `None`
/// answer is a true answer.
#[async_trait]
pub trait AgentHandle: crate::turn::Agent + SessionKnobs + Send + Sync {
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

    /// The mode this handle is currently in.
    ///
    /// Deliberately has no default. It defaulted to `"plan"`, which was
    /// harmless while plan only filtered the tool set — but once plan carried
    /// a deny rule, any handle that tracks no mode denied everything. A mock
    /// that forgets to answer should fail to compile, not fail closed at
    /// runtime in one test rig.
    fn get_mode_id(&self) -> &str;

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()>;

    /// Daemon-internal mirror sync. The authoritative `AgentManager::set_mode`
    /// persists the mode on the session's agent_config and emits the
    /// `mode_changed` event itself; what's left is to update the cached
    /// handle's local mode mirror. `apply_mode` is that mirror-only update.
    ///
    /// This is distinct from `set_mode_str`, which is the *external* entry
    /// point — a client (TUI, web) calls `set_mode_str` to ask the daemon to
    /// change the mode AND propagate to whatever backing store the handle
    /// fronts. Calling `set_mode_str` from inside `AgentManager::set_mode`
    /// would, for handle types whose backing store IS the daemon itself
    /// (e.g. a `DaemonAgentHandle` in test setups), re-enter the dispatch
    /// path holding the cached handle's mutex.
    ///
    /// Default: delegate to `set_mode_str`. Handle types whose backing
    /// store is the daemon itself override to update only the local mirror.
    async fn apply_mode(&mut self, mode_id: &str) -> ChatResult<()> {
        self.set_mode_str(mode_id).await
    }

    /// The external (ACP) agent's own session id, when this handle fronts
    /// one.
    ///
    /// The daemon persists the id on the session, so a handle built after
    /// a restart can send `session/resume`. The `None` default is a true
    /// answer, not a stub: an internal agent has no external session.
    fn acp_session_id(&self) -> Option<String> {
        None
    }

    /// Clear the conversation history.
    ///
    /// Resets the agent's conversation context. The caller clears the UI
    /// state separately. Required: a silent `Ok(())` default let a handle
    /// that owns its history report a clear it never did. A handle that
    /// cannot clear returns `ChatError::NotSupported`.
    async fn clear_history(&mut self) -> ChatResult<()>;

    /// Undo up to `count` turns. Returns one summary per turn removed.
    ///
    /// The default refuses, because only an agent that tracks conversation
    /// state can rewind it.
    async fn undo(&mut self, count: usize) -> ChatResult<Vec<crate::types::UndoSummary>> {
        let _ = count;
        Err(ChatError::NotSupported("undo".to_string()))
    }

    /// Cancel the current agent operation
    ///
    /// Propagates cancellation to the backend (e.g., daemon RPC).
    /// Default is a no-op for agents that don't support remote cancellation.
    async fn cancel(&self) -> ChatResult<()> {
        Ok(())
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

/// Forward every knob through the box, so a `Box<dyn AgentHandle>` answers
/// `SessionKnobs` like the handle inside it.
#[async_trait]
impl SessionKnobs for Box<dyn AgentHandle + Send + Sync> {
    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        SessionKnobs::switch_model(&mut **self, model_id).await
    }

    fn current_model(&self) -> Option<&str> {
        (**self).current_model()
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        (**self).fetch_available_models().await
    }

    async fn fetch_available_modes(&mut self) -> Vec<String> {
        (**self).fetch_available_modes().await
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

    async fn set_autocompact_threshold(&mut self, threshold: Option<f32>) -> ChatResult<()> {
        (**self).set_autocompact_threshold(threshold).await
    }

    fn get_autocompact_threshold(&self) -> Option<f32> {
        (**self).get_autocompact_threshold()
    }

    async fn set_precognition(&mut self, enabled: bool) -> ChatResult<()> {
        (**self).set_precognition(enabled).await
    }

    fn get_precognition(&self) -> bool {
        (**self).get_precognition()
    }

    async fn set_precognition_results(&mut self, count: usize) -> ChatResult<()> {
        (**self).set_precognition_results(count).await
    }

    fn get_precognition_results(&self) -> usize {
        (**self).get_precognition_results()
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

    async fn apply_mode(&mut self, mode_id: &str) -> ChatResult<()> {
        (**self).apply_mode(mode_id).await
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        (**self).clear_history().await
    }

    // Forwarded, not defaulted: without this line the box answers the
    // trait default `None` for every inner handle, ACP ones included.
    fn acp_session_id(&self) -> Option<String> {
        (**self).acp_session_id()
    }

    async fn undo(&mut self, count: usize) -> ChatResult<Vec<crate::types::UndoSummary>> {
        (**self).undo(count).await
    }

    async fn cancel(&self) -> ChatResult<()> {
        AgentHandle::cancel(&**self).await
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
