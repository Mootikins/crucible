//! Daemon-side Lua plugin loading
//!
//! Provides a [`DaemonPluginLoader`] that creates its own `LuaExecutor`,
//! registers daemon-appropriate modules (networking, filesystem, shell,
//! JSON query, paths) and discovers/loads plugins.
//!
//! The daemon is headless, but `cru.oil` (node constructors, via
//! `LuaExecutor::new()`) and the statusline expression registry
//! ([`DaemonPluginLoader::register_statusline_exprs`]) are still registered:
//! plugins *build* UI descriptions daemon-side and clients render them.

pub mod bootstrap;
pub mod option_store;

pub use bootstrap::{
    bootstrap_plugin_entry, bootstrap_plugins, daemon_plugin_paths, default_daemon_plugin_paths,
    BootstrapOutcome,
};
#[cfg(test)]
pub(crate) use bootstrap::{normalize_git_url, plugin_name_from_url, runtime_plugin_paths};

use crate::plugin_tools::PluginRegistry;
use crucible_core::storage::NoteStore;
use crucible_core::storage::PropertyStore;
use crucible_lua::{
    register_context_attach, register_context_module, register_context_validators,
    register_crucible_on_api, register_graph_module, register_isolation_module, register_oq_module,
    register_paths_module, register_publish_module, register_schedule_module,
    register_sessions_module, register_sessions_module_with_api, register_shell_module,
    register_status_module, register_storage_module, register_storage_module_with_store,
    register_tools_module, register_tools_module_with_api, register_ui_module,
    register_ui_module_with_api, register_vault_module, register_ws_module, ContextAttachRegistry,
    DaemonSessionApi, DaemonToolsApi, IsolationRegistry, LuaExecutor, LuaScriptHandlerRegistry,
    LuaValidatorRegistry, OptionsRegistry, PathsContext, PluginManager, PluginSource, PluginSpec,
    PublicationRegistry, ShellPolicy, StatusRegistry,
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

/// A service function extracted from a plugin, tagged with its owner.
///
/// The owner tag is what lets the spawn site record the task's `JoinHandle`
/// against the plugin — a bare `(service_name, Function)` pair left nothing
/// to abort on reload, which is how the discord gateway got duplicated.
pub struct PluginServiceFn {
    pub plugin: String,
    pub service: String,
    pub func: mlua::Function,
}

/// Pull the service/tool/command `mlua::Function` handles out of a plugin's
/// returned spec table.
fn extract_exports(spec: &mlua::Table) -> PluginExports {
    let mut exports = PluginExports::default();
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
    exports
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
    /// Service functions extracted from plugins during loading, drained by
    /// the spawn site via [`Self::take_service_fns`].
    service_fns: Vec<PluginServiceFn>,
    /// Live service tasks by owning plugin, recorded by the spawn site so
    /// reload/disable/remove can abort them ([`Self::abort_services`]).
    service_tasks: HashMap<String, Vec<tokio::task::JoinHandle<()>>>,
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
    /// `cru.oil` comes with `LuaExecutor::new()`, and statusline expressions
    /// are bound later via [`Self::register_statusline_exprs`] — both build
    /// UI *descriptions* that clients render; nothing here draws.
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
        reg("ui", register_ui_module(lua))?;
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
            service_tasks: HashMap::new(),
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
    /// Also sets `cru.kiln.active` to the kiln's registry NAME.
    ///
    /// `kiln_path` is the storage authority — it never reaches Lua. The global
    /// was `cru.kiln.active_path`, the kiln's absolute directory, handed to
    /// every loaded plugin for a kiln it had not named: the same disclosure
    /// `precognition_*` payloads and `cru.config` were closed against, through
    /// a side door. An unregistered kiln sets nothing and *clears* whatever the
    /// previous open left behind — a stale name is worse than no name, because
    /// a handler asking `cru.kiln.active` would be told about a kiln it is no
    /// longer looking at.
    pub fn upgrade_with_storage(
        &self,
        store: Arc<dyn NoteStore>,
        kiln_path: &std::path::Path,
        kiln_name: Option<&crucible_core::config::KilnName>,
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

        // Set cru.kiln.active so plugins know which kiln is active, by name.
        let globals = lua.globals();
        if let Ok(cru) = globals.get::<mlua::Table>("cru") {
            if let Ok(kiln) = cru.get::<mlua::Table>("kiln") {
                // `mlua::Nil` rather than `""`: an empty string is truthy in
                // Lua, so `if cru.kiln.active then` would answer yes for a kiln
                // nothing can name.
                match kiln_name {
                    Some(name) => {
                        let _ = kiln.set("active", name.as_str());
                    }
                    None => {
                        let _ = kiln.set("active", mlua::Nil);
                    }
                }
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
        register_ui_module_with_api(lua, Arc::clone(&api))
            .map_err(|e| anyhow::anyhow!("ui upgrade: {e}"))?;
        register_context_module(lua, api).map_err(|e| anyhow::anyhow!("context module: {e}"))?;
        info!("Lua sessions + ui + context modules upgraded with daemon API");
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

        // The kill switch, applied between discovery and load. A bundled
        // plugin's `plugin.yaml` ships inside the binary and is re-stamped
        // whenever `version + blake3(runtime tree)` changes, so editing
        // `enabled:` there does not survive an upgrade — `[plugins.<name>]
        // enabled = false` in config.toml is the only durable lever.
        // `disable` unloads first, and `unload` returns early for anything not
        // Active, so running it before `load_all` is just a state flip.
        for name in &discovered {
            let disabled = self
                .plugin_config
                .get(name)
                .and_then(|c| c.get("enabled"))
                .and_then(|v| v.as_bool())
                == Some(false);
            if disabled {
                if let Err(e) = self.plugin_manager.disable(name) {
                    warn!("Failed to disable plugin '{name}' from config: {e}");
                } else {
                    info!("Plugin '{name}' disabled by config");
                }
            }
        }

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
                    // Per-plugin fail-open at boot is load-bearing: one broken
                    // plugin must not take the daemon down. `load_plugin_spec`
                    // already marked it Error and made it inert.
                    warn!("Failed to extract spec for plugin '{}': {}", name, e);
                }
            }
        }

        self.remember_specs(&specs);
        Ok(specs)
    }

    /// Upsert by name. Assignment (`self.loaded_specs = specs`) here would drop
    /// every previously loaded plugin's entry the moment `load_plugins` runs a
    /// second time — which it does once `plugin.install` loads at runtime: an
    /// Active plugin is skipped by `load_all` (`AlreadyLoaded`), so its spec is
    /// absent from any later call's result. A spec with `name: None` never
    /// matches an existing entry; it is pushed.
    fn remember_specs(&mut self, specs: &[PluginSpec]) {
        for spec in specs {
            match self
                .loaded_specs
                .iter_mut()
                .find(|s| s.name.is_some() && s.name == spec.name)
            {
                Some(existing) => *existing = spec.clone(),
                None => self.loaded_specs.push(spec.clone()),
            }
        }
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
    /// The functions hold internal refs to the Lua VM and can be spawned as
    /// independent async tasks via `func.call_async::<()>(())`. Spawners must
    /// hand the resulting handle back via [`Self::record_service_task`] —
    /// see `server::plugins::spawn_plugin_services`, the one blessed spawn
    /// path.
    pub fn take_service_fns(&mut self) -> Vec<PluginServiceFn> {
        std::mem::take(&mut self.service_fns)
    }

    /// Record a spawned service task against its owning plugin so
    /// reload/disable/remove can abort it.
    pub fn record_service_task(&mut self, plugin: &str, handle: tokio::task::JoinHandle<()>) {
        let handles = self.service_tasks.entry(plugin.to_string()).or_default();
        // Opportunistic reap: services that ran to completion (or already
        // died) would otherwise accumulate one finished handle per reload.
        handles.retain(|h| !h.is_finished());
        handles.push(handle);
    }

    /// Abort every recorded service task belonging to `plugin`.
    ///
    /// `JoinHandle::abort` cancels at the task's next await point — the
    /// documented author contract: services are cancel-safe and get no stop
    /// callback.
    fn abort_services(&mut self, plugin: &str) {
        if let Some(handles) = self.service_tasks.remove(plugin) {
            for handle in handles {
                handle.abort();
            }
        }
    }

    /// Load one plugin's spec and execute it in the daemon VM.
    ///
    /// On ANY failure the plugin ends up marked `Error` and fully inert:
    /// "not Active" must imply "nothing of this plugin's is registered or
    /// running". Execute failures used to `mark_error` and still return
    /// `Ok(spec)`, so `reload_plugin` reported success while the previous
    /// generation's `crucible.on` handlers stayed live — and `pre_tool_call`
    /// fails closed, so one stale handler could deny every tool call in every
    /// session.
    async fn load_plugin_spec(&mut self, name: &str) -> anyhow::Result<PluginSpec> {
        let result = self.load_plugin_spec_inner(name).await;
        if let Err(e) = &result {
            // Adjacent, with no await between them: the loader mutex is what
            // makes the Error-but-still-registered window unobservable.
            self.make_plugin_inert(name);
            self.plugin_manager.mark_error(name, e.to_string());
        }
        result
    }

    async fn load_plugin_spec_inner(&mut self, name: &str) -> anyhow::Result<PluginSpec> {
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
                    self.service_fns.push(PluginServiceFn {
                        plugin: name.to_string(),
                        service: svc_name,
                        func,
                    });
                }
                self.plugin_registry.register_plugin(
                    name,
                    self.executor.lua(),
                    &spec.tools,
                    &spec.commands,
                    exports.tools,
                    exports.commands,
                );
                Ok(spec)
            }
            Err(e) => {
                warn!(
                    "Failed to execute plugin '{}' in daemon runtime: {}",
                    name, e
                );
                // The spec still describes the plugin — record it so
                // `plugin.list` shows what a broken plugin DECLARES alongside
                // `state: Error`. (Losing this is what let a dead reference
                // plugin look healthy for months.)
                self.remember_specs(std::slice::from_ref(&spec));
                Err(e)
            }
        }
    }

    /// Remove every registration attributed to `name`. "Not Active" must imply
    /// "nothing of this plugin's is registered or running." Keep this
    /// synchronous and call it adjacent to `mark_error` with no await between
    /// them — the loader mutex is what makes the Active-but-inert window
    /// unobservable.
    ///
    /// `IsolationRegistry` and `StatusRegistry` are session-keyed, not
    /// plugin-keyed, so no plugin-scoped release exists for them or is needed.
    fn make_plugin_inert(&mut self, name: &str) {
        self.abort_services(name);
        self.plugin_registry.remove_plugin(name);
        self.handler_registry.clear_plugin_handlers(name);
        if let Err(e) = crucible_lua::clear_plugin_hooks(self.executor.lua(), name) {
            warn!("clear session hooks for dead plugin '{name}': {e}");
        }
        self.publications.release_plugin(name);
        self.options.release_plugin(name);
        // Dropped RegistryKeys only mark their slots; reclaim them so repeated
        // failed reloads don't grow the Lua registry.
        self.executor.lua().expire_registry_values();
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

        // Read (and for `.fnl`, compile) the source BEFORE the loading marker
        // is set, so a read/compile failure cannot leave the marker behind.
        //
        // A `.fnl` main is compiled first. The discovery pass in `crucible-lua`
        // already did this for its throwaway sandbox VM, so a Fennel plugin
        // looked healthy right up to here and then died loading its own source
        // as Lua ("syntax error near '-'", from the `;;;` header comment).
        // No Fennel plugin had ever executed in the daemon.
        let source = std::fs::read_to_string(init_path)
            .map_err(|e| anyhow::anyhow!("read {}: {e}", init_path.display()))?;
        let source = if init_path.extension().is_some_and(|ext| ext == "fnl") {
            self.executor
                .compile_fennel_source(&source)
                .map_err(|e| anyhow::anyhow!("fennel compile {}: {e}", init_path.display()))?
        } else {
            source
        };

        // Drop this plugin's previously-registered handlers and session hooks,
        // and mark it as the loading plugin so anything it registers now is
        // attributed to it. Without both halves a reload appends a second copy
        // of every `crucible.on` handler and every session hook — stale
        // handlers keep firing against dead state (and `pre_tool_call` fails
        // closed, denying every tool call in every session), while a doubled
        // `on_session_start` runs oci's container setup twice per session.
        self.handler_registry.clear_plugin_handlers(name);
        crucible_lua::clear_plugin_hooks(lua, name)
            .map_err(|e| anyhow::anyhow!("clear session hooks for '{name}': {e}"))?;
        lua.globals().set("__crucible_loading_plugin__", name)?;

        // Execute init.lua with eval_async — captures return value AND enables
        // async Lua. Results are captured, not `?`-ed: the marker clear below
        // must run on every exit path.
        let eval_result: mlua::Result<mlua::Value> = lua
            .load(&source)
            .set_name(init_path.to_string_lossy().as_ref())
            .eval_async()
            .await;

        // Extract the callables from the returned spec table. The sandbox pass
        // in `load_plugin_spec` sees the same table but in a throwaway VM, so
        // its functions are useless — only these handles can be invoked.
        let executed: anyhow::Result<PluginExports> = match eval_result {
            Err(e) => Err(anyhow::anyhow!("exec {}: {e}", init_path.display())),
            Ok(mlua::Value::Table(spec)) => match self.call_plugin_setup(name, &spec).await {
                Err(e) => Err(e),
                Ok(()) => Ok(extract_exports(&spec)),
            },
            Ok(_) => Ok(PluginExports::default()),
        };

        // The marker is cleared UNCONDITIONALLY, after setup: setup-registered
        // handlers belong to the plugin — they must be cleared on its reload —
        // and a marker that survives a raise misattributes whatever loads next,
        // up to and including the user's init.lua (which runs after all
        // plugins; reloading the dead plugin would then delete the user's
        // handlers). Anything registered after this point (e.g. from a
        // lifecycle hook at session start) is not attributable to a load, so
        // it is left unowned rather than mis-attributed.
        lua.globals()
            .set("__crucible_loading_plugin__", mlua::Value::Nil)?;

        let exports = executed?;
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

        // A failure in either manager step must leave the plugin inert AND
        // marked Error. `unload` succeeds (state Discovered) before `load`
        // evals the file, so bailing bare on a `load` failure — the everyday
        // trigger is saving init.lua with a syntax error while the watcher
        // is on — left the previous generation's tools, handlers, hooks and
        // services fully live while plugin.list said `Discovered` with no
        // error: broken looked exactly like installed-but-not-loaded.
        if let Err(e) = self
            .plugin_manager
            .unload(name)
            .and_then(|()| self.plugin_manager.load(name))
        {
            self.make_plugin_inert(name);
            self.plugin_manager.mark_error(name, e.to_string());
            anyhow::bail!("reload plugin '{name}': {e}");
        }

        // `load` returns Ok for a disabled plugin — skipping one is not an
        // error — so reload must re-check before executing. Otherwise the kill
        // switch only holds at boot: `plugin.reload`, the web UI's reload
        // button, and (with no human in the loop) the file watcher would each
        // re-run a disabled plugin's init.lua and setup(), re-register its
        // tools, and re-spawn its services, while `plugin.list` still reported
        // it Disabled.
        //
        // The realistic path is an operator disabling a misbehaving plugin,
        // then opening its init.lua to investigate and saving the file.
        if self
            .plugin_manager
            .get(name)
            .is_some_and(|p| p.state == crucible_lua::manifest::PluginState::Disabled)
        {
            self.make_plugin_inert(name);
            anyhow::bail!("plugin '{name}' is disabled; enable it before reloading");
        }

        // The old generation's services must die before the new one's are
        // extracted and spawned — without this, reloading discord left two
        // gateway loops both consuming events.
        self.abort_services(name);

        let spec = match self.load_plugin_spec(name).await {
            Ok(spec) => spec,
            Err(e) => {
                // A failed reload must leave the plugin fully inert, not
                // half-alive: `register_plugin` only replaces entries on a
                // SUCCESSFUL load, so without this the previous version's
                // tools/commands/handlers stayed registered while state said
                // Error — 'broken' looked exactly like 'working'.
                // `load_plugin_spec` already made it inert; repeating the
                // idempotent call keeps this arm correct on its own.
                self.make_plugin_inert(name);
                return Err(e);
            }
        };
        // Same reclaim on the success path — the old version's handler keys
        // were just dropped by re-registration.
        self.executor.lua().expire_registry_values();

        self.remember_specs(std::slice::from_ref(&spec));

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

    /// Manager key (manifest `name`) for the plugin discovered at `dir`.
    ///
    /// plugins.toml declarations and clone directories go by the URL's last
    /// segment; the plugin manager goes by `plugin.yaml`'s `name`. For a repo
    /// `crucible-discord` whose manifest says `name: discord` the two differ,
    /// and resolving by URL name silently misses the running plugin — the
    /// directory is the one identity both sides share.
    pub fn plugin_name_for_dir(&self, dir: &std::path::Path) -> Option<String> {
        self.plugin_manager
            .list()
            .find(|p| p.dir == dir)
            .map(|p| p.manifest.name.clone())
    }

    /// Fully remove a plugin from the running daemon: deactivate + forget.
    ///
    /// The dependent check lives in `PluginManager::unload` and only fires for
    /// Active plugins (`unload` early-returns Ok for any other state) — with
    /// zero shipped plugins declaring dependencies that is acceptable; this
    /// pins the actual behavior, not an aspirational one. The refusal arrives
    /// as `LifecycleError::LoadError(String)` (no dedicated variant); the
    /// message is forwarded verbatim, and a refusal leaves everything —
    /// registrations included — exactly as it was.
    ///
    /// Reload-failure paths must NOT come through here: their `Error` entry
    /// with `last_error` is the diagnostic surface, and `forget` would erase it.
    pub async fn deactivate_and_forget_plugin(&mut self, name: &str) -> anyhow::Result<()> {
        match self.plugin_manager.unload(name) {
            Ok(()) => {}
            // Declared in plugins.toml but never discovered by this daemon
            // (clone deleted by hand, bootstrap failed at boot): nothing to
            // deactivate is not a refusal. Erroring here made a stale
            // declaration unremovable for as long as the daemon ran — the
            // caller's declared-in-TOML precondition already guards typos.
            Err(crucible_lua::lifecycle::LifecycleError::NotFound(_)) => {}
            Err(e) => return Err(e.into()),
        }
        self.make_plugin_inert(name);
        self.loaded_specs
            .retain(|s| s.name.as_deref() != Some(name));
        // Without forget, the entry stays in the manager map (state
        // Discovered): plugin.list still shows it, and — because discover()
        // skips known names — a reinstall loads nothing and reports success.
        self.plugin_manager.forget(name);
        Ok(())
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

#[cfg(test)]
mod tests;
