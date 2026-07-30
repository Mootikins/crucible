//! The per-session Lua VM, and the session config it seeds.
//!
//! Split from `agent_manager/mod.rs` for the 1500-line file budget, along a
//! real seam rather than an arbitrary one: everything here is about standing
//! up a session's Lua state and reading what that Lua decided.
//!
//! Load order matters and is the whole extensibility mechanism — no file gets
//! a privileged API, only an earlier turn to speak:
//!
//! 1. built-in defaults (`BUILTIN_INIT_LUA`)
//! 2. `~/.config/crucible/init.lua` and the workspace's `.crucible/lua/init.lua`
//!
//! Later files override earlier ones with ordinary assignment, which is why
//! `cru.defaults.x = …` needs no override mechanism of its own.

use super::*;

impl AgentManager {
    pub(in crate::agent_manager) fn get_or_create_session_state(
        &self,
        session_id: &str,
    ) -> Arc<Mutex<SessionEventState>> {
        if let Some(state) = self.session_states.get(session_id) {
            return state.clone();
        }

        let lua = Lua::new();
        let registry = LuaScriptHandlerRegistry::new();
        let permission_hooks = Arc::new(StdMutex::new(Vec::new()));
        let permission_functions = Arc::new(StdMutex::new(HashMap::new()));

        if let Err(e) = register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        ) {
            error!(session_id = %session_id, error = %e, "Failed to register crucible.on API");
        }

        if let Err(e) = register_permission_hook_api(
            &lua,
            permission_hooks.clone(),
            permission_functions.clone(),
        ) {
            error!(session_id = %session_id, error = %e, "Failed to register crucible.permissions API");
        }

        // Every session VM writes to the SAME store, so a default set by any
        // file on any VM is what `configure_agent` reads.
        if let Err(e) = crucible_lua::register_session_defaults(&lua, self.session_defaults.clone())
        {
            error!(session_id = %session_id, error = %e, "Failed to register crucible.defaults API");
        }

        // Session handlers get the same attachment surface plugin handlers
        // have; a handler shouldn't behave differently depending on which VM
        // it was registered in. Unconditional — the registry exists from
        // `AgentManager::new`, so there is no ordering to get wrong.
        if let Err(e) = crucible_lua::register_context_attach(&lua, self.context_attach()) {
            error!(session_id = %session_id, error = %e, "Failed to register cru.context.attach");
        }

        if let Ok(cru) = lua.globals().get::<mlua::Table>("cru") {
            if let Err(e) =
                crucible_lua::register_statusline_exprs(&lua, &cru, self.statusline_exprs())
            {
                error!(session_id = %session_id, error = %e, "Failed to register cru.statusline");
            }
        }

        if let Err(e) = lua.load(crucible_lua::BUILTIN_INIT_LUA).exec() {
            warn!(session_id = %session_id, error = %e, "Failed to load built-in init.lua (fail-open)");
        }

        let mut reactor = Reactor::new();
        if let Some(session) = self.session_manager.get_session(session_id) {
            let user_init = session.workspace.join(".crucible/lua/init.lua");
            if user_init.exists() {
                match std::fs::read_to_string(&user_init) {
                    Ok(source) => {
                        if let Err(e) = lua.load(&source).set_name("user init.lua").exec() {
                            warn!(
                                session_id = %session_id,
                                path = %user_init.display(),
                                error = %e,
                                "Failed to load user init.lua (fail-open)"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            session_id = %session_id,
                            path = %user_init.display(),
                            error = %e,
                            "Failed to read user init.lua (fail-open)"
                        );
                    }
                }
            }

            discover_and_register_lua_handlers(&mut reactor, &session.kiln, session_id);
        }

        let state = Arc::new(Mutex::new(SessionEventState {
            lua,
            registry,
            permission_hooks,
            permission_functions,
            reactor,
            spill_counter: std::sync::atomic::AtomicU32::new(1),
        }));
        self.session_states
            .insert(session_id.to_string(), state.clone());
        state
    }

    fn apply_session_defaults(&self, session_id: &str, mut agent: SessionAgent) -> SessionAgent {
        let _vm = self.get_or_create_session_state(session_id);
        let defaults = self.session_defaults.get();

        if agent.system_prompt.is_empty() {
            if let Some(prompt) = defaults.system_prompt {
                agent.system_prompt = prompt;
            }
        }
        agent.temperature = agent.temperature.or(defaults.temperature);
        agent.max_tokens = agent.max_tokens.or(defaults.max_tokens);
        agent.thinking_budget = agent.thinking_budget.or(defaults.thinking_budget);
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

        let agent = self.apply_session_defaults(session_id, agent);
        session.agent = Some(agent.clone());

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
