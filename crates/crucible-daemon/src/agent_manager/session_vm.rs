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

        let workspace = self
            .session_manager
            .get_session(session_id)
            .map(|s| s.workspace.to_string_lossy().into_owned());
        let mut lua_session = crucible_lua::Session::new(session_id.to_string());
        if let Some(workspace) = workspace {
            lua_session = lua_session.with_workspace(workspace);
        }
        lua_session.bind(Box::new(crucible_lua::SessionDefaultsRpc::new(
            scope.clone(),
        )));

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
            error!(session_id = %session_id, error = %e, "Failed to register cru.defaults API");
        }

        // `on_session_start` / `on_session_end`. Registered only by
        // `LuaExecutor` until now, which is why the API was nil here — the
        // shipped defaults AND `cru setup`'s user template both advertised it
        // while it silently did nothing on this VM.
        for namespace in ["cru", "crucible"] {
            match crucible_lua::lua_util::get_or_create_namespace(&lua, namespace) {
                Ok(table) => {
                    if let Err(e) = crucible_lua::register_hooks_module(&lua, &table) {
                        error!(session_id = %session_id, namespace, error = %e, "Failed to register lifecycle hooks API");
                    }
                }
                Err(e) => {
                    error!(session_id = %session_id, namespace, error = %e, "Failed to create Lua namespace");
                }
            }
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

        // Resolved off the runtimepath, so a copied-out `runtime/defaults/`
        // shadows what shipped. The origin is logged because "I edited the
        // defaults and nothing changed" is otherwise near-undiagnosable — it
        // usually means a higher-priority candidate won.
        let (defaults_src, defaults_origin) =
            crate::runtime_defaults::load_defaults(&self.runtimepath);
        debug!(session_id = %session_id, source = %defaults_origin, "Loading Lua defaults");
        if let Err(e) = lua
            .load(&defaults_src)
            .set_name(defaults_origin.to_string())
            .exec()
        {
            warn!(
                session_id = %session_id,
                source = %defaults_origin,
                error = %e,
                "Failed to load Lua defaults (fail-open)"
            );
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

        // Every file has now run, so the hooks list is complete. Fire it here
        // rather than at session.create: this is the only point where the
        // session's OWN Lua (workspace `.crucible/lua/init.lua`) has been
        // loaded, and a project-local hook that never fires is the bug this
        // whole arc is about.
        //
        // Seeded from the global defaults so `session.x` reads the inherited
        // value — `session.system_prompt = session.system_prompt .. "…"`
        // extends the default instead of clobbering it. The result is this
        // session's starting values; `apply_session_defaults` reads it.
        let scope = crucible_lua::SessionDefaults::new();
        scope.set(self.session_defaults.get());
        self.fire_session_start_hooks(&lua, session_id, &scope);
        self.session_overrides
            .insert(session_id.to_string(), scope.get());

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
        // Creating the VM ran `on_session_start`, which captured this
        // session's values into `session_overrides` — already seeded from the
        // globals, so it is the complete picture. Fall back to the raw globals
        // only if no VM state was recorded (a manager whose VM construction
        // failed outright).
        let defaults = self
            .session_overrides
            .get(session_id)
            .map(|entry| entry.value().clone())
            .unwrap_or_else(|| self.session_defaults.get());

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
