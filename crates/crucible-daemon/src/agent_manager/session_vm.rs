//! The per-session Lua VM, and the session config it seeds.
//!
//! Split from `agent_manager/mod.rs` for the 1500-line file budget, along a
//! real seam rather than an arbitrary one: everything here is about standing
//! up a session's Lua state and reading what that Lua decided.
//!
//! A VM is its list of sources, executed in order: the defaults file the
//! runtimepath resolves, then the user's config. Nothing merges and nothing
//! re-applies — a later file wins by ordinary assignment.
//!
//! No workspace file is on that list. A workspace is data the agent reads,
//! not code the daemon executes, and `execution_roots` can only protect a
//! tree that a loader names in advance.
//!
//! `~/.config/crucible/init.lua` is the second source, so a value it sets
//! wins and `cru.modes.x = nil` removes. It also runs at boot on the daemon
//! VM (`daemon_plugins/boot.rs`) against the same `cru.defaults` and
//! `cru.modes` stores; one file, two VMs, one order each.

use super::*;

impl AgentManager {
    /// The scope a session's `on_session_start` hooks write into, and the
    /// variables they read.
    ///
    /// Seeded from the global defaults so `session.x` reads the inherited
    /// value — `session.system_prompt = session.system_prompt .. "…"` extends
    /// the default instead of clobbering it.
    ///
    /// Paired with [`Self::commit_start_hook_scope`]. `SessionLifecycle` runs
    /// the hooks between the two, because that is where the plugin loader and
    /// the `required = true` refusal live.
    pub(crate) fn start_hook_scope(
        &self,
        session_id: &str,
    ) -> (
        crucible_lua::SessionDefaults,
        crucible_lua::SessionVariables,
    ) {
        let scope = crucible_lua::SessionDefaults::new();
        scope.set(self.session_defaults.get());

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
        scope: &crucible_lua::SessionDefaults,
    ) {
        self.slot(session_id).set_overrides(scope.get());
        self.schedule_variable_persist(session_id);
    }

    pub(in crate::agent_manager) fn get_or_create_session_state(
        &self,
        session_id: &str,
    ) -> Arc<Mutex<SessionEventState>> {
        let slot = self.slot(session_id);
        // `get_or_init`, not check-then-insert: two concurrent first turns on a
        // session used to build two VMs and keep whichever inserted last,
        // silently discarding every handler registered on the other.
        slot.lua
            .get_or_init(|| self.build_session_state(session_id))
            .clone()
    }

    /// Per-session state, and the one-time work that goes with a session's
    /// first turn.
    ///
    /// There is no per-session Lua VM. Every file runs once, on the daemon VM
    /// (`daemon_plugins::boot`), and a hook receives its session as an
    /// argument rather than by living in a VM that belongs to it. What is
    /// genuinely per session is the starting values `on_session_start`
    /// chooses, so that is what this builds.
    fn build_session_state(&self, _session_id: &str) -> Arc<Mutex<SessionEventState>> {
        // `on_session_start` does NOT fire here. `SessionLifecycle` fires it
        // once, at session create, against the same daemon VM — firing again
        // here ran every hook twice, with a different session binding each
        // time. This builds only what a session owns.
        Arc::new(Mutex::new(SessionEventState {
            spill_counter: std::sync::atomic::AtomicU32::new(1),
        }))
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
        let _vm = self.get_or_create_session_state(session_id);
        // Creating the VM ran `on_session_start`, which captured this
        // session's values into the slot's `overrides` — already seeded from the
        // globals, so it is the complete picture. Fall back to the raw globals
        // only if no VM state was recorded (a manager whose VM construction
        // failed outright).
        let defaults = self
            .slot(session_id)
            .overrides()
            .unwrap_or_else(|| self.session_defaults.get());

        if agent.system_prompt.is_empty() {
            if let Some(prompt) = defaults.system_prompt {
                agent.system_prompt = prompt;
            }
        }
        agent.thinking_budget = agent.thinking_budget.or(defaults.thinking_budget);
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
        // defaults only fill in prompt/temperature/max_tokens/thinking_budget/
        // mode/model, none of which `resolve_provider_trust` reads, and refusing
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
