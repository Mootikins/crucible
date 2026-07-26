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
            // Cached so per-turn reads never queue behind the loader
            // mutex while a session-start hook builds a container.
            self.agent_manager
                .set_plugin_tool_registry(loader.plugin_registry());

            let tools_api: Arc<dyn crucible_lua::DaemonToolsApi> = Arc::new(
                crate::tools_bridge::DaemonToolsBridge::new(Arc::clone(&self.workspace_tools)),
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

            // User init.lua runs AFTER plugins so its setup() calls override
            // the TOML each plugin was loaded with — Lua beats TOML.
            if let Some(config_dir) = dirs::config_dir() {
                loader
                    .eval_user_init(&config_dir.join("crucible").join("init.lua"))
                    .await;
            }

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
            if let Some(config_dir) = dirs::config_dir() {
                let stubs_dir = config_dir.join("crucible").join("luals");
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
