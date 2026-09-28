//! The runtime handle of an agent, and the session knobs that it answers.
//!
//! Only the daemon runs agents, so only the daemon owns these traits. A
//! client drives a session through `DaemonClient` RPCs, never through an
//! agent handle.

use async_trait::async_trait;
use futures::stream::BoxStream;

use crucible_core::traits::chat::{ChatError, ChatResult};
use crucible_core::types::acp::schema::SessionModeState;

/// The session-scoped knobs every agent handle must answer.
///
/// Each knob is required. A client or a provider that omits one does not
/// compile. Before this split the knobs had defaults that returned `None`
/// or `NotSupported`, so a new handle compiled with every knob unwired and
/// nothing reported it (see the cross-layer checklist in AGENTS.md).
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

    /// Set one of the settings the external agent advertised for itself.
    ///
    /// The value is the agent's own id for the choice, or `"true"`/`"false"`
    /// for a toggle. The refusing default is the true answer for an agent
    /// that advertises nothing: there is no option to set.
    async fn set_agent_config_option(&mut self, id: &str, value: &str) -> ChatResult<()> {
        let _ = (id, value);
        Err(ChatError::NotSupported("set_agent_config_option".into()))
    }

    /// The session settings the external agent advertised for itself.
    ///
    /// ACP agents list these in the `session/new` reply; the model selector
    /// is one of them, and `thought_level` is another. `thought_level`
    /// belongs to the agent: Crucible has no equivalent knob and does not
    /// interpret it. The empty default is a true answer, not a stub: an
    /// internal agent advertises nothing, because Crucible defines its
    /// settings rather than discovering them.
    fn agent_config_options(&self) -> &[crucible_core::types::acp::schema::SessionConfigOption] {
        &[]
    }

    /// The current model identifier, if known.
    fn current_model(&self) -> Option<&str>;

    /// Fetch the available models from the provider. Async so the daemon
    /// proxy can query its models API. Empty when the agent has no model
    /// discovery.
    async fn fetch_available_models(&mut self) -> Vec<String>;

    /// The modes this session may enter, in declaration order.
    ///
    /// Modes are declared in Lua, so a client cannot know them at compile
    /// time. Empty means "ask nobody": the caller keeps the list it already
    /// had rather than dropping to zero modes. Each descriptor carries the
    /// effective values, such as `writes`, so a client shows what happens.
    async fn fetch_available_modes(&mut self) -> Vec<crucible_core::types::mode::ModeDescriptor>;

    /// The system prompt the handle was built with.
    ///
    /// Read-only: the prompt comes from config, an agent card or Lua, never
    /// from a runtime setter. It stays on the trait because it is the one
    /// place a test can prove that AGENTS.md rules reached the model.
    fn get_system_prompt(&self) -> Option<String>;

    /// Set the context truncation strategy.
    async fn set_context_strategy(
        &mut self,
        strategy: crucible_core::session::ContextStrategy,
    ) -> ChatResult<()>;

    /// Get the current context truncation strategy.
    fn get_context_strategy(&self) -> crucible_core::session::ContextStrategy;

    /// Turn Precognition (auto-RAG context injection) on or off for this
    /// session. Session-scoped, not display state: every client attached to
    /// the session sees the change.
    async fn set_precognition(&mut self, enabled: bool) -> ChatResult<()>;

    /// Whether Precognition is currently enabled. `AgentConfig` defaults
    /// to on.
    fn get_precognition(&self) -> bool;

    /// Set the permission floor for one plugin's turns.
    async fn set_plugin_approval(
        &mut self,
        plugin: &str,
        approval: crucible_core::session::PluginApproval,
    ) -> ChatResult<()>;

    /// Read the permission floor for one plugin; absence means inherit.
    fn get_plugin_approval(&self, plugin: &str) -> crucible_core::session::PluginApproval;

    /// Set the maximum number of consecutive plugin turns before approval rises to Ask.
    async fn set_plugin_turn_limit(&mut self, limit: u32) -> ChatResult<()>;

    /// Read the current session's plugin turn limit.
    fn get_plugin_turn_limit(&self) -> u32;
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
        impl $crate::agent_manager::SessionKnobs for $ty {
            async fn switch_model(
                &mut self,
                _model_id: &str,
            ) -> crucible_core::traits::chat::ChatResult<()> {
                Err(crucible_core::traits::chat::ChatError::NotSupported(
                    "switch_model".into(),
                ))
            }
            fn current_model(&self) -> Option<&str> {
                None
            }
            async fn fetch_available_models(&mut self) -> Vec<String> {
                Vec::new()
            }
            async fn fetch_available_modes(
                &mut self,
            ) -> Vec<crucible_core::types::mode::ModeDescriptor> {
                Vec::new()
            }
            async fn set_context_strategy(
                &mut self,
                _strategy: crucible_core::session::ContextStrategy,
            ) -> crucible_core::traits::chat::ChatResult<()> {
                Err(crucible_core::traits::chat::ChatError::NotSupported(
                    "set_context_strategy".into(),
                ))
            }
            fn get_system_prompt(&self) -> Option<String> {
                None
            }
            fn get_context_strategy(&self) -> crucible_core::session::ContextStrategy {
                crucible_core::session::ContextStrategy::default()
            }
            async fn set_precognition(
                &mut self,
                _enabled: bool,
            ) -> crucible_core::traits::chat::ChatResult<()> {
                Err(crucible_core::traits::chat::ChatError::NotSupported(
                    "set_precognition".into(),
                ))
            }
            fn get_precognition(&self) -> bool {
                true
            }
            async fn set_plugin_approval(
                &mut self,
                _plugin: &str,
                _approval: crucible_core::session::PluginApproval,
            ) -> crucible_core::traits::chat::ChatResult<()> {
                Err(crucible_core::traits::chat::ChatError::NotSupported(
                    "set_plugin_approval".into(),
                ))
            }
            fn get_plugin_approval(&self, _plugin: &str) -> crucible_core::session::PluginApproval {
                crucible_core::session::PluginApproval::Inherit
            }
            async fn set_plugin_turn_limit(
                &mut self,
                _limit: u32,
            ) -> crucible_core::traits::chat::ChatResult<()> {
                Err(crucible_core::traits::chat::ChatError::NotSupported(
                    "set_plugin_turn_limit".into(),
                ))
            }
            fn get_plugin_turn_limit(&self) -> u32 {
                25
            }
        }
    };
}

/// Runtime handle to an active agent.
///
/// `AgentHandle` is a supertrait of [`Agent`](crucible_core::turn::Agent): every
/// handle must also expose the lean `Agent` surface (`capabilities`,
/// `turn`, `cancel`, `switch_model`). The session knobs live in
/// [`SessionKnobs`], a second supertrait, and every knob is required.
/// What stays here is the message path, the mode, and the optional
/// capabilities (undo, interactions, a daemon session id) whose `None`
/// answer is a true answer.
#[async_trait]
pub trait AgentHandle: crucible_core::turn::Agent + SessionKnobs + Send + Sync {
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
    /// This is distinct from `set_mode_str`, which changes the mode AND
    /// propagates it to whatever backing store the handle fronts. A handle
    /// whose `set_mode_str` calls back into `AgentManager::set_mode` would
    /// re-enter the dispatch path while it holds the cached handle's mutex.
    ///
    /// Default: delegate to `set_mode_str`. A handle whose `set_mode_str`
    /// calls back into the daemon overrides this to update only its mirror.
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

    /// Undo up to `count` turns. Returns one summary per turn removed.
    ///
    /// The default refuses, because only an agent that tracks conversation
    /// state can rewind it.
    async fn undo(&mut self, count: usize) -> ChatResult<Vec<crucible_core::types::UndoSummary>> {
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
}

/// Blanket `Agent` impl for boxed `AgentHandle` trait objects. Forwards
/// each method to the underlying handle so callers holding a
/// `Box<dyn AgentHandle + Send + Sync>` can drive it through `Agent`
/// without downcasting.
#[async_trait]
impl crucible_core::turn::Agent for Box<dyn AgentHandle + Send + Sync> {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        (**self).capabilities()
    }

    async fn turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> Result<BoxStream<'a, crucible_core::turn::TurnEvent>, crucible_core::turn::AgentError>
    {
        (**self).turn(ctx).await
    }

    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        crucible_core::turn::Agent::cancel(&**self).await
    }

    async fn switch_model(
        &mut self,
        model_id: &str,
    ) -> Result<(), crucible_core::turn::NotSupported> {
        crucible_core::turn::Agent::switch_model(&mut **self, model_id).await
    }
}

/// Forward every knob through the box, so a `Box<dyn AgentHandle>` answers
/// `SessionKnobs` like the handle inside it.
#[async_trait]
impl SessionKnobs for Box<dyn AgentHandle + Send + Sync> {
    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        SessionKnobs::switch_model(&mut **self, model_id).await
    }

    async fn set_plugin_approval(
        &mut self,
        plugin: &str,
        approval: crucible_core::session::PluginApproval,
    ) -> ChatResult<()> {
        (**self).set_plugin_approval(plugin, approval).await
    }

    fn get_plugin_approval(&self, plugin: &str) -> crucible_core::session::PluginApproval {
        (**self).get_plugin_approval(plugin)
    }

    async fn set_plugin_turn_limit(&mut self, limit: u32) -> ChatResult<()> {
        (**self).set_plugin_turn_limit(limit).await
    }

    fn get_plugin_turn_limit(&self) -> u32 {
        (**self).get_plugin_turn_limit()
    }

    // Every DEFAULTED method on the trait has to be repeated here, and the
    // compiler does not say so. This impl shadows the defaults, so a defaulted
    // method left out of it answers the default for every boxed handle in the
    // daemon and the concrete impl underneath is never reached.
    //
    // That has now happened twice: `get_modes` shipped correct and
    // unreachable, and these two read `&[]` and `NotSupported` for a live ACP
    // agent until they were added here. Nothing enumerates a trait's defaulted
    // methods, so there is no general guard — the test that catches this pair
    // is `acp_session_knobs_e2e::the_agents_own_settings_reach_a_client`,
    // which drives a real agent process and would see the default.
    //
    // Add a defaulted method to `SessionKnobs`, add it here too.
    fn agent_config_options(&self) -> &[crucible_core::types::acp::schema::SessionConfigOption] {
        (**self).agent_config_options()
    }

    async fn set_agent_config_option(&mut self, id: &str, value: &str) -> ChatResult<()> {
        (**self).set_agent_config_option(id, value).await
    }

    fn current_model(&self) -> Option<&str> {
        (**self).current_model()
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        (**self).fetch_available_models().await
    }

    async fn fetch_available_modes(&mut self) -> Vec<crucible_core::types::mode::ModeDescriptor> {
        (**self).fetch_available_modes().await
    }

    async fn set_context_strategy(
        &mut self,
        strategy: crucible_core::session::ContextStrategy,
    ) -> ChatResult<()> {
        (**self).set_context_strategy(strategy).await
    }

    fn get_system_prompt(&self) -> Option<String> {
        (**self).get_system_prompt()
    }

    fn get_context_strategy(&self) -> crucible_core::session::ContextStrategy {
        (**self).get_context_strategy()
    }

    async fn set_precognition(&mut self, enabled: bool) -> ChatResult<()> {
        (**self).set_precognition(enabled).await
    }

    fn get_precognition(&self) -> bool {
        (**self).get_precognition()
    }
}

/// Blanket implementation for boxed trait objects
///
/// This allows `Box<dyn AgentHandle + Send + Sync>` to be used anywhere
/// an `AgentHandle` is expected, enabling factory patterns that return
/// type-erased agents.
#[async_trait]
impl AgentHandle for Box<dyn AgentHandle + Send + Sync> {
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

    // Forwarded, not defaulted: without this line the box answers the
    // trait default `None` for every inner handle, ACP ones included.
    fn acp_session_id(&self) -> Option<String> {
        (**self).acp_session_id()
    }

    async fn undo(&mut self, count: usize) -> ChatResult<Vec<crucible_core::types::UndoSummary>> {
        (**self).undo(count).await
    }

    async fn cancel(&self) -> ChatResult<()> {
        AgentHandle::cancel(&**self).await
    }
}
