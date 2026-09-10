//! What a session's config starts as, and who decides it.
//!
//! Split from `agent_manager/mod.rs` for the 1500-line file budget, along a
//! real seam: everything here is about the values a session begins with and
//! the one place they are written down.
//!
//! Sessions run no Lua. The daemon VM loads the runtimepath's defaults file
//! and then `~/.config/crucible/init.lua`, once, at boot — a later file wins
//! by ordinary assignment, and `cru.modes.x = nil` removes. No workspace file
//! is on that list: a workspace is data the agent reads, not code the daemon
//! executes, and `execution_roots` can only protect a tree a loader names in
//! advance.
//!
//! `on_session_start` fires once per session, from `SessionLifecycle`, and
//! writes into the scope [`AgentManager::start_hook_scope`] hands it.
//! [`AgentManager::apply_session_defaults`] reads that scope back when the
//! agent is built — the global tier is the fallback, not the source.
//!
//! That global tier is the config store: `chat.system_prompt`. It is read
//! live, not snapshotted at boot, because `config.set` and the settings UI
//! both write the store while the daemon runs.

use super::*;

impl AgentManager {
    /// The scope a session's `on_session_start` hooks write into, and the
    /// variables they read.
    ///
    /// Seeded from the config store so `session.x` reads the inherited value
    /// — `session.system_prompt = session.system_prompt .. "…"` extends the
    /// configured prompt instead of clobbering it.
    ///
    /// Paired with [`Self::commit_start_hook_scope`]. `SessionLifecycle` runs
    /// the hooks between the two, because that is where the plugin loader and
    /// the `required = true` refusal live.
    pub(crate) fn start_hook_scope(
        &self,
        session_id: &str,
    ) -> (
        crucible_lua::SessionStartScope,
        crucible_lua::SessionVariables,
    ) {
        let scope = crucible_lua::SessionStartScope::new();
        scope.set(configured_start_values());

        // Seed the variables from storage before the hooks run, so
        // `session:get_variable` reads what an earlier life of this session
        // stored. A fresh manager over the same storage — a resume — has an
        // empty slot until this happens.
        let variables = self.slot(session_id).variables();
        if let Some(stored) = self.session_manager.get_session(session_id) {
            variables.replace(stored.variables);
        }
        (scope, variables)
    }

    /// Record what the start hooks chose. `apply_session_defaults` reads it.
    pub(crate) fn commit_start_hook_scope(
        &self,
        session_id: &str,
        scope: &crucible_lua::SessionStartScope,
    ) {
        self.slot(session_id).set_overrides(scope.get());
        self.schedule_variable_persist(session_id);
    }

    /// Persist the variables a start hook stored, from the synchronous VM
    /// builder. The Lua setter is synchronous and storage is not, so the map
    /// lives on the slot and reaches `meta.json` from this task. A builder
    /// that runs outside a runtime (a unit test) leaves the map on the slot;
    /// `configure_agent` merges it with its own save.
    fn schedule_variable_persist(&self, session_id: &str) {
        let slot = self.slot(session_id);
        let stored = self
            .session_manager
            .get_session(session_id)
            .map(|s| s.variables)
            .unwrap_or_default();
        if slot.variables().snapshot() == stored {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let session_manager = Arc::clone(&self.session_manager);
        let session_id = session_id.to_string();
        handle.spawn(async move {
            if let Err(e) = persist_variables(&session_manager, &slot, &session_id).await {
                warn!(session_id = %session_id, error = %e, "Could not persist session variables");
            }
        });
    }

    fn apply_session_defaults(&self, session_id: &str, mut agent: SessionAgent) -> SessionAgent {
        // Creating the VM ran `on_session_start`, which captured this
        // session's values into the slot's `overrides` — already seeded from
        // the store, so it is the complete picture. Fall back to the store
        // itself only if no VM state was recorded (a manager whose VM
        // construction failed outright).
        let defaults = self
            .slot(session_id)
            .overrides()
            .unwrap_or_else(configured_start_values);

        if agent.system_prompt.is_empty() {
            if let Some(prompt) = defaults.system_prompt {
                agent.system_prompt = prompt;
            }
        }
        agent.mode = agent.mode.or(defaults.mode);
        // `model` is never empty on the incoming agent, so a hook's choice
        // replaces it instead of filling a gap.
        if let Some(model) = defaults.model {
            agent.model = model;
        }
        agent
    }

    pub async fn configure_agent(
        &self,
        session_id: &str,
        agent: SessionAgent,
    ) -> Result<(), AgentError> {
        let mut session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        // The same gate `switch_model` applies, and for the same reason:
        // configure_agent is the other way a session's provider changes after
        // its kilns have already passed the attach-time trust check.
        //
        // Checked on the incoming agent, before `apply_session_defaults` — the
        // defaults only fill in prompt/mode/model, none of which
        // `resolve_provider_trust` reads, and refusing
        // first avoids spinning up a session Lua VM for a call that cannot
        // succeed.
        self.refuse_untrusted_for_attached_kilns(&session, &agent)?;

        let agent = self.apply_session_defaults(session_id, agent);
        session.agent = Some(agent.clone());
        // The VM may just have run the start hooks; save what they stored
        // with this write instead of racing the scheduled one.
        session.variables = self.slot(session_id).variables().snapshot();

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        info!(
            session_id = %session_id,
            model = %agent.model,
            provider = %agent.provider,
            "Agent configured for session"
        );

        Ok(())
    }
}

/// Copy the slot's variable map into the session and save it. A no-op when
/// the session already holds the same map.
async fn persist_variables(
    session_manager: &SessionManager,
    slot: &slot::SessionSlot,
    session_id: &str,
) -> Result<(), AgentError> {
    let mut session = session_manager
        .get_session(session_id)
        .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;
    let variables = slot.variables().snapshot();
    if session.variables == variables {
        return Ok(());
    }
    session.variables = variables;
    session_manager
        .update_session(&session)
        .await
        .map_err(AgentError::Session)
}

/// The values a session starts from before any hook runs.
///
/// Only `system_prompt` has a global tier. `mode` and `model` are deliberately
/// per-session: a hook chooses them, because a global `model` would silently
/// replace the one the caller named on the command line.
///
/// Read from the live store rather than a boot snapshot, so a `config.set` RPC
/// or a settings-UI save reaches the next session without a restart.
fn configured_start_values() -> crucible_lua::SessionStartValues {
    crucible_lua::SessionStartValues {
        system_prompt: configured_system_prompt(),
        ..Default::default()
    }
}

/// `chat.system_prompt` from the config store.
///
/// Falls back to the compiled-in constant, which is the same string the store
/// carries on its `Default` layer. The fallback runs only before the store is
/// seeded — a test that builds a VM directly, rather than booting a daemon.
fn configured_system_prompt() -> Option<String> {
    let configured = crucible_lua::get_app_config()
        .as_ref()
        .and_then(|config| crucible_core::config::leaf_at(config, "chat.system_prompt"))
        .and_then(|leaf| leaf.as_str())
        .map(str::to_string);
    Some(configured.unwrap_or_else(|| {
        crucible_core::config::components::chat::DEFAULT_SYSTEM_PROMPT.to_string()
    }))
}
