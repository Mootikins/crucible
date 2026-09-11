//! The scope an `on_session_start` hook writes into.
//!
//! Sessions run no Lua of their own. `on_session_start` fires once per
//! session, before the agent exists, so `session.x` in a hook cannot
//! reconfigure a live agent — it chooses what the agent is BUILT with. Those
//! choices land here, and `AgentManager::apply_session_defaults` reads them
//! back.
//!
//! ## The two tiers
//!
//! | Neovim   | Crucible                                 |
//! |----------|------------------------------------------|
//! | `vim.o`  | `chat.system_prompt` in the config store |
//! | `vim.bo` | `session.system_prompt` (this)           |
//!
//! The global tier is the config store itself. It used to be `cru.defaults`,
//! a second store that held one key, kept its own `RwLock`, and carried no
//! provenance — so `settings.json`, `:set`, `config.origin` and the settings
//! UI all passed it by. `AgentManager::start_hook_scope` seeds this scope
//! from the store, which is why a hook still reads the inherited value before
//! it overrides:
//!
//! ```lua
//! cru.on_session_start(function(session)
//!   session.system_prompt = session.system_prompt .. "\n\nCite ticket IDs."
//! end)
//! ```

use std::sync::{Arc, RwLock};

use crate::session_api::{SessionConfigRpc, SessionVariables, UnsupportedSessionRpc};

/// Session settings that carry a global default. Every field is `Option`
/// because "unset" is meaningful: a `None` default leaves whatever the agent
/// card or session already specified untouched.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionStartValues {
    pub system_prompt: Option<String>,
    /// Starting mode. Settable from an `on_session_start` hook, which is why
    /// it lives here rather than only on `SessionAgent`: the hook runs before
    /// the agent is built, so it is choosing what the agent starts as.
    pub mode: Option<String>,
    /// Model the agent starts with. Only an `on_session_start` hook sets it,
    /// through `session.model`. `chat.model` is the config key, and it is a
    /// separate tier on purpose: `apply_session_defaults` never overwrites the
    /// model a caller named on the command line, while a hook is a deliberate
    /// per-session choice and does.
    pub model: Option<String>,
}

/// Shared handle to one session's start scope.
///
/// Cheap to clone; the daemon VM and every reader share one store, so a
/// default set by one file is visible to the daemon regardless of which VM ran
/// it. This is plain process-local state, not an RPC surface — unlike
/// `session.x`, there is no per-session actor to route through.
#[derive(Debug, Clone, Default)]
pub struct SessionStartScope {
    inner: Arc<RwLock<SessionStartValues>>,
}

impl SessionStartScope {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> SessionStartValues {
        self.inner
            .read()
            .expect("session defaults: poisoned")
            .clone()
    }

    /// Replace every value at once. Used by tests and by config seeding.
    pub fn set(&self, values: SessionStartValues) {
        *self.inner.write().expect("session defaults: poisoned") = values;
    }

    fn update<F: FnOnce(&mut SessionStartValues)>(&self, f: F) {
        f(&mut self.inner.write().expect("session defaults: poisoned"));
    }
}

/// A [`crate::SessionConfigRpc`] whose reads and writes land in a
/// [`SessionStartScope`] rather than a live session.
///
/// This is what `session.x` means inside an `on_session_start` hook: the
/// session has no agent yet, so there is nothing to reconfigure — the hook is
/// choosing the values the agent will be *built* with. Seeded from the global
/// defaults, so a hook reads the inherited value before overriding it, exactly
/// like a Neovim `FileType` autocmd seeing the global option before setting
/// the buffer-local one:
///
/// ```lua
/// cru.on_session_start(function(session)
///   session.system_prompt = session.system_prompt .. "\n\nCite ticket IDs."
/// end)
/// ```
pub struct SessionStartScopeRpc {
    store: SessionStartScope,
    /// Where `session:set_variable` lands. `None` reports the knob as
    /// unsupported, the same way the model knobs do.
    variables: Option<SessionVariables>,
}

impl SessionStartScopeRpc {
    pub fn new(store: SessionStartScope) -> Self {
        Self {
            store,
            variables: None,
        }
    }

    /// Give `session:set_variable` and `session:get_variable` a store.
    #[must_use]
    pub fn with_variables(mut self, variables: SessionVariables) -> Self {
        self.variables = Some(variables);
        self
    }
}

impl SessionConfigRpc for SessionStartScopeRpc {
    fn get_system_prompt(&self) -> Option<String> {
        self.store.get().system_prompt
    }

    fn set_system_prompt(&self, prompt: &str) -> Result<(), String> {
        self.store
            .update(|v| v.system_prompt = Some(prompt.to_string()));
        Ok(())
    }

    fn get_mode(&self) -> String {
        self.store.get().mode.unwrap_or_else(|| "chat".to_string())
    }

    fn set_mode(&self, mode: &str) -> Result<(), String> {
        self.store.update(|v| v.mode = Some(mode.to_string()));
        Ok(())
    }

    fn get_model(&self) -> Option<String> {
        self.store.get().model
    }

    fn switch_model(&self, model: &str) -> Result<(), String> {
        self.store.update(|v| v.model = Some(model.to_string()));
        Ok(())
    }

    fn set_variable(&self, key: &str, value: serde_json::Value) -> Result<(), String> {
        match &self.variables {
            Some(variables) => {
                variables.set(key, value);
                Ok(())
            }
            None => UnsupportedSessionRpc.set_variable(key, value),
        }
    }

    fn get_variable(&self, key: &str) -> Option<serde_json::Value> {
        self.variables.as_ref().and_then(|v| v.get(key))
    }
}

#[cfg(test)]
mod session_start_hook_tests {
    use super::*;
    /// Setting the mode in an `on_session_start` hook must work, and must not
    /// take the rest of the hook down with it.
    ///
    /// `SessionConfigRpc`'s setters used to default to `Ok(())`, so
    /// `session.mode = "plan"` was silently inert. Defaulting them to an error
    /// made the assignment *raise*, which aborts the whole hook body — so the
    /// `system_prompt` line after it, which had always worked, stopped
    /// running. Silence was wrong; taking the rest of the hook with it is
    /// worse. `mode` is a real session default now.
    #[test]
    fn setting_mode_does_not_abort_the_rest_of_the_hook() {
        let store = SessionStartScope::new();
        let rpc = SessionStartScopeRpc::new(store.clone());

        rpc.set_mode("plan").expect("mode is a session default");
        rpc.set_system_prompt("after the mode line")
            .expect("the line after must still run");

        assert_eq!(store.get().mode.as_deref(), Some("plan"));
        assert_eq!(
            store.get().system_prompt.as_deref(),
            Some("after the mode line")
        );
        assert_eq!(rpc.get_mode(), "plan", "and reads back");
    }

    /// A hook picks the model the agent starts with. `session.model = "x"`
    /// lands in the scoped store, so `apply_session_defaults` sees it.
    #[test]
    fn switching_the_model_stores_it_as_a_session_default() {
        let store = SessionStartScope::new();
        let rpc = SessionStartScopeRpc::new(store.clone());

        rpc.switch_model("claude-sonnet-4")
            .expect("model is a session default");

        assert_eq!(store.get().model.as_deref(), Some("claude-sonnet-4"));
        assert_eq!(rpc.get_model().as_deref(), Some("claude-sonnet-4"));
    }
}
