//! Daemon-side Lua plugin loading
//!
//! Provides a [`DaemonPluginLoader`] that creates its own `LuaExecutor`,
//! registers daemon-appropriate modules (networking, filesystem, shell,
//! JSON query, paths) and discovers/loads plugins.
//!
//! UI modules (oil, popup, panel, statusline) are intentionally excluded —
//! the daemon is headless.

pub mod option_store;

use crate::plugin_tools::PluginRegistry;
use anyhow::Context;
use crucible_core::storage::NoteStore;
use crucible_core::storage::PropertyStore;
use crucible_lua::{
    register_context_attach, register_context_module, register_context_validators,
    register_crucible_on_api, register_graph_module, register_isolation_module, register_oq_module,
    register_paths_module, register_publish_module, register_schedule_module,
    register_sessions_module, register_sessions_module_with_api, register_shell_module,
    register_status_module, register_storage_module, register_storage_module_with_store,
    register_tools_module, register_tools_module_with_api, register_vault_module,
    register_ws_module, ContextAttachRegistry, DaemonSessionApi, DaemonToolsApi, IsolationRegistry,
    LuaExecutor, LuaScriptHandlerRegistry, LuaValidatorRegistry, OptionsRegistry, PathsContext,
    PluginManager, PluginSource, PluginSpec, PublicationRegistry, ShellPolicy, StatusRegistry,
};
use mlua::LuaSerdeExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Callables extracted from a plugin's returned spec table, live in the
/// daemon's Lua VM.
#[derive(Default)]
struct PluginExports {
    services: Vec<(String, mlua::Function)>,
    tools: HashMap<String, mlua::Function>,
    commands: HashMap<String, mlua::Function>,
}

/// Split the raw `[plugins]` TOML table into per-plugin sections and the
/// `watch` knob.
///
/// `[plugins] watch = true` shares the table with `[plugins.<name>]`
/// sections, so scalar entries are knobs, not plugin configs — without the
/// split, `watch = true` would be handed to a phantom plugin named "watch"
/// and the file watcher stayed hardcoded off (`plugin_watch: false` at every
/// construction site, with no config key at all).
pub fn split_plugins_config(
    raw: &HashMap<String, serde_json::Value>,
) -> (HashMap<String, serde_json::Value>, bool) {
    let watch = raw.get("watch").and_then(|v| v.as_bool()).unwrap_or(false);
    let sections = raw
        .iter()
        .filter(|(_, v)| v.is_object())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    (sections, watch)
}

/// Daemon-side plugin loader with its own Lua runtime.
///
/// The daemon gets a separate `LuaExecutor` — it does **not** share
/// Lua state with any CLI instance.
pub struct DaemonPluginLoader {
    executor: LuaExecutor,
    plugin_manager: PluginManager,
    loaded_specs: Vec<PluginSpec>,
    /// Service functions extracted from plugins during loading.
    /// Each entry is `(service_name, mlua::Function)`.
    service_fns: Vec<(String, mlua::Function)>,
    /// Shared registry of Lua-defined output validators.
    ///
    /// Plugins call `cru.context.register_validator(name, fn)` which inserts
    /// a `RegistryKey` into this map; the agent stream loop dispatches
    /// validations by name without re-entering Lua's globals table.
    validator_registry: Arc<LuaValidatorRegistry>,
    /// Handlers registered by plugins via `crucible.on(event, opts, fn)`.
    ///
    /// Paired with [`Self::plugin_lua`] the same way `validator_registry` is:
    /// the handler bodies are `RegistryKey`s into *this* loader's Lua state,
    /// so dispatching them requires both halves. Plugin hooks live here rather
    /// than in the per-session registry because plugins are loaded once, at
    /// daemon start, into a VM no session owns.
    handler_registry: Arc<LuaScriptHandlerRegistry>,
    /// `[plugins.*]` sections from config.toml, keyed by plugin name.
    ///
    /// Also exposed to Lua as `crucible.config.get("<plugin>.<key>")`; kept
    /// here so each plugin's section can be handed to its `setup()` at load.
    plugin_config: HashMap<String, serde_json::Value>,
    /// Spec-declared tools and commands paired with their live `mlua::Function`
    /// handles. Shared with the agent's tool dispatcher and the plugin RPCs.
    plugin_registry: Arc<PluginRegistry>,
    /// Sessions a plugin has claimed isolation for. Read by the tool-call
    /// dispatcher to refuse unhandled host-touching tools.
    isolation: IsolationRegistry,
    /// Per-session status slots published by plugins, read by TUI and web.
    status: StatusRegistry,
    /// Data plugins published about themselves, read by TUI and web.
    ///
    /// The generic contribution channel. Without it a client wanting to know
    /// what a plugin offers had to read the plugin's own config section and
    /// match on its shape — which put one plugin's config schema in the
    /// rendering layer and left a second plugin answering the same question
    /// invisible.
    publications: PublicationRegistry,
    /// Settings trees plugins declared, read by TUI and web.
    options: OptionsRegistry,
    /// Data root under which [`option_store`] keeps values changed through the
    /// settings pane — the daemon's *resolved* `data_home`, never the global
    /// `crucible_home()`, so an injected root is honored.
    ///
    /// `None` for a loader nobody bound one to (tests, embeddings): nothing is
    /// persisted or replayed. Falling back to the global would make any test
    /// that reloads a plugin read and write the developer's real `~/.crucible`.
    option_store_dir: Option<PathBuf>,
}

impl DaemonPluginLoader {
    /// Create a new loader, registering daemon-appropriate Lua modules.
    ///
    /// `LuaExecutor::new()` provides: `cru.http`, `cru.fs`, `cru.timer`,
    /// `cru.ratelimit`, and `cru.retry`/`cru.emitter`/`cru.check` (lua stdlib).
    ///
    /// Additional daemon modules registered here:
    /// - `cru.ws` — WebSocket client
    /// - `cru.shell` — Shell execution (with default policy)
    /// - `oq` — JSON/YAML/TOML query
    /// - `paths` — Standard path helpers
    /// - `cru.kiln` / `cru.graph` — Kiln and graph stubs (upgraded with storage later)
    /// - `cru.schedule` — Interval-based scheduled callbacks
    ///
    /// **Not** registered (UI-only):
    /// - `cru.oil`, `cru.popup`, `cru.panel`, `cru.statusline`
    pub fn new(plugin_config: HashMap<String, serde_json::Value>) -> anyhow::Result<Self> {
        let executor = LuaExecutor::new().map_err(|e| anyhow::anyhow!("LuaExecutor init: {e}"))?;

        // LuaExecutor::new() already registers: http, fs, timer, ratelimit, lua_stdlib.
        // Register additional daemon-specific modules here.
        let lua = executor.lua();

        // Helper to convert module registration errors with context
        fn reg(name: &str, result: Result<(), impl std::fmt::Display>) -> anyhow::Result<()> {
            result.map_err(|e| anyhow::anyhow!("{name} module: {e}"))
        }

        reg("ws", register_ws_module(lua))?;
        reg("shell", register_shell_module(lua, ShellPolicy::default()))?;
        reg("oq", register_oq_module(lua))?;
        reg("paths", register_paths_module(lua, PathsContext::new()))?;
        reg("graph", register_graph_module(lua))?;
        reg("vault", register_vault_module(lua))?;
        reg("storage", register_storage_module(lua))?;
        reg("sessions", register_sessions_module(lua))?;
        reg("tools", register_tools_module(lua))?;
        reg("schedule", register_schedule_module(lua))?;
        reg(
            "config",
            Self::register_plugin_config(lua, plugin_config.clone()),
        )?;

        let plugin_manager = PluginManager::new();

        // Validator registry is created up front so plugins can register
        // validators during init — even before `upgrade_with_sessions`
        // wires the daemon-backed `cru.context.*` methods. The same Arc
        // is shared with `AgentManager` so the stream loop can dispatch
        // by name without re-entering Lua's symbol table.
        let validator_registry = Arc::new(LuaValidatorRegistry::new());
        register_context_validators(lua, Arc::clone(&validator_registry))
            .map_err(|e| anyhow::anyhow!("context validators: {e}"))?;

        // `crucible.on` must exist on *this* VM. Registering it only on the
        // per-session and `lua.init_session` runtimes left it nil for plugins,
        // so every hook-registering plugin raised at load and was downgraded
        // to a warning. Covered by
        // `plugin_runtime_exposes_the_documented_api_surface`.
        // `crucible.require_isolation` — a plugin sandboxing the session
        // declares it here so the dispatcher can default-deny anything the
        // plugin did not handle.
        let isolation = IsolationRegistry::new();
        reg(
            "isolation",
            register_isolation_module(
                lua,
                &lua.globals().get::<mlua::Table>("crucible")?,
                isolation.clone(),
            ),
        )?;

        // `crucible.set_status` — a durable, session-scoped UI slot. Without
        // it a plugin could only emit transient notifications, so a session's
        // isolation state was unverifiable from the UI.
        let status = StatusRegistry::new();
        reg(
            "status",
            register_status_module(
                lua,
                &lua.globals().get::<mlua::Table>("crucible")?,
                status.clone(),
            ),
        )?;

        // `crucible.publish` — what a plugin states about itself, for clients
        // to render. Rebound per plugin at execute time so the publishing
        // plugin is recorded by the loader rather than claimed by the caller.
        let publications = PublicationRegistry::new();

        // `crucible.options` — one declaration, rendered by every frontend.
        // Bound per plugin at execute time for the same reason `publish` is.
        let options = OptionsRegistry::new();

        let handler_registry = Arc::new(LuaScriptHandlerRegistry::new());
        reg(
            "crucible.on",
            register_crucible_on_api(
                lua,
                handler_registry.runtime_handlers(),
                handler_registry.handler_functions(),
            ),
        )?;

        Ok(Self {
            executor,
            plugin_manager,
            loaded_specs: Vec::new(),
            service_fns: Vec::new(),
            validator_registry,
            handler_registry,
            plugin_config,
            plugin_registry: Arc::new(PluginRegistry::new()),
            isolation,
            status,
            publications,
            options,
            option_store_dir: None,
        })
    }

    /// Bind the data root persisted plugin options live under.
    ///
    /// Set once, by `Server::bind`, from the daemon's resolved `data_home`.
    pub fn with_option_store(mut self, dir: PathBuf) -> Self {
        self.option_store_dir = Some(dir);
        self
    }

    /// Where persisted plugin options live, for the RPC layer that records
    /// them. `None` when no data root was bound — nothing is persisted.
    pub fn option_store_dir(&self) -> Option<&Path> {
        self.option_store_dir.as_deref()
    }

    /// Handlers registered by plugins via `crucible.on`.
    ///
    /// Hand this to `AgentManager` together with [`Self::plugin_lua`] — the
    /// handler bodies are registry keys into that specific Lua state, so
    /// neither half dispatches without the other.
    pub fn plugin_handlers(&self) -> Arc<LuaScriptHandlerRegistry> {
        Arc::clone(&self.handler_registry)
    }

    /// Isolation claims made by plugins, for the tool-call dispatcher.
    ///
    /// Paired with [`Self::plugin_handlers`]: handlers do the sandboxing, this
    /// says which sessions are *supposed* to be sandboxed so anything the
    /// handlers missed is refused rather than silently run on the host.
    pub fn isolation(&self) -> IsolationRegistry {
        self.isolation.clone()
    }

    /// Per-session status slots published by plugins, for the RPC layer.
    pub fn status(&self) -> StatusRegistry {
        self.status.clone()
    }

    /// What plugins published about themselves, for the RPC layer.
    pub fn publications(&self) -> PublicationRegistry {
        self.publications.clone()
    }

    /// Settings trees plugins declared, for the RPC layer.
    pub fn options(&self) -> OptionsRegistry {
        self.options.clone()
    }

    /// Register `cru.context.attach` on the plugin VM against the daemon's
    /// registry.
    ///
    /// The registry is owned by `AgentManager`, not by this loader: it is a
    /// per-session buffer with no plugin dependency, and having the loader own
    /// it meant session VMs raced plugin boot for a working binding.
    pub fn register_context_attach(
        &self,
        registry: Arc<ContextAttachRegistry>,
    ) -> anyhow::Result<()> {
        register_context_attach(self.executor.lua(), registry)
            .map_err(|e| anyhow::anyhow!("context.attach module: {e}"))
    }

    /// Bind the statusline expression registry onto the plugin VM. Same
    /// ownership rule as `register_context_attach`: the registry is created by
    /// the agent manager, never here.
    pub fn register_statusline_exprs(
        &self,
        registry: Arc<crucible_lua::StatuslineExprRegistry>,
    ) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        let cru: mlua::Table = lua
            .globals()
            .get("cru")
            .map_err(|e| anyhow::anyhow!("cru table: {e}"))?;
        crucible_lua::register_statusline_exprs(lua, &cru, registry)
            .map_err(|e| anyhow::anyhow!("statusline module: {e}"))
    }

    /// Fire `crucible.on_session_start` hooks registered by plugins.
    ///
    /// Syncs first: hooks live in Lua globals until pulled into the executor's
    /// list, and plugins register them at load — long before any session
    /// exists. Without this the plugin runtime's lifecycle hooks never ran at
    /// all, so a plugin that registers its `crucible.on` handlers inside
    /// `on_session_start` (as `oci` does) never registered anything.
    ///
    /// A raising hook propagates — the caller must refuse the session. A plugin
    /// that acquires an isolation boundary here (`oci` and its container) has
    /// no other way to say "do not proceed", and silently continuing would run
    /// the agent's tools on the host. Plugins wanting non-fatal failure catch
    /// it themselves.
    pub async fn fire_session_start(
        &mut self,
        session: &crucible_lua::Session,
    ) -> anyhow::Result<()> {
        self.executor
            .sync_session_start_hooks()
            .map_err(|e| anyhow::anyhow!("sync session_start hooks: {e}"))?;
        self.executor
            .fire_session_start_hooks(session)
            .await
            .map_err(|e| anyhow::anyhow!("fire session_start hooks: {e}"))
    }

    /// Fire `crucible.on_session_end` hooks registered by plugins.
    /// See [`Self::fire_session_start`] for why this syncs first.
    ///
    /// Teardown failures are reported but must not block the session ending —
    /// refusing to end a session leaves the user stuck, which is the opposite
    /// of the start-hook tradeoff.
    pub async fn fire_session_end(
        &mut self,
        session: &crucible_lua::Session,
    ) -> anyhow::Result<()> {
        self.executor
            .sync_session_end_hooks()
            .map_err(|e| anyhow::anyhow!("sync session_end hooks: {e}"))?;
        self.executor
            .fire_session_end_hooks(session)
            .await
            .map_err(|e| anyhow::anyhow!("fire session_end hooks: {e}"))
    }

    /// Tools and commands contributed by loaded plugins.
    ///
    /// Hand this `Arc` to the agent's tool dispatcher (via
    /// [`crate::plugin_tools::PluginToolExecutor`]) and to the plugin RPC
    /// handlers. It updates in place on plugin reload, so holders never go
    /// stale.
    pub fn plugin_registry(&self) -> Arc<PluginRegistry> {
        Arc::clone(&self.plugin_registry)
    }

    /// Shared registry of Lua-defined output validators.
    ///
    /// Hand this `Arc` to `AgentManager::set_lua_validators` together with
    /// [`Self::plugin_lua`] so the agent stream loop can resolve
    /// `OutputValidation::Lua { name }` against plugin-registered functions.
    pub fn validator_registry(&self) -> Arc<LuaValidatorRegistry> {
        Arc::clone(&self.validator_registry)
    }

    /// Clone of the plugin runtime's `Lua` handle.
    ///
    /// `mlua::Lua` is `Send + Sync` with the `send` feature enabled and
    /// is internally reference-counted; the clone is cheap and lets the
    /// agent stream loop call into Lua-registered validators without
    /// going through the plugin loader's outer mutex.
    pub fn plugin_lua(&self) -> Arc<mlua::Lua> {
        Arc::new(self.executor.lua().clone())
    }

    /// Register plugin config as `crucible.config` in the Lua runtime.
    ///
    /// Provides `crucible.config.get("plugin_name.key")` for dotted-key lookup
    /// from `[plugins.*]` sections in config.toml.
    fn register_plugin_config(
        lua: &mlua::Lua,
        config: HashMap<String, serde_json::Value>,
    ) -> Result<(), mlua::Error> {
        let config_table = lua.create_table()?;

        // Store the raw config data as a Lua table
        let data = lua.to_value(&config)?;
        config_table.set("_data", data)?;

        // crucible.config.get("namespace.key") -> value
        let get_fn = lua.create_function(|lua, key: String| {
            let globals = lua.globals();
            let crucible: mlua::Table = globals.get("crucible")?;
            let config: mlua::Table = crucible.get("config")?;
            let data: mlua::Value = config.get("_data")?;

            let mlua::Value::Table(data_table) = data else {
                return Ok(mlua::Value::Nil);
            };

            // Walk every dot segment: "oci.container.image" descends
            // oci → container → image. It used to split on the FIRST dot
            // only, so nested TOML tables were unreachable past one level.
            let mut current: mlua::Value = mlua::Value::Table(data_table);
            for segment in key.split('.') {
                let mlua::Value::Table(table) = current else {
                    return Ok(mlua::Value::Nil);
                };
                current = table.get(segment.to_string())?;
            }
            Ok(current)
        })?;
        config_table.set("get", get_fn)?;

        // Register on the crucible global
        let globals = lua.globals();
        let crucible: mlua::Table = globals.get("crucible")?;
        crucible.set("config", config_table)?;

        Ok(())
    }

    /// Upgrade graph, vault, and storage modules with real store-backed implementations.
    ///
    /// Call after a kiln opens and storage is available. Replaces stub functions
    /// registered in `new()` with implementations that query the store.
    /// Also sets `cru.kiln.active_path` to the kiln directory path.
    pub fn upgrade_with_storage(
        &self,
        store: Arc<dyn NoteStore>,
        kiln_path: &std::path::Path,
    ) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        let authority = crucible_core::storage::Scope::workspace_unchecked(kiln_path);

        crucible_lua::register_graph_module_with_store_scoped(
            lua,
            store.clone(),
            authority.clone(),
        )
        .map_err(|e| anyhow::anyhow!("graph upgrade: {e}"))?;
        crucible_lua::register_vault_module_with_store_scoped(lua, store, authority)
            .map_err(|e| anyhow::anyhow!("vault upgrade: {e}"))?;

        // Set cru.kiln.active_path so plugins know which kiln is active
        let globals = lua.globals();
        if let Ok(cru) = globals.get::<mlua::Table>("cru") {
            if let Ok(kiln) = cru.get::<mlua::Table>("kiln") {
                let _ = kiln.set("active_path", kiln_path.to_string_lossy().to_string());
            }
        }

        info!(
            "Lua graph/vault modules upgraded with storage (kiln: {})",
            kiln_path.display()
        );
        Ok(())
    }

    /// Upgrade the `cru.storage` module with a real PropertyStore backend.
    ///
    /// Call after a kiln opens and storage is available. The namespace for each
    /// plugin is determined dynamically from `cru._current_plugin` at call time.
    pub fn upgrade_with_property_store(&self, store: Arc<dyn PropertyStore>) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        register_storage_module_with_store(lua, store)
            .map_err(|e| anyhow::anyhow!("storage upgrade: {e}"))?;
        info!("Lua storage module upgraded with PropertyStore");
        Ok(())
    }

    /// Upgrade sessions module with real daemon-backed implementations.
    ///
    /// Call after session/agent managers are created. Replaces stub `cru.sessions.*`
    /// functions with implementations that delegate to the provided API. Also
    /// registers `cru.context.*` (Wave 1 plugin closure surface) which shares
    /// the same [`DaemonSessionApi`].
    pub fn upgrade_with_sessions(&self, api: Arc<dyn DaemonSessionApi>) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        register_sessions_module_with_api(lua, Arc::clone(&api))
            .map_err(|e| anyhow::anyhow!("sessions upgrade: {e}"))?;
        register_context_module(lua, api).map_err(|e| anyhow::anyhow!("context module: {e}"))?;
        info!("Lua sessions + context modules upgraded with daemon API");
        Ok(())
    }

    /// Upgrade tools module with real daemon-backed implementations.
    ///
    /// Call after workspace tools are available. Replaces stub `cru.tools.*`
    /// functions with implementations that delegate to the provided API.
    pub fn upgrade_with_tools(&self, api: Arc<dyn DaemonToolsApi>) -> anyhow::Result<()> {
        register_tools_module_with_api(self.executor.lua(), api)
            .map_err(|e| anyhow::anyhow!("tools upgrade: {e}"))?;
        info!("Lua tools module upgraded with daemon API");
        Ok(())
    }

    /// Discover and load plugins from the given search paths.
    ///
    /// Returns the list of [`PluginSpec`]s extracted from successfully loaded
    /// plugins. Service functions are stored internally and can be retrieved
    /// via [`take_service_fns`].
    /// Add plugin search paths to Lua's `package.path` so `require("plugin")`
    /// works globally (from user init.lua, BUILTIN_INIT_LUA, or other plugins).
    ///
    /// Each search path gets two entries:
    /// - `{path}/?.lua` — for single-file plugins
    /// - `{path}/?/init.lua` — for directory plugins (e.g., `require("kiln-expert")` finds `kiln-expert/init.lua`)
    fn configure_runtime_path(
        &self,
        plugin_paths: &[(PathBuf, PluginSource)],
    ) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        let mut entries = Vec::new();

        for (path, _source) in plugin_paths {
            if !path.exists() {
                continue;
            }
            let path_str = path.to_string_lossy().replace('\\', "/");
            entries.push(format!("{path_str}/?.lua"));
            entries.push(format!("{path_str}/?/init.lua"));
        }

        if entries.is_empty() {
            return Ok(());
        }

        let new_paths = entries.join(";");
        // Guarded prepend: reload re-runs this, and unguarded prepends grew
        // package.path by one copy of every entry per reload for the
        // daemon's lifetime.
        let code = format!(
            r#"
for entry in string.gmatch({new_paths:?}, "[^;]+") do
    -- Delimiter-aware membership: a plain substring find lets a longer
    -- pattern suppress insertion of a distinct shorter one.
    if not ((";" .. package.path .. ";"):find(";" .. entry .. ";", 1, true)) then
        package.path = entry .. ";" .. package.path
    end
end
"#
        );
        lua.load(&code)
            .exec()
            .map_err(|e| anyhow::anyhow!("configure runtime path: {e}"))?;

        tracing::debug!("Configured Lua runtime path with {} entries", entries.len());
        Ok(())
    }

    pub async fn load_plugins(
        &mut self,
        plugin_paths: &[(PathBuf, PluginSource)],
    ) -> anyhow::Result<Vec<PluginSpec>> {
        // Set up global runtime path BEFORE discovery so require() works everywhere
        self.configure_runtime_path(plugin_paths)?;

        for (path, source) in plugin_paths {
            self.plugin_manager
                .add_search_path_with_source(path.clone(), *source);
        }

        let discovered = self
            .plugin_manager
            .discover()
            .map_err(|e| anyhow::anyhow!("plugin discover: {e}"))?;

        if discovered.is_empty() {
            info!("No daemon plugins discovered");
            return Ok(Vec::new());
        }

        info!("Discovered {} daemon plugin(s)", discovered.len());

        let loaded = self
            .plugin_manager
            .load_all()
            .map_err(|e| anyhow::anyhow!("plugin load_all: {e}"))?;

        // Second layer on the kill switch. `load_all` already filters these
        // out; re-checking here means a future regression in that filter
        // cannot silently re-enable execution of a plugin the operator has
        // switched off. `enabled: false` is the documented remediation for a
        // misbehaving plugin, so it is worth two cheap checks.
        let loaded: Vec<String> = loaded
            .into_iter()
            .filter(|name| {
                let disabled = self
                    .plugin_manager
                    .get(name)
                    .is_some_and(|p| p.state == crucible_lua::manifest::PluginState::Disabled);
                if disabled {
                    warn!("Refusing to execute disabled plugin '{name}'");
                }
                !disabled
            })
            .collect();

        info!("Loaded {} daemon plugin(s)", loaded.len());

        let mut specs = Vec::new();
        for name in &loaded {
            match self.load_plugin_spec(name).await {
                Ok(spec) => {
                    info!(
                        "Plugin '{}' spec extracted (tools={}, commands={}, handlers={}, services={})",
                        name,
                        spec.tools.len(),
                        spec.commands.len(),
                        spec.handlers.len(),
                        spec.services.len(),
                    );
                    for svc in &spec.services {
                        info!(
                            "  service '{}' (fn={}) — {}",
                            svc.name, svc.service_fn, svc.description
                        );
                    }
                    // Spec-table handlers are parsed for discovery display but
                    // NEVER dispatched — `crucible.on` at load is the working
                    // API. Say so loudly instead of letting the declaration
                    // look registered.
                    if !spec.handlers.is_empty() {
                        warn!(
                            "Plugin '{}' declares {} spec-table handler(s), which are not \
                             dispatched; register them with crucible.on(...) in init.lua instead",
                            name,
                            spec.handlers.len(),
                        );
                    }
                    specs.push(spec);
                }
                Err(e) => {
                    warn!("Failed to extract spec for plugin '{}': {}", name, e);
                    self.plugin_manager.mark_error(name, e.to_string());
                }
            }
        }

        self.loaded_specs = specs.clone();
        Ok(specs)
    }

    /// Plugin directories that failed discovery, as `{path, error}` objects.
    ///
    /// These have no entry in `plugin.list`'s `plugin_info` — they never became
    /// plugins — so they are reported alongside it.
    pub fn discovery_errors(&self) -> Vec<serde_json::Value> {
        self.plugin_manager
            .discovery_errors()
            .iter()
            .map(|e| {
                serde_json::json!({
                    "path": e.path.to_string_lossy(),
                    "error": e.error,
                })
            })
            .collect()
    }

    /// Drain and return all extracted service functions.
    ///
    /// Each entry is `(service_name, mlua::Function)`. The functions hold
    /// internal refs to the Lua VM and can be spawned as independent async
    /// tasks via `func.call_async::<()>(())`.
    pub fn take_service_fns(&mut self) -> Vec<(String, mlua::Function)> {
        std::mem::take(&mut self.service_fns)
    }

    async fn load_plugin_spec(&mut self, name: &str) -> anyhow::Result<PluginSpec> {
        let plugin = self
            .plugin_manager
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' not found after load", name))?;

        let main_path = plugin.main_path();

        // Extract spec from sandbox (for metadata)
        let spec = crucible_lua::load_plugin_spec(&main_path)
            .map_err(|e| anyhow::anyhow!("spec load for '{}': {e}", name))?
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' returned no spec", name))?;

        // Execute the plugin in the daemon's real Lua runtime using eval_async
        // so that async Lua functions (gateway.connect, etc.) can yield.
        // Also extract service/tool/command Function refs from the returned
        // spec table — the sandbox pass above only yields metadata. `name` is
        // needed to hand the plugin its `[plugins.<name>]` section in setup().
        match self.execute_plugin(name, &main_path).await {
            Ok(exports) => {
                for (svc_name, func) in exports.services {
                    debug!(
                        "Extracted service function '{}' from plugin '{}'",
                        svc_name, name
                    );
                    self.service_fns.push((svc_name, func));
                }
                self.plugin_registry.register_plugin(
                    name,
                    self.executor.lua(),
                    &spec.tools,
                    &spec.commands,
                    exports.tools,
                    exports.commands,
                );
            }
            Err(e) => {
                warn!(
                    "Failed to execute plugin '{}' in daemon runtime: {}",
                    name, e
                );
                // The spec still describes the plugin, so keep returning it for
                // display — but nothing was registered from it, so it is not
                // Active. Reporting Active here is what let a dead reference
                // plugin look healthy for months.
                self.plugin_manager.mark_error(name, e.to_string());
            }
        }

        Ok(spec)
    }

    /// Execute a plugin's init.lua in the daemon's Lua executor (async).
    ///
    /// Sets up `package.path` so that `require("gateway")` etc. resolves
    /// to files in the plugin's `lua/` directory, then evaluates the init file
    /// using `eval_async` to enable async Lua function yielding.
    ///
    /// Calls the returned spec's `setup(cfg)` with this plugin's
    /// `[plugins.<name>]` section — the documented configuration mechanism.
    ///
    /// Returns the callables the plugin exported: services, tools and commands.
    async fn execute_plugin(
        &self,
        name: &str,
        init_path: &std::path::Path,
    ) -> anyhow::Result<PluginExports> {
        let lua = self.executor.lua();
        let plugin_dir = init_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("init path has no parent"))?;
        let lua_dir = plugin_dir.join("lua");

        // Add plugin's lua/ dir to package.path so require() works
        let lua_dir_str = lua_dir
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        // Rebind `crucible.publish` to THIS plugin before its body runs.
        //
        // One Lua VM serves every plugin, so a single global binding would
        // attribute whatever it stored to whichever plugin the closure happened
        // to be built for. Taking the name from an argument instead would let a
        // plugin publish under another's name — attribution nothing could
        // trust. The loader knows who it is about to execute; it says so.
        self.publications.release_plugin(name);
        register_publish_module(
            lua,
            &lua.globals().get::<mlua::Table>("crucible")?,
            self.publications.clone(),
            name.to_string(),
        )?;
        self.options.release_plugin(name);
        crucible_lua::register_options_module(
            lua,
            &lua.globals().get::<mlua::Table>("crucible")?,
            self.options.clone(),
            name.to_string(),
        )?;

        let setup_code = format!(
            r#"
local entry = "{}/?.lua"
if not ((";" .. package.path .. ";"):find(";" .. entry .. ";", 1, true)) then
    package.path = entry .. ";" .. package.path
end
"#,
            lua_dir_str
        );
        lua.load(&setup_code)
            .exec()
            .map_err(|e| anyhow::anyhow!("package.path setup: {e}"))?;

        // All plugins share one Lua VM, so `package.loaded` is shared too.
        // Several plugins ship a module named `config`; without this, the
        // second one to load silently gets the first one's module.
        self.clear_plugin_lua_cache(&lua_dir)?;

        // Drop this plugin's previously-registered handlers, and mark it as
        // the loading plugin so anything it registers now is attributed to it.
        // Without both halves a reload appends a second copy of every
        // `crucible.on` handler, and the stale copies keep firing against dead
        // state — which, with `pre_tool_call` failing closed, denies every
        // tool call in every session.
        self.handler_registry.clear_plugin_handlers(name);
        lua.globals().set("__crucible_loading_plugin__", name)?;

        // Execute init.lua with eval_async — captures return value AND enables async Lua
        let source = std::fs::read_to_string(init_path)
            .map_err(|e| anyhow::anyhow!("read {}: {e}", init_path.display()))?;
        let return_val: mlua::Value = lua
            .load(&source)
            .set_name(init_path.to_string_lossy().as_ref())
            .eval_async()
            .await
            .map_err(|e| anyhow::anyhow!("exec {}: {e}", init_path.display()))?;

        // Anything registered after this point (e.g. from a lifecycle hook at
        // session start) is not attributable to a load, so it is left
        // unowned rather than mis-attributed to whichever plugin loaded last.
        lua.globals()
            .set("__crucible_loading_plugin__", mlua::Value::Nil)?;

        // Extract the callables from the returned spec table. The sandbox pass
        // in `load_plugin_spec` sees the same table but in a throwaway VM, so
        // its functions are useless — only these handles can be invoked.
        let mut exports = PluginExports::default();
        if let mlua::Value::Table(spec) = return_val {
            self.call_plugin_setup(name, &spec).await?;

            // Extract service functions from the returned spec table
            if let Ok(svc_table) = spec.get::<mlua::Table>("services") {
                for (name, entry) in svc_table.pairs::<String, mlua::Table>().flatten() {
                    if let Ok(func) = entry.get::<mlua::Function>("fn") {
                        exports.services.push((name, func));
                    }
                }
            }
            for (field, target) in [
                ("tools", &mut exports.tools),
                ("commands", &mut exports.commands),
            ] {
                if let Ok(table) = spec.get::<mlua::Table>(field) {
                    for (name, entry) in table.pairs::<String, mlua::Table>().flatten() {
                        if let Ok(func) = entry.get::<mlua::Function>("fn") {
                            target.insert(name, func);
                        }
                    }
                }
            }
        }

        info!("Executed plugin in daemon runtime: {}", init_path.display());
        Ok(exports)
    }

    /// Hand `[plugins.<name>]` to the plugin's `setup(cfg)`, if it declares one.
    ///
    /// Always passes a table — plugins treat `setup()` as their activation
    /// point, so an absent config section must not mean "never configured".
    async fn call_plugin_setup(&self, name: &str, spec: &mlua::Table) -> anyhow::Result<()> {
        let Ok(setup) = spec.get::<mlua::Function>("setup") else {
            return Ok(());
        };

        let cfg = self
            .plugin_config
            .get(name)
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let cfg = self
            .executor
            .lua()
            .to_value(&cfg)
            .map_err(|e| anyhow::anyhow!("setup config for '{name}': {e}"))?;

        setup
            .call_async::<()>(cfg)
            .await
            .map_err(|e| anyhow::anyhow!("setup() for '{name}': {e}"))?;

        debug!("Called setup() for plugin '{}'", name);
        Ok(())
    }

    /// Evaluate the user's init.lua in the plugin runtime, if one exists.
    ///
    /// Runs AFTER plugins load — that ordering is the configuration
    /// precedence contract: the daemon applies `[plugins.<name>]` TOML as
    /// each plugin's base config via `setup()` at load, and the user's
    /// `require("<plugin>").setup{...}` here lands last and wins. Lua beats
    /// TOML, the Neovim convention (it used to be silently backwards: TOML
    /// was both base and final word, with no user-Lua entry point at all).
    ///
    /// Fail-open: this is user configuration, not a gate — a broken init.lua
    /// is warned about and the daemon runs with TOML-only config.
    pub async fn eval_user_init(&self, path: &std::path::Path) {
        if !path.exists() {
            return;
        }
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to read {}: {e}", path.display());
                return;
            }
        };
        let chunk_name = format!("@{}", path.to_string_lossy());
        match self
            .executor
            .lua()
            .load(&source)
            .set_name(chunk_name)
            .eval_async::<mlua::Value>()
            .await
        {
            Ok(_) => info!("Evaluated user init: {}", path.display()),
            Err(e) => warn!("User init.lua error ({}): {e}", path.display()),
        }
    }

    /// Reload a plugin: unload registrations, re-execute `init.lua`, and
    /// re-extract service functions. `execute_plugin` clears the plugin's
    /// `package.loaded` entries so its `lua/` modules are re-required.
    pub async fn reload_plugin(&mut self, name: &str) -> anyhow::Result<PluginSpec> {
        if self.plugin_manager.get(name).is_none() {
            anyhow::bail!("plugin '{}' not found", name);
        }

        self.plugin_manager
            .unload(name)
            .map_err(|e| anyhow::anyhow!("unload plugin '{}': {e}", name))?;

        self.plugin_manager
            .load(name)
            .map_err(|e| anyhow::anyhow!("reload plugin '{}': {e}", name))?;

        let spec = match self.load_plugin_spec(name).await {
            Ok(spec) => spec,
            Err(e) => {
                // A failed reload must leave the plugin fully inert, not
                // half-alive: `register_plugin` only replaces entries on a
                // SUCCESSFUL load, so without this the previous version's
                // tools/commands/handlers stayed registered while state said
                // Error — 'broken' looked exactly like 'working'.
                self.plugin_registry.remove_plugin(name);
                self.handler_registry.clear_plugin_handlers(name);
                // Dropped RegistryKeys only mark their slots; reclaim them so
                // repeated failed reloads don't grow the Lua registry.
                self.executor.lua().expire_registry_values();
                return Err(e);
            }
        };
        // Same reclaim on the success path — the old version's handler keys
        // were just dropped by re-registration.
        self.executor.lua().expire_registry_values();

        let spec_name = spec.name.clone();
        if let Some(existing) = self.loaded_specs.iter_mut().find(|s| s.name == spec_name) {
            *existing = spec.clone();
        } else {
            self.loaded_specs.push(spec.clone());
        }

        // Re-executing init.lua re-ran `setup(cfg)` against the ORIGINAL TOML,
        // so every value the user changed in the settings pane just reverted.
        // Replayed here, on the loader, because both reload paths — the RPC
        // handler and the file watcher — go through this function.
        if let Some(dir) = &self.option_store_dir {
            option_store::restore_plugin(dir, &self.options, name);
        }

        info!("Reloaded plugin '{}' successfully", name);
        Ok(spec)
    }

    /// Clear `package.loaded` entries for modules whose `.lua` file lives under `lua_dir`.
    fn clear_plugin_lua_cache(&self, lua_dir: &std::path::Path) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        let lua_dir_str = lua_dir
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");

        lua.load(format!(
            r#"
            local dir = "{lua_dir_str}"
            for mod_name, _ in pairs(package.loaded) do
                local path = dir .. "/" .. mod_name:gsub("%.", "/") .. ".lua"
                local f = io.open(path, "r")
                if f then
                    f:close()
                    package.loaded[mod_name] = nil
                end
            end
            "#,
        ))
        .exec()
        .map_err(|e| anyhow::anyhow!("clear lua cache: {e}"))
    }

    pub fn loaded_plugin_names(&self) -> Vec<String> {
        self.loaded_specs
            .iter()
            .filter_map(|s| s.name.clone())
            .collect()
    }

    /// Return plugin info including provenance source for every discovered
    /// plugin — **not** only the healthy ones.
    ///
    /// Includes capability counts (`tools`, `commands`, `handlers`, `services`)
    /// sourced from `loaded_specs`, so UIs can show what each plugin provides
    /// without a second RPC, plus `last_error` for the ones that broke.
    ///
    /// This deliberately does not filter on `Active`: a plugin that failed to
    /// load was previously dropped from the response entirely, making "broken"
    /// indistinguishable from "not installed" for every client.
    pub fn loaded_plugin_info(&self) -> Vec<serde_json::Value> {
        self.plugin_manager
            .list()
            .map(|p| {
                let spec = self
                    .loaded_specs
                    .iter()
                    .find(|s| s.name.as_deref() == Some(p.manifest.name.as_str()));
                serde_json::json!({
                    "name": p.manifest.name,
                    "version": p.manifest.version,
                    "source": p.source.to_string(),
                    "state": p.state.to_string(),
                    "last_error": p.last_error,
                    "dir": p.dir.to_string_lossy(),
                    "tools": spec.map(|s| s.tools.len()).unwrap_or(0),
                    "commands": spec.map(|s| s.commands.len()).unwrap_or(0),
                    "handlers": self
                        .handler_registry
                        .plugin_handler_count(&p.manifest.name),
                    "services": spec.map(|s| s.services.len()).unwrap_or(0),
                })
            })
            .collect()
    }

    /// Return `(plugin_name, plugin_dir)` pairs for all discovered plugins.
    ///
    /// Used by the plugin file watcher to know which directories to monitor
    /// and which plugin name to reload when a file changes. Broken plugins are
    /// included on purpose — a failed plugin's directory is precisely the one
    /// being edited to fix it, and `reload_plugin` handles a non-Active state.
    pub fn loaded_plugin_dirs(&self) -> Vec<(String, PathBuf)> {
        self.plugin_manager
            .list()
            .filter_map(|p| {
                let name = p.manifest.name.clone();
                let dir = p.dir.clone();
                if dir.exists() {
                    Some((name, dir))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Generate LuaCATS type stubs for IDE support.
    ///
    /// Introspects **this loader's own VM** — the one plugins run on. It used
    /// to build a temporary executor instead, which registered a different set
    /// of modules and then fabricated six `cru.*` namespaces that the real VM
    /// does not have, so autocomplete advertised an API that was nil at
    /// runtime. Read-only: `render_stubs` only walks tables.
    pub fn generate_stubs(&self, output_dir: &std::path::Path) -> anyhow::Result<()> {
        crucible_lua::stubs::StubGenerator::generate_from(self.executor.lua(), output_dir)
            .map_err(|e| anyhow::anyhow!("stub generation: {e}"))
    }

    pub fn executor(&self) -> &LuaExecutor {
        &self.executor
    }

    /// Evaluate Lua code in the plugin runtime context.
    ///
    /// If `code` starts with `=`, prepend `return ` (Neovim convention).
    /// Returns the string representation of the result.
    pub async fn eval(&self, code: &str) -> anyhow::Result<String> {
        let code = if let Some(expr) = code.strip_prefix('=') {
            format!("return {expr}")
        } else {
            code.to_string()
        };

        let lua = self.executor.lua();
        let result: mlua::Value = lua
            .load(&code)
            .set_name("=lua.eval")
            .eval_async()
            .await
            .map_err(|e| anyhow::anyhow!("{}", crucible_lua::format_lua_error(None, &e)))?;

        match &result {
            mlua::Value::Nil => Ok("nil".to_string()),
            mlua::Value::Boolean(b) => Ok(b.to_string()),
            mlua::Value::Integer(n) => Ok(n.to_string()),
            mlua::Value::Number(n) => Ok(n.to_string()),
            mlua::Value::String(s) => Ok(s
                .to_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|_| "<invalid utf8>".to_string())),
            mlua::Value::Table(_) => {
                // Use json encoding for tables
                match lua.from_value::<serde_json::Value>(result) {
                    Ok(json) => Ok(serde_json::to_string_pretty(&json)?),
                    Err(_) => Ok("<table>".to_string()),
                }
            }
            other => Ok(format!("<{}>", other.type_name())),
        }
    }
}

/// Build plugin search paths from config `runtimepath` + env vars + defaults.
///
/// If `runtimepath` is non-empty, each entry's `plugins/` subdir is used as a
/// Runtime source. Otherwise falls back to `CRUCIBLE_RUNTIME` env var and
/// exe-relative detection.
///
/// `CRUCIBLE_PLUGIN_PATH` env var always prepends (highest priority).
/// `~/.config/crucible/plugins/` is always included as User source.
///
/// Paths are ordered by priority (highest first) — same-named plugins at
/// higher-priority paths shadow lower-priority ones.
pub fn daemon_plugin_paths(runtimepath: &[std::path::PathBuf]) -> Vec<(PathBuf, PluginSource)> {
    let mut paths = Vec::new();

    // 1. CRUCIBLE_PLUGIN_PATH env var (highest priority, for dev/CI)
    if let Ok(env_paths) = std::env::var("CRUCIBLE_PLUGIN_PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for p in env_paths.split(sep) {
            if !p.is_empty() {
                paths.push((PathBuf::from(p), PluginSource::EnvPath));
            }
        }
    }

    // 2. User plugins (~/.config/crucible/plugins/)
    if let Some(config_dir) = dirs::config_dir() {
        paths.push((
            config_dir.join("crucible").join("plugins"),
            PluginSource::User,
        ));
    }

    // 3. Runtime paths — from config runtimepath or auto-detected
    if !runtimepath.is_empty() {
        for rtp in runtimepath {
            let expanded = expand_tilde(rtp);
            let plugins_dir = expanded.join("plugins");
            if plugins_dir.exists() {
                tracing::debug!("Adding runtimepath plugin dir: {:?}", plugins_dir);
                paths.push((plugins_dir, PluginSource::Runtime));
            }
        }
    } else {
        // Auto-detect: CRUCIBLE_RUNTIME env → exe-relative fallback
        if let Ok(runtime_base) = std::env::var("CRUCIBLE_RUNTIME") {
            let runtime_plugins = PathBuf::from(runtime_base).join("plugins");
            if runtime_plugins.exists() {
                tracing::debug!("Adding runtime plugin path: {:?}", runtime_plugins);
                paths.push((runtime_plugins, PluginSource::Runtime));
            }
        } else {
            // Installed layout, then the dev tree, then the copy extracted from
            // the binary; see `runtime_roots`.
            paths.extend(runtime_plugin_paths(
                &crucible_core::runtime_roots::for_current_exe(),
            ));
        }
    }

    paths
}

/// The `plugins/` directories among `roots`, in the order given.
///
/// Split out so the auto-detect branch is reachable from a test without
/// controlling the running binary's location — the sibling resolver in
/// `skills::discovery` has had `runtime_skill_paths` for the same reason, and
/// this branch had no equivalent, which is why nothing caught that the bundled
/// plugins reached no installed user.
fn runtime_plugin_paths(roots: &[PathBuf]) -> Vec<(PathBuf, PluginSource)> {
    roots
        .iter()
        .map(|root| root.join("plugins"))
        .filter(|dir| dir.exists())
        .inspect(|dir| tracing::debug!("Adding runtime plugin path: {:?}", dir))
        .map(|dir| (dir, PluginSource::Runtime))
        .collect()
}

/// Expand `~` at the start of a path to the user's home directory.
fn expand_tilde(path: &std::path::Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") || s == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.join(&s[2..]);
        }
    }
    path.to_path_buf()
}

/// Return default plugin paths (no config runtimepath).
/// Convenience for callers that don't have access to config.
pub fn default_daemon_plugin_paths() -> Vec<(PathBuf, PluginSource)> {
    daemon_plugin_paths(&[])
}

/// Outcome of attempting to bootstrap a single plugin entry.
#[derive(Debug, Clone)]
pub enum BootstrapOutcome {
    /// Plugin already cloned at the expected destination; no work done.
    AlreadyPresent,
    /// Disabled in config; skipped.
    Disabled,
    /// Successfully cloned (and pinned, if specified).
    Cloned { dest: PathBuf },
}

/// Bootstrap a single plugin entry: clone if missing, check out pin if
/// set. Returns a structured outcome so callers (CLI vs daemon startup)
/// can decide how loudly to react to failures.
///
/// Pin handling: when a pin is set we drop `--depth 1` because a shallow
/// clone often won't contain the target SHA on the tip. Tags and branch
/// names usually work shallow, but SHAs need full history. Trading
/// bandwidth for correctness.
pub async fn bootstrap_plugin_entry(
    entry: &crucible_core::config::PluginEntry,
) -> anyhow::Result<BootstrapOutcome> {
    if !entry.enabled {
        return Ok(BootstrapOutcome::Disabled);
    }

    let plugins_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine config directory"))?
        .join("crucible")
        .join("plugins");

    let name = plugin_name_from_url(&entry.url)
        .ok_or_else(|| anyhow::anyhow!("Plugin URL '{}' has no usable name segment", entry.url))?;
    let dest = plugins_dir.join(&name);
    if dest.exists() {
        return Ok(BootstrapOutcome::AlreadyPresent);
    }

    let url =
        normalize_git_url(&entry.url).with_context(|| format!("rejecting plugin '{}'", name))?;
    info!("Cloning plugin '{}' from {}", name, url);

    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("clone");
    // Shallow clone unless we need to check out a specific SHA later —
    // shallow clones often don't contain the target SHA.
    if entry.pin.is_none() {
        cmd.args(["--depth", "1"]);
    }
    if let Some(ref branch) = entry.branch {
        cmd.args(["--branch", branch]);
    }
    // Defense-in-depth: `--` stops git from parsing any subsequent argv
    // as flags, even if a future caller bypasses normalize_git_url.
    cmd.arg("--").arg(&url).arg(&dest);

    let output = cmd
        .output()
        .await
        .with_context(|| format!("failed to spawn git clone for '{}'", name))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git clone failed for '{}': {}", name, stderr.trim());
    }

    if let Some(ref pin) = entry.pin {
        let checkout = tokio::process::Command::new("git")
            .args(["checkout", pin])
            .current_dir(&dest)
            .output()
            .await
            .with_context(|| format!("failed to spawn git checkout for pin '{}'", pin))?;
        if !checkout.status.success() {
            // Roll back the cloned dir so retries don't get stuck on
            // a half-installed plugin. Warn loudly if rollback itself
            // fails — the user needs to know `dest` is dirty so they
            // can clean it up manually.
            if let Err(rb_err) = tokio::fs::remove_dir_all(&dest).await {
                warn!(
                    plugin = %name,
                    path = %dest.display(),
                    error = %rb_err,
                    "Failed to roll back half-installed plugin after pin checkout failure; \
                     remove the directory manually before retrying"
                );
            }
            let stderr = String::from_utf8_lossy(&checkout.stderr);
            anyhow::bail!(
                "git checkout failed for pin '{}' of plugin '{}' (manually remove {} if it still exists): {}",
                pin,
                name,
                dest.display(),
                stderr.trim()
            );
        }
    }

    Ok(BootstrapOutcome::Cloned { dest })
}

/// Bootstrap declared plugins by git-cloning any that are missing.
///
/// Reads `PluginEntry` declarations (typically from `plugins.toml`).
/// Failures are warned and skipped — the daemon should start even if
/// one plugin can't be fetched. For per-entry error reporting (e.g.
/// `cru install`), use `bootstrap_plugin_entry` directly.
pub async fn bootstrap_plugins(
    entries: &[crucible_core::config::PluginEntry],
) -> anyhow::Result<()> {
    for entry in entries {
        match bootstrap_plugin_entry(entry).await {
            Ok(_) => {}
            Err(e) => {
                warn!("Plugin bootstrap failed: {}", e);
            }
        }
    }
    Ok(())
}

/// Local alias for `crucible_core::config::plugin_name_from_url` so
/// the existing call sites in this file read naturally. The canonical
/// implementation lives in core so CLI, daemon, and any future
/// consumer share the same definition of "safe plugin directory name".
fn plugin_name_from_url(url: &str) -> Option<String> {
    crucible_core::config::plugin_name_from_url(url)
}

/// Normalize and validate a plugin git URL.
///
/// Accepted forms:
/// - `https://...` / `http://...`
/// - `ssh://git@host/repo[.git]`
/// - `git@host:user/repo[.git]`
/// - Bare `user/repo` shorthand (expanded to `https://github.com/user/repo.git`)
///
/// Rejected:
/// - URLs starting with `-` (parsed as a git flag — CVE-2017-1000117 family)
/// - URLs containing `::` (git external transport — RCE vector via `ext::sh ...`)
/// - Other schemes (`file://`, `git://`, custom) — narrows the attack surface to
///   forms with a vetted use case
/// - Shorthand containing anything outside `[A-Za-z0-9._/-]` (defends against
///   shell-quoting hazards if the value ever lands in a non-`exec`-style context)
fn normalize_git_url(url: &str) -> anyhow::Result<String> {
    if url.is_empty() {
        anyhow::bail!("plugin URL is empty");
    }
    if url.starts_with('-') {
        anyhow::bail!(
            "plugin URL '{}' starts with '-' (would be parsed as a git flag)",
            url
        );
    }
    if url.contains("::") {
        anyhow::bail!(
            "plugin URL '{}' contains '::' (git external transport, disallowed)",
            url
        );
    }

    if url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://git@")
        || url.starts_with("git@")
    {
        Ok(url.to_string())
    } else if url.contains("://") {
        anyhow::bail!(
            "plugin URL '{}' uses unsupported scheme (allowed: https, http, ssh://git@, git@host:repo)",
            url
        )
    } else {
        if !url
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
        {
            anyhow::bail!(
                "plugin shorthand '{}' must match [A-Za-z0-9._/-]+ (got '{}')",
                url,
                url
            );
        }
        Ok(format!("https://github.com/{}.git", url))
    }
}

#[cfg(test)]
mod tests;
