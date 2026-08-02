//! Plugin runtime boot: wiring the loader into the daemon, loading the
//! shipped/user plugins, and starting their services, schedules and watcher.
//! Split from `mod.rs`'s `run()` for the 1000-line module budget — this is
//! the one self-contained phase of startup.

use super::*;

impl Server {
    /// Bind the plugin runtime and load every plugin. Runs once at startup,
    /// before the accept loop; holding the loader mutex for the whole phase
    /// is correct here (nothing else can contend yet).
    pub(super) async fn boot_plugins(&self) {
        let mut loader_guard = self.plugin_loader.lock().await;
        if let Some(ref mut loader) = *loader_guard {
            // Upgrade sessions module with real daemon API before loading plugins
            let session_api: Arc<dyn crucible_lua::DaemonSessionApi> =
                Arc::new(crate::session_bridge::DaemonSessionBridge::new(
                    self.session_manager.clone(),
                    self.agent_manager.clone(),
                    self.event_tx.clone(),
                ));
            if let Err(e) = loader.upgrade_with_sessions(session_api) {
                warn!("Failed to upgrade Lua sessions module: {}", e);
            }

            // Hand the validator registry + plugin Lua handle to the
            // agent manager so the stream loop can dispatch
            // `OutputValidation::Lua { name }` against plugin-registered
            // validators. Bind once; `set_lua_validators` is idempotent.
            self.agent_manager
                .set_lua_validators(loader.validator_registry(), loader.plugin_lua());
            // Same pairing for `crucible.on` hooks — without this bind,
            // plugins register handlers into a registry the stream loop
            // never reads.
            self.agent_manager
                .set_plugin_handlers(loader.plugin_handlers(), loader.plugin_lua());
            self.agent_manager.set_isolation(loader.isolation());
            // Registry lives on the AgentManager (created eagerly, so session
            // VMs never race this); the plugin VM just gets the same instance.
            if let Err(e) = loader.register_statusline_exprs(self.agent_manager.statusline_exprs())
            {
                tracing::warn!(error = %e, "failed to register cru.statusline on the plugin VM");
            }
            // Storing a value and telling a client about it are separate
            // events: over a socket, with the TUI idle-blocked on input, a
            // changed value repaints nothing by itself. Without this bind the
            // registry records values that no attached client ever sees —
            // `sl.expr("git")` would only update when something else happened
            // to trigger a repaint.
            {
                let event_tx = self.event_tx.clone();
                let agents = self.agent_manager.clone();
                self.agent_manager
                    .statusline_exprs()
                    .set_change_notifier(std::sync::Arc::new(move |session_id: &str| {
                        crate::server::ui_broadcast::broadcast_exprs_changed(
                            &event_tx, &agents, session_id,
                        );
                    }));
            }
            if let Err(e) = loader.register_context_attach(self.agent_manager.context_attach()) {
                tracing::warn!(error = %e, "failed to register cru.context.attach on the plugin VM");
            }
            // Cached so per-turn reads never queue behind the loader
            // mutex while a session-start hook builds a container.
            self.agent_manager
                .set_plugin_tool_registry(loader.plugin_registry());

            // Bound to the same isolation registry the tool dispatcher reads.
            // Without it `cru.tools.call` runs workspace tools with no agent
            // and no session, so a sandboxed session's plugins could reach the
            // host beside an agent that could not.
            let tools_api: Arc<dyn crucible_lua::DaemonToolsApi> = Arc::new(
                crate::tools_bridge::DaemonToolsBridge::new(
                    Arc::clone(&self.workspace_tools),
                    self.agent_manager.permission_config(),
                )
                .with_isolation(loader.isolation()),
            );
            if let Err(e) = loader.upgrade_with_tools(tools_api) {
                warn!("Failed to upgrade Lua tools module: {}", e);
            }

            // Bootstrap declared plugins from plugins.toml before discovery
            let plugins_toml = dirs::config_dir().map(|d| d.join("crucible").join("plugins.toml"));
            if let Some(ref path) = plugins_toml {
                if path.exists() {
                    match std::fs::read_to_string(path) {
                        Ok(content) => {
                            match toml::from_str::<crucible_core::config::PluginsConfig>(&content) {
                                Ok(config) => {
                                    if let Err(e) =
                                        crate::daemon_plugins::bootstrap_plugins(&config.plugin)
                                            .await
                                    {
                                        warn!("Plugin bootstrap error: {}", e);
                                    }
                                }
                                Err(e) => warn!("Failed to parse {}: {}", path.display(), e),
                            }
                        }
                        Err(e) => warn!("Failed to read {}: {}", path.display(), e),
                    }
                }
            }

            let paths = crate::daemon_plugins::daemon_plugin_paths(&self.runtimepath);
            match loader.load_plugins(&paths).await {
                Ok(specs) => {
                    if !specs.is_empty() {
                        info!("Loaded {} daemon plugin(s)", specs.len());
                    }
                }
                Err(e) => {
                    warn!("Failed to load daemon plugins: {}", e);
                }
            }

            // Register `crucible.colorscheme` / `crucible.statusline` on the PLUGIN VM
            // before user init.lua runs — this is the VM that evaluates it.
            // Without this they are nil there, so `crucible.colorscheme.setup{...}`
            // errors and the user's theme never parses. Registration only; the
            // init.lua evaluation is `eval_user_init` below, and doing both here
            // would evaluate it twice.
            if let Err(e) = crucible_lua::config::register_ui_namespaces(&loader.plugin_lua()) {
                warn!("Failed to register UI config namespaces on the plugin VM: {e}");
            }

            // User init.lua runs AFTER plugins so its setup() calls override
            // the TOML each plugin was loaded with — Lua beats TOML.
            if let Some(config_dir) = dirs::config_dir() {
                loader
                    .eval_user_init(&config_dir.join("crucible").join("init.lua"))
                    .await;
            }

            // ...and settings changed through the options pane beat both, for
            // the same reason init.lua beats TOML: it is the most recent thing
            // the user actually did. Last in the chain, and replayed through
            // each plugin's own setter — see `option_store`.
            crate::daemon_plugins::option_store::restore(
                &crucible_core::config::crucible_home(),
                &loader.options(),
            );

            // Give Lua a trigger on the workspace changing. Until now every
            // hookable event was on the agent turn loop, so a handler could not
            // react to files at all — which is why a value like git status had
            // no honest trigger and fell back to polling.
            if let Some((registry, plugin_lua)) = self.agent_manager.plugin_handlers() {
                crate::server::file_event_hooks::spawn_file_event_hooks(
                    self.event_tx.subscribe(),
                    registry,
                    plugin_lua,
                );
            }

            // A re-evaluated config is a style change too — this is what makes
            // editing init.lua and reloading take effect without restarting the
            // TUI.
            crate::server::ui_broadcast::broadcast_style_changed(
                &self.event_tx,
                &self.agent_manager,
                crate::server::ui_broadcast::GLOBAL,
            );

            // Extract service functions and spawn them as independent async tasks.
            // Each mlua::Function holds an internal ref to the Lua VM; mlua's
            // reentrant mutex serializes actual Lua execution, giving cooperative
            // multitasking without external coordination.
            let service_fns = loader.take_service_fns();
            if !service_fns.is_empty() {
                info!("Spawning {} plugin service(s)", service_fns.len());
            }
            for (name, func) in service_fns {
                info!("Starting service: {}", name);
                tokio::spawn(async move {
                    match func.call_async::<()>(()).await {
                        Ok(()) => info!("Service '{}' completed", name),
                        Err(e) => warn!("Service '{}' failed: {}", name, e),
                    }
                });
            }

            // Auto-generate LuaCATS stubs for IDE support
            if let Some(stubs_dir) = crucible_core::config::lua_stubs_dir() {
                match loader.generate_stubs(&stubs_dir) {
                    Ok(()) => debug!("Generated LuaCATS stubs at {}", stubs_dir.display()),
                    Err(e) => debug!("LuaCATS stub generation skipped: {}", e),
                }
            }

            // Register declarative schedules from config
            for schedule in &self.schedules {
                if !schedule.enabled {
                    continue;
                }
                let secs = match crucible_core::config::parse_duration_string(&schedule.every) {
                    Some(d) if d.as_secs() > 0 => d.as_secs(),
                    Some(_) => {
                        warn!(
                            "Schedule '{}': interval must be positive (got '{}')",
                            schedule.name, schedule.every
                        );
                        continue;
                    }
                    None => {
                        warn!(
                            "Schedule '{}': invalid interval '{}'",
                            schedule.name, schedule.every
                        );
                        continue;
                    }
                };
                let action = schedule
                    .action
                    .strip_prefix("lua:")
                    .unwrap_or(&schedule.action);
                let code = format!(
                    "cru.schedule({{ every = {} }}, function() {} end)",
                    secs, action
                );
                match loader.eval(&code).await {
                    Ok(_) => {
                        info!(
                            "Registered schedule '{}' (every {})",
                            schedule.name, schedule.every
                        );
                    }
                    Err(e) => {
                        warn!("Failed to register schedule '{}': {}", schedule.name, e);
                    }
                }
            }

            if self.plugin_watch {
                let plugin_dirs = loader.loaded_plugin_dirs();
                if !plugin_dirs.is_empty() {
                    let plugin_loader_clone = self.plugin_loader.clone();
                    spawn_plugin_watcher(plugin_dirs, plugin_loader_clone);
                }
            }
        }
    }
}
