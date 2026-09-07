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
    /// Run `cru.on_session_start` hooks against a defaults scope.
    ///
    /// Sync, like the permission hooks and for the same reason: this runs
    /// inside session-VM construction, which is not async, and a start hook
    /// deciding a session's opening configuration has no business awaiting.
    ///
    /// Fails open per hook — one plugin's broken hook must not stop another's
    /// from running, nor block the session. (The `required = true` escalation
    /// that `LuaExecutor` honours is about isolation boundaries owning session
    /// refusal; that path stays with the plugin loader, which is where
    /// isolation claims live.)
    fn fire_session_start_hooks(
        &self,
        lua: &Lua,
        session_id: &str,
        scope: &crucible_lua::SessionDefaults,
    ) {
        let hooks = match crucible_lua::get_session_start_hooks(lua) {
            Ok(hooks) => hooks,
            Err(e) => {
                warn!(session_id = %session_id, error = %e, "Could not read on_session_start hooks");
                return;
            }
        };
        if hooks.is_empty() {
            return;
        }

        // The same object plugins get, so `session.isolation` reads the same
        // from a user's `cru.on_session_start` as from a plugin's. Two surfaces
        // for one documented field that disagreed would be worse than the field
        // not existing on one of them.
        let daemon_session = self.session_manager.get_session(session_id);
        let variables = self.slot(session_id).variables();
        let mut lua_session = crucible_lua::Session::new(session_id.to_string());
        if let Some(daemon_session) = daemon_session {
            // Only when there IS one: `session.workspace` reads `nil` in
            // Lua for a workspace-less session rather than a wrong path.
            if let Some(workspace) = &daemon_session.workspace {
                lua_session = lua_session.with_workspace(workspace.to_string_lossy().into_owned());
            }
            if let Some(isolation) = daemon_session.isolation {
                lua_session = lua_session.with_isolation(isolation);
            }
            // Seed before the hooks run, so `get_variable` reads what an
            // earlier life of this session stored.
            variables.replace(daemon_session.variables);
        }
        lua_session.bind(Box::new(
            crucible_lua::SessionDefaultsRpc::new(scope.clone()).with_variables(variables),
        ));

        for key in &hooks {
            match lua.registry_value::<mlua::Function>(key) {
                Ok(func) => {
                    if let Err(e) = func.call::<()>(lua_session.clone()) {
                        warn!(
                            session_id = %session_id,
                            error = %e,
                            "on_session_start hook failed (fail-open)"
                        );
                    }
                }
                Err(e) => {
                    warn!(session_id = %session_id, error = %e, "on_session_start hook missing from registry");
                }
            }
        }
        debug!(session_id = %session_id, hooks = hooks.len(), "Fired on_session_start hooks");
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
    fn build_session_state(&self, session_id: &str) -> Arc<Mutex<SessionEventState>> {
        // The hooks live on the daemon VM, which ran every file at boot.
        // Fire them here rather than at session.create, because this is where
        // the per-session scope they write into is created.
        //
        // Seeded from the global defaults so `session.x` reads the inherited
        // value — `session.system_prompt = session.system_prompt .. "…"`
        // extends the default instead of clobbering it. The result is this
        // session's starting values; `apply_session_defaults` reads it.
        let scope = crucible_lua::SessionDefaults::new();
        scope.set(self.session_defaults.get());
        // No daemon VM bound is a test manager: nothing registered hooks.
        if let Some((_, daemon_lua)) = self.plugin_handlers() {
            self.fire_session_start_hooks(&daemon_lua, session_id, &scope);
        }
        self.slot(session_id).set_overrides(scope.get());
        self.schedule_variable_persist(session_id);

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
