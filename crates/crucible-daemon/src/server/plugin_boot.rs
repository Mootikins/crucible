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
            let session_api: Arc<dyn crucible_lua::DaemonSessionApi> = Arc::new(
                crate::session_bridge::DaemonSessionBridge::new(self.rpc_context.clone()),
            );
            if let Err(e) = loader.upgrade_with_sessions(session_api) {
                warn!("Failed to upgrade Lua sessions module: {}", e);
            }
            // `cru.log.notify` on the plugin VM goes to the hub unstamped;
            // the hub reads `opts.workspace` / `opts.kiln` or goes global.
            // One VM, one sink. A handler that wants a notification scoped to
            // its session passes the scope — it has `ctx.session_id`.
            let notifications = self.rpc_context.notifications.clone();
            if let Err(e) = loader.upgrade_with_notify_sink(notifications.sink(None)) {
                warn!("Failed to upgrade the Lua notify sink: {}", e);
            }

            // Same pairing for `cru.on` hooks — without this bind,
            // plugins register handlers into a registry the stream loop
            // never reads.
            self.agent_manager
                .set_plugin_handlers(loader.plugin_handlers(), loader.plugin_lua());
            // `cru.permissions.on_request` from the defaults file and from the
            // user's `init.lua`. Both ran on this VM; the tool gate needs the
            // registry and the VM together.
            self.agent_manager
                .set_daemon_permissions(loader.permission_registry());
            // `cru.context.attach` — the drain is per turn, so a handler that
            // attaches must reach the manager's registry, not a second one.
            if let Err(e) = crucible_lua::register_context_attach(
                &loader.plugin_lua(),
                self.agent_manager.context_attach(),
            ) {
                warn!(error = %e, "failed to register cru.context.attach on the plugin VM");
            }
            // The index pipeline fires `index:blocks` through the same pair.
            self.rpc_context
                .kiln
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
                let event_tx = self.rpc_context.event_tx.clone();
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
            // Cached for the same reason, and read on the same kind of path:
            // titling looks up who publishes `session_title` off the turn
            // loop, and must not queue behind another session's start.
            self.agent_manager.set_publications(loader.publications());

            // Bound to the same isolation registry the tool dispatcher reads.
            // Without it `cru.tools.call` runs workspace tools with no agent
            // and no session, so a sandboxed session's plugins could reach the
            // host beside an agent that could not.
            let tools_api: Arc<dyn crucible_lua::DaemonToolsApi> = Arc::new(
                crate::tools_bridge::DaemonToolsBridge::new(
                    Arc::clone(&self.workspace_tools),
                    self.agent_manager.permission_config(),
                )
                .with_isolation(loader.isolation())
                // The registry `cru.tools.set_active` writes and the agent
                // handle reads. Without this bind the two halves are separate
                // registries, so a plugin's narrowing would reach no session.
                .with_active_tools(
                    self.agent_manager.active_tools(),
                    Arc::clone(self.agent_manager.session_manager()),
                ),
            );
            if let Err(e) = loader.upgrade_with_tools(tools_api) {
                warn!("Failed to upgrade Lua tools module: {}", e);
            }

            // Bootstrap git-hosted plugins before discovery: the union of
            // the DECLARED set (`plugins.declare` in init.lua) and the
            // INSTALLED manifest (`<data_home>/plugins.installed.json`).
            let manifest_path = crate::plugin_ops::installed_manifest_path(&self.data_home);

            // `plugins.toml` is no longer read. Its entries are imported
            // into the manifest once (idempotently); the leftover file is
            // inert and warned about until the user deletes it.
            if let Some(toml_path) = crate::plugin_ops::legacy_plugins_toml_path() {
                for (is_warning, line) in
                    crate::plugin_ops::sweep_legacy_plugins_toml(&toml_path, &manifest_path)
                {
                    if is_warning {
                        warn!("{line}");
                    } else {
                        info!("{line}");
                    }
                }
            }

            let declared = self
                .rpc_context
                .effective_config()
                .as_ref()
                .and_then(|cfg| cfg.get("plugins"))
                .and_then(|v| {
                    serde_json::from_value::<std::collections::BTreeMap<String, serde_json::Value>>(
                        v.clone(),
                    )
                    .ok()
                })
                .map(|map| crucible_core::config::declared_plugins(&map))
                .unwrap_or_default();
            for warning in &declared.1 {
                warn!("{warning}");
            }
            let installed = match crate::plugin_ops::installed_entries(&manifest_path) {
                Ok(entries) => entries,
                Err(e) => {
                    warn!("Failed to read {}: {e}", manifest_path.display());
                    Vec::new()
                }
            };
            let (entries, shadows) =
                crate::daemon_plugins::union_plugin_entries(declared.0, installed);
            for name in &shadows {
                info!(
                    "plugin '{name}': the init.lua declaration supersedes the installed \
                     manifest entry"
                );
            }
            if !entries.is_empty() {
                if let Err(e) = crate::daemon_plugins::bootstrap_plugins(&entries).await {
                    warn!("Plugin bootstrap error: {}", e);
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

            // Register `cru.colorscheme` / `cru.statusline` on the PLUGIN VM.
            // The daemon's boot path already did this before it evaluated the
            // user's init.lua (the evaluation now runs BEFORE plugin loading —
            // the boot inversion); the call is guarded, so repeating it here
            // covers the value-injection binds (tests, embeddings) that never
            // went through the boot evaluation.
            if let Err(e) = crucible_lua::config::register_ui_namespaces(&loader.plugin_lua()) {
                warn!("Failed to register UI config namespaces on the plugin VM: {e}");
            }

            // Settings changed through the options pane beat the config, for
            // the same reason init.lua beats TOML: it is the most recent thing
            // the user actually did. Last in the chain, and replayed through
            // each plugin's own setter — see `option_store`.
            // Read from the daemon's resolved data root, not the global
            // `crucible_home()`, so an injected root is honored (see
            // `RpcContext::data_home`).
            crate::daemon_plugins::option_store::restore(&self.data_home, &loader.options());

            // Give Lua a trigger on the workspace changing. Until now every
            // hookable event was on the agent turn loop, so a handler could not
            // react to files at all — which is why a value like git status had
            // no honest trigger and fell back to polling.
            if let Some((registry, plugin_lua)) = self.agent_manager.plugin_handlers() {
                crate::server::file_event_hooks::spawn_file_event_hooks(
                    self.rpc_context.event_tx.subscribe(),
                    registry,
                    plugin_lua,
                );
            }

            // A re-evaluated config is a style change too — this is what makes
            // editing init.lua and reloading take effect without restarting the
            // TUI.
            crate::server::ui_broadcast::broadcast_style_changed(
                &self.rpc_context.event_tx,
                &self.agent_manager,
                crate::server::ui_broadcast::GLOBAL,
            );

            // Extract service functions and spawn them as independent async tasks.
            // Each mlua::Function holds an internal ref to the Lua VM; mlua's
            // reentrant mutex serializes actual Lua execution, giving cooperative
            // multitasking without external coordination.
            crate::server::plugins::spawn_plugin_services(loader);

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
