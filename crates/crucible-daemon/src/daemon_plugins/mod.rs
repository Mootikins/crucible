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

mod activate;
pub mod boot;
pub mod bootstrap;
pub mod option_store;
pub mod resolve;

pub use boot::{
    boot_input_hash, evaluate_boot_config, evaluate_boot_config_with_paths, BootConfig,
    PluginPathsFn,
};
#[cfg(test)]
pub(crate) use bootstrap::normalize_git_url;
pub use bootstrap::{
    bootstrap_entries, bootstrap_plugin_entry, bootstrap_plugins, daemon_plugin_paths,
    daemon_plugin_paths_from, declared_git_entry, default_daemon_plugin_paths, BootstrapOutcome,
};
#[cfg(test)]
pub(crate) use crucible_core::config::plugin_name_from_url;

use crate::plugin_tools::PluginRegistry;
use crucible_core::storage::NoteStore;
use crucible_core::storage::PropertyStore;
use crucible_lua::{
    register_context_attach, register_context_module, register_cru_on_api,
    register_isolation_module, register_oq_module, register_paths_module, register_schedule_module,
    register_sessions_module, register_shell_module, register_status_module,
    register_storage_module, register_storage_module_with_store, register_surface_module,
    register_tools_module, register_tools_module_with_api, register_ui_module,
    register_ui_module_with_api, register_vault_module, register_ws_module, ContextAttachRegistry,
    DaemonSessionApi, DaemonToolsApi, IsolationRegistry, LuaExecutor, LuaScriptHandlerRegistry,
    OptionsRegistry, PathsContext, PluginManager, PluginShellPolicy, PluginSource, PluginSpec,
    PublicationRegistry, StatusRegistry, SurfaceRegistry,
};
use mlua::LuaSerdeExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

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

/// Split the raw `[plugins]` TOML table into per-plugin sections and the
/// `watch` knob.
///
/// `[plugins] watch = true` shares the table with `[plugins.<name>]`
/// sections, so scalar entries are knobs, not plugin configs — without the
/// split, `watch = true` would be handed to a phantom plugin named "watch"
/// and the file watcher stayed hardcoded off (`plugin_watch: false` at every
/// construction site, with no config key at all).
pub fn split_plugins_config(
    raw: &std::collections::BTreeMap<String, serde_json::Value>,
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
    /// Each active plugin's declarations, by the plugin's name.
    ///
    /// The key is the plugin's identity, the directory name, so a plugin
    /// whose fragment declares another name still finds its own entry.
    loaded_specs: HashMap<String, PluginSpec>,
    /// The daemon-backed session API `upgrade_with_sessions` registered with,
    /// so late-created Lua runtimes (`lua.init_session`) can register the
    /// same module against the same bridge instead of a second instance.
    ///
    /// Boot installs it once and every later caller only reads it, which is
    /// exactly [`crucible_lua::HostHook`]'s shape: the `Mutex<Option<_>>` it
    /// replaces paid a lock per read and let a second upgrade rebind the
    /// bridge in silence.
    session_api: crucible_lua::HostHook<Arc<dyn crucible_lua::DaemonSessionApi>>,
    /// Service functions extracted from plugins during loading, drained by
    /// the spawn site via [`Self::take_service_fns`].
    service_fns: Vec<PluginServiceFn>,
    /// Live service tasks by owning plugin, recorded by the spawn site so
    /// reload/disable/remove can abort them ([`Self::abort_services`]).
    service_tasks: HashMap<String, Vec<tokio::task::JoinHandle<()>>>,
    /// The session-default store the user's `init.lua` writes.
    ///
    /// Shared with `AgentManager`, which registers the same handle into every
    /// session reads, so one write at boot reaches every session.
    /// The mode registry, shared the same way and for the same reason.
    modes: crucible_lua::ModeRegistry,
    /// Shared registry of Lua-defined output validators.
    ///
    /// Plugins call `cru.context.register_validator(name, fn)` which inserts
    /// a `RegistryKey` into this map; the agent stream loop dispatches
    /// Handlers registered by plugins via `cru.on(event, opts, fn)`.
    ///
    /// Paired with [`Self::plugin_lua`]:
    /// the handler bodies are `RegistryKey`s into *this* loader's Lua state,
    /// so dispatching them requires both halves. Plugin hooks live here rather
    /// than in the per-session registry because plugins are loaded once, at
    /// daemon start, into a VM no session owns.
    handler_registry: Arc<LuaScriptHandlerRegistry>,
    /// The `plugins.*` config subtrees, keyed by plugin name.
    ///
    /// Also exposed to Lua as `cru.plugin.config.get("<plugin>.<key>")`; kept
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
    /// Panels plugins declared, read by TUI and web.
    ///
    /// Released by `make_plugin_inert`, like the publications beside it. A
    /// successful reload never reaches that path — it re-declares the same
    /// `(plugin, name)` keys and keeps the rows — so the only callers are the
    /// failure paths and the uninstall, and a panel whose plugin is inert must
    /// close rather than keep drawing rows nothing can refresh.
    surfaces: SurfaceRegistry,
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
    /// The statusline expression values, owned by `AgentManager` and bound onto
    /// this VM by [`Self::register_statusline_exprs`].
    ///
    /// Kept only so `make_plugin_inert` can release the values a plugin set.
    /// A `OnceLock` because boot binds it exactly once and every read is on a
    /// teardown path: a `Mutex<Option<_>>` would pay a lock and permit a silent
    /// rebind onto a second registry, which is how a release would quietly
    /// address the wrong store.
    statusline_exprs: std::sync::OnceLock<Arc<crucible_lua::StatuslineExprRegistry>>,
    /// Each active plugin's module table, by the plugin's name. What makes
    /// `activate` idempotent: a second call answers the stored table. A
    /// plugin made inert loses its entry, so a reload runs the file again.
    active_modules: HashMap<String, mlua::RegistryKey>,
    /// Every discovered plugin, in the order discovery found it. The
    /// spec-driven pass activates in this order, and `discover` reports only
    /// the names it found on that call, so the loader keeps the whole list.
    discovery_order: Vec<String>,
    /// Data root under which [`option_store`] keeps values changed through the
    /// settings pane — the daemon's *resolved* `data_home`, never the global
    /// `crucible_home()`, so an injected root is honored.
    ///
    /// `None` for a loader nobody bound one to (tests, embeddings): nothing is
    /// persisted or replayed. Falling back to the global would make any test
    /// that reloads a plugin read and write the developer's real `~/.crucible`.
    option_store_dir: Option<PathBuf>,
}

/// The registered directory of a kiln NAME, for a Lua resolver.
///
/// The error string reaches Lua as-is, so it names the kiln and never a
/// directory.
fn registered_kiln_path(
    registry: &crate::kiln_registry::KilnRegistry,
    name: &str,
) -> Result<(crucible_core::config::KilnName, PathBuf), String> {
    let kiln_name = crucible_core::config::KilnName::parse(name).map_err(|e| e.to_string())?;
    let path = registry
        .resolve(&kiln_name)
        .path()
        .ok_or_else(|| format!("kiln '{kiln_name}' is not registered"))?;
    Ok((kiln_name, path))
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
    /// - `cru.kiln` — Kiln stub (upgraded with storage later)
    /// - `cru.schedule` — Interval-based scheduled callbacks
    ///
    /// `cru.oil` comes with `LuaExecutor::new()`, and statusline expressions
    /// are bound later via [`Self::register_statusline_exprs`] — both build
    /// UI *descriptions* that clients render; nothing here draws.
    pub fn new(plugin_config: HashMap<String, serde_json::Value>) -> anyhow::Result<Self> {
        let executor = LuaExecutor::new().map_err(|e| anyhow::anyhow!("LuaExecutor init: {e}"))?;

        // LuaExecutor::new() already registers: http, fs, timer, ratelimit, the prelude.
        // Register additional daemon-specific modules here.
        let lua = executor.lua();

        // Helper to convert module registration errors with context
        fn reg(name: &str, result: Result<(), impl std::fmt::Display>) -> anyhow::Result<()> {
            result.map_err(|e| anyhow::anyhow!("{name} module: {e}"))
        }

        reg("ws", register_ws_module(lua))?;
        reg(
            "shell",
            register_shell_module(lua, PluginShellPolicy::default()),
        )?;
        reg("oq", register_oq_module(lua))?;
        reg("paths", register_paths_module(lua, PathsContext::new()))?;
        reg("vault", register_vault_module(lua))?;
        reg("embed", crucible_lua::register_embed_module(lua))?;
        reg("storage", register_storage_module(lua))?;
        reg("session", register_sessions_module(lua))?;
        reg("ui", register_ui_module(lua))?;
        reg("tools", register_tools_module(lua))?;
        reg("schedule", register_schedule_module(lua))?;
        reg(
            "config",
            Self::register_plugin_config(lua, plugin_config.clone()),
        )?;

        // `cru.statusline`, `cru.colorscheme`, `cru.hl`, `cru.geometry` and
        // `cru.syntax`.
        //
        // The daemon evaluates the user's
        // `init.lua` on THIS VM (`daemon_plugins::boot`, step 3), so these
        // belong to the loader's shape, not to a later call on it.
        //
        // They used to be registered only AFTER construction, at three call
        // sites. `generate_stubs` introspects the loader's VM, so it rendered a
        // definitions file without them, and `runtime/defaults/init.lua` —
        // shipped, and evaluated on a VM that HAS them — reported five type
        // errors for API that works.
        //
        // The three later calls REPLACE the colorscheme, hl, geometry and
        // syntax tables rather than merging into them — only
        // `register_statusline_namespace` guards itself. That is harmless
        // because nothing else puts members on those four, which is a property
        // of today's code and not of the design. Anything that starts adding to
        // one of them must guard here first.
        reg(
            "ui namespaces",
            crucible_lua::config::register_ui_namespaces(lua),
        )?;

        // `cru.modes`. The store is the SAME handle the sessions read, so a
        // mode declared in the user's `~/.config/crucible/init.lua` reaches
        // every session with no copy step. `AgentManager` adopts it at bind
        // time.
        //
        // A workspace cannot reach this: no workspace file runs on any VM.
        // This file and the runtimepath's defaults file are the only writers.
        let modes = crucible_lua::ModeRegistry::new();
        reg("modes", crucible_lua::register_modes(lua, modes.clone()))?;

        // One store for every `cru.*` callback on this VM: `cru.on`,
        // `cru.permissions.on_request`, the two session hooks and
        // `cru.on_provider_auth`. Built here so both registration APIs below
        // wire the same handle, and so `clear_source` reaches all of them.
        let handler_registry = Arc::new(LuaScriptHandlerRegistry::new());

        // `cru.permissions`. This VM runs every Lua file, so this is the only
        // registration; the tool gate dispatches these hooks.
        reg(
            "permissions",
            crucible_lua::register_permission_hook_api(lua, (*handler_registry).clone()),
        )?;

        // `cru.context` must exist from init, not only after
        // `upgrade_with_sessions` mounts the daemon-backed methods. The stub
        // carries the pure half (`estimate_tokens`) and answers "no daemon
        // connected" for the rest, and `register_context_module` mounts OVER
        // this table rather than replacing it. Without this the namespace is
        // absent on a loader that never upgrades, which
        // `the_plugin_vm_exposes_exactly_the_declared_namespaces` catches.
        reg("context", crucible_lua::register_context_module_stub(lua))?;

        let plugin_manager = PluginManager::new();

        // `cru.on` must exist on *this* VM. Registering it only on the
        // per-session and `lua.init_session` runtimes left it nil for plugins,
        // so every hook-registering plugin raised at load and was downgraded
        // to a warning. Covered by
        // `plugin_runtime_exposes_the_documented_api_surface`.
        // `cru.isolation.require` — a plugin sandboxing the session
        // declares it here so the dispatcher can default-deny anything the
        // plugin did not handle.
        let isolation = IsolationRegistry::new();
        reg(
            "isolation",
            register_isolation_module(lua, isolation.clone()),
        )?;

        // `cru.plugin.set_status` — a durable, session-scoped UI slot.
        // Without it a plugin could only emit transient notifications, so a
        // session's isolation state was unverifiable from the UI.
        let status = StatusRegistry::new();
        reg("status", register_status_module(lua, status.clone()))?;

        // `cru.plugin.setup` — the spec. The rank of a write comes from the
        // source in force on this VM, so the operator's `init.lua` outranks
        // the shipped defaults and a plugin's own fragment. The boot sets the
        // `import` root when it knows the config root (`boot.rs`).
        reg("plugin spec", crucible_lua::register_plugin_spec_api(lua))?;

        // `cru.surface.declare` — a panel every client draws in its own idiom.
        // Data, never a node tree: the browser cannot afford a cell grid, and a
        // grid cannot express the DOM. See `crucible-lua/src/surfaces.rs`.
        let surfaces = SurfaceRegistry::new();
        reg("surfaces", register_surface_module(lua, surfaces.clone()))?;

        // `cru.plugin.publish` — what a plugin states about itself, for
        // clients to render. Rebound per plugin at execute time so the
        // publishing plugin is recorded by the loader rather than claimed by
        // the caller.
        let publications = PublicationRegistry::new();

        // `cru.plugin.options` — one declaration, rendered by every frontend.
        // Bound per plugin at execute time for the same reason `publish` is.
        let options = OptionsRegistry::new();

        reg(
            "cru.on",
            register_cru_on_api(lua, (*handler_registry).clone()),
        )?;

        Ok(Self {
            executor,
            plugin_manager,
            loaded_specs: HashMap::new(),
            session_api: crucible_lua::HostHook::new(),
            service_fns: Vec::new(),
            service_tasks: HashMap::new(),
            modes,
            handler_registry,
            plugin_config,
            plugin_registry: Arc::new(PluginRegistry::new()),
            isolation,
            status,
            surfaces,
            publications,
            options,
            statusline_exprs: std::sync::OnceLock::new(),
            active_modules: HashMap::new(),
            discovery_order: Vec::new(),
            option_store_dir: None,
        })
    }

    /// Install the `[plugins.<name>]` sections after construction.
    ///
    /// The boot inversion creates the loader BEFORE the config exists — the
    /// VM must be live for `init.lua` to evaluate in it — so the sections
    /// arrive from the FINAL store once the evaluation has finished, and the
    /// `cru.plugin.config` table is re-registered over the empty one the
    /// constructor installed.
    pub fn with_plugin_config(
        mut self,
        plugin_config: HashMap<String, serde_json::Value>,
    ) -> anyhow::Result<Self> {
        Self::register_plugin_config(self.executor.lua(), plugin_config.clone())
            .map_err(|e| anyhow::anyhow!("config module: {e}"))?;
        self.plugin_config = plugin_config;
        Ok(self)
    }

    /// Bind the data root persisted plugin options live under.
    ///
    /// Set once, by `Server::bind_with_plugin_config`, from the daemon's resolved `data_home`.
    pub fn with_option_store(mut self, dir: PathBuf) -> Self {
        self.option_store_dir = Some(dir);
        self
    }

    /// Wire `cru.kiln.path` to the daemon's kiln registry.
    ///
    /// The resolver runs registry lookups inside the daemon; a plugin asks
    /// for a kiln by NAME and receives the resolved root only from this one
    /// API — the same rule `cru.kiln.active` and `LOCATION_CONFIG_KEYS`
    /// hold. (`kiln://` addressing in `cru.fs` is removed; the fs module
    /// refuses the scheme permanently.)
    pub fn with_kiln_path_resolver(
        self,
        registry: Arc<crate::kiln_registry::KilnRegistry>,
    ) -> anyhow::Result<Self> {
        // The same registry answers the two questions a plugin's file access
        // asks: "where is kiln X" and "which directories may I read and write
        // at all". Binding them together is what keeps the second from
        // drifting behind the first.
        self.bind_fs_roots(Arc::clone(&registry));
        let resolver: crucible_lua::KilnPathResolver =
            Arc::new(move |name: &str| registered_kiln_path(&registry, name).map(|(_, path)| path));
        crucible_lua::register_kiln_path_resolver(self.executor.lua(), resolver)
            .map_err(|e| anyhow::anyhow!("cru.kiln.path (kiln resolver): {e}"))?;
        Ok(self)
    }

    /// Bind where a plugin's `cru.fs.read` and `cru.fs.write` may reach.
    ///
    /// Three kinds of root, and each is a place the plugin was already meant
    /// to work in: every registered kiln (its notes are the data plugins
    /// exist to handle), the daemon's plugin-state directory (a plugin's own
    /// files), and the process working directory (the invocation's workspace,
    /// which is what `cru.paths.workspace()` answers with when one is set).
    ///
    /// It does NOT confine `mkdir`, `list`, `copy` or `remove_all`, which
    /// predate it and are used against paths outside all three — `worktree`
    /// checks a destination it is about to create. Narrowing those is a
    /// separate decision with a migration behind it; the two NEW functions
    /// start scoped, which is the direction to move the rest in.
    ///
    /// It is ergonomics, not a boundary: `register_stdlib_compat` installs an
    /// unscoped `io.open` for every plugin, so a plugin that wants to leave
    /// its roots calls that instead. See [[Meta/Analysis/Plugin Merge Plan]].
    fn bind_fs_roots(&self, registry: Arc<crate::kiln_registry::KilnRegistry>) {
        let state_root = crucible_core::config::crucible_home().join("plugin-state");
        let resolver: crucible_lua::FsRootsResolver = Arc::new(move |plugin: &str| {
            let mut roots: Vec<PathBuf> = registry
                .entries()
                .iter()
                .map(|kiln| kiln.path().to_path_buf())
                .collect();
            roots.push(state_root.join(plugin));
            if let Ok(cwd) = std::env::current_dir() {
                roots.push(cwd);
            }
            roots
        });
        crucible_lua::register_fs_roots_resolver(self.executor.lua(), resolver);
    }

    /// Wire the named kiln reads — `cru.kiln.blocks`, `note`, `notes`,
    /// `links` and `search` — to the daemon's open kilns.
    ///
    /// A plugin names a kiln; the registry turns the name into a directory
    /// and the manager opens that directory on first use. The directory
    /// never reaches Lua, and an unregistered name answers with an error
    /// that names the kiln alone.
    pub fn with_kiln_repository_resolver(
        self,
        registry: Arc<crate::kiln_registry::KilnRegistry>,
        kiln_manager: Arc<crate::kiln_manager::KilnManager>,
    ) -> anyhow::Result<Self> {
        let resolver: crucible_lua::KilnRepositoryResolver = Arc::new(move |name: &str| {
            let name = name.to_string();
            let registry = Arc::clone(&registry);
            let kiln_manager = Arc::clone(&kiln_manager);
            Box::pin(async move {
                let (kiln_name, path) = registered_kiln_path(&registry, &name)?;
                let handle = kiln_manager
                    .get_or_open(&path)
                    .await
                    .map_err(|e| format!("kiln '{kiln_name}' did not open: {e}"))?;
                Ok(handle.as_knowledge_repository())
            })
        });
        crucible_lua::register_kiln_repository_resolver(self.executor.lua(), resolver)
            .map_err(|e| anyhow::anyhow!("cru.kiln named reads (kiln resolver): {e}"))?;
        Ok(self)
    }

    /// Wire `cru.embed` to the provider a named kiln embeds with — the same
    /// one the `kiln.embed_query` RPC uses.
    pub fn with_embed_resolver(
        self,
        registry: Arc<crate::kiln_registry::KilnRegistry>,
        kiln_manager: Arc<crate::kiln_manager::KilnManager>,
    ) -> anyhow::Result<Self> {
        let resolver: crucible_lua::EmbedResolver = Arc::new(move |name: &str| {
            let name = name.to_string();
            let registry = Arc::clone(&registry);
            let kiln_manager = Arc::clone(&kiln_manager);
            Box::pin(async move {
                let (kiln_name, _) = registered_kiln_path(&registry, &name)?;
                // The provider is one per config, never per connection, so
                // this does not open the kiln: `process_batch` holds the
                // connection map while `index:blocks` fires, and a handler
                // that waited on it would wait on itself.
                kiln_manager
                    .embedding_provider()
                    .await
                    .map_err(|e| format!("kiln '{kiln_name}' has no embedder: {e}"))
            })
        });
        crucible_lua::register_embed_resolver(self.executor.lua(), resolver)
            .map_err(|e| anyhow::anyhow!("cru.embed (kiln resolver): {e}"))?;
        Ok(self)
    }

    /// Where persisted plugin options live, for the RPC layer that records
    /// them. `None` when no data root was bound — nothing is persisted.
    pub fn option_store_dir(&self) -> Option<&Path> {
        self.option_store_dir.as_deref()
    }

    /// Handlers registered by plugins via `cru.on`.
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

    /// Panels plugins declared, for the RPC layer.
    pub fn surfaces(&self) -> SurfaceRegistry {
        self.surfaces.clone()
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
    /// it meant a session raced plugin boot for a working binding.
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
        // Held as well as bound: `make_plugin_inert` has to take a plugin's
        // values back, and the bind alone leaves the loader with no handle on
        // the store the VM now writes to.
        if self.statusline_exprs.set(Arc::clone(&registry)).is_err() {
            tracing::warn!("statusline expression registry was already bound");
        }
        crucible_lua::register_statusline_exprs(lua, &cru, registry)
            .map_err(|e| anyhow::anyhow!("statusline module: {e}"))
    }

    /// Fire `cru.on_session_start` hooks registered by plugins.
    ///
    /// Syncs first: hooks live in Lua globals until pulled into the executor's
    /// list, and plugins register them at load — long before any session
    /// exists. Without this the plugin runtime's lifecycle hooks never ran at
    /// all, so a plugin that registers its `cru.on` handlers inside
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
            .fire_session_start_hooks(session)
            .await
            .map_err(|e| anyhow::anyhow!("fire session_start hooks: {e}"))
    }

    /// Fire `cru.on_session_end` hooks registered by plugins.
    ///
    /// Teardown failures are reported but must not block the session ending —
    /// refusing to end a session leaves the user stuck, which is the opposite
    /// of the start-hook tradeoff.
    pub async fn fire_session_end(
        &mut self,
        session: &crucible_lua::Session,
    ) -> anyhow::Result<()> {
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

    /// The mode store this VM writes.
    ///
    /// `AgentManager` adopts the handle, so a mode declared in the user's
    /// `init.lua` is read by every session built afterwards.
    pub fn mode_registry(&self) -> crucible_lua::ModeRegistry {
        self.modes.clone()
    }

    /// The permission hooks registered on this VM, for the tool gate.
    pub fn permission_registry(&self) -> crate::agent_manager::DaemonPermissions {
        (self.handler_registry.clone(), self.plugin_lua())
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

    /// Register plugin config as `cru.plugin.config` in the Lua runtime.
    ///
    /// Provides `cru.plugin.config.get("plugin_name.key")` for dotted-key
    /// lookup into the `plugins.*` config subtrees. Deliberately NOT
    /// `cru.config`: that name is the app-config store (`get`/`set`), and a
    /// plugin's own section is a different thing — the plugin seam owns
    /// plugin-scoped state.
    fn register_plugin_config(
        lua: &mlua::Lua,
        config: HashMap<String, serde_json::Value>,
    ) -> Result<(), mlua::Error> {
        let config_table = lua.create_table()?;

        // Store the raw config data as a Lua table
        let data = lua.to_value(&config)?;
        config_table.set("_data", data)?;

        // cru.plugin.config.get("namespace.key") -> value
        //
        // Declared through `Ns::over` on the table built above, so the string
        // is checked against this closure's own Rust types like every other
        // `cru.*` function. It answers `nil` for a key that is not there and
        // for a segment that is not a table, so the return is `any?`.
        let mut ns = crucible_lua::Ns::over(lua, "cru.plugin.config", config_table.clone());
        ns.func("get", "(key: string) -> any?", |lua, key: String| {
            let globals = lua.globals();
            let cru: mlua::Table = globals.get("cru")?;
            let plugin: mlua::Table = cru.get("plugin")?;
            let config: mlua::Table = plugin.get("config")?;
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
        })
        .map_err(|e| mlua::Error::runtime(format!("cru.plugin.config.get: {e}")))?;

        let plugin = crucible_lua::lua_util::get_or_create_module(lua, "plugin")?;
        plugin.set("config", config_table)?;

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
    /// Call after session/agent managers are created. Replaces stub `cru.session.*`
    /// functions with implementations that delegate to the provided API. Also
    /// registers `cru.context.*` (Wave 1 plugin closure surface) which shares
    /// the same [`DaemonSessionApi`].
    pub fn upgrade_with_sessions(&self, api: Arc<dyn DaemonSessionApi>) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        // Bound to this VM's current-session holder so a `delegate = true`
        // create stamps parentage from the session the daemon bound here —
        // never from anything a plugin wrote.
        crucible_lua::register_sessions_module_with_api_and_current(
            lua,
            Arc::clone(&api),
            self.executor.current_session().clone(),
        )
        .map_err(|e| anyhow::anyhow!("sessions upgrade: {e}"))?;
        if !self.session_api.install(Arc::clone(&api)) {
            warn!("the daemon session API was already installed; keeping the first");
        }
        register_ui_module_with_api(lua, Arc::clone(&api))
            .map_err(|e| anyhow::anyhow!("ui upgrade: {e}"))?;
        register_context_module(lua, api).map_err(|e| anyhow::anyhow!("context module: {e}"))?;
        info!("Lua sessions + ui + context modules upgraded with daemon API");
        Ok(())
    }

    /// Point the plugin VM's `cru.log.notify` at the daemon's hub. Until
    /// this runs the calls queue in the VM, which is what the stub
    /// generator and the unit tests read.
    pub fn upgrade_with_notify_sink(
        &self,
        sink: Arc<dyn crucible_lua::NotificationSink>,
    ) -> anyhow::Result<()> {
        crucible_lua::upgrade_with_notify_sink(self.executor.lua(), sink, None)
            .map_err(|e| anyhow::anyhow!("notify sink upgrade: {e}"))
    }

    /// The daemon-backed session API `upgrade_with_sessions` registered with,
    /// for late-created runtimes that want the same module against the same
    /// bridge. `None` before the upgrade has run.
    pub fn session_api(&self) -> Option<Arc<dyn DaemonSessionApi>> {
        self.session_api.get().map(Arc::clone)
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

    /// Put `plugin_paths` on the resolver and on the plugin manager, so
    /// `require("plugin")` resolves from user init, built-ins and other
    /// plugins, and discovery walks the same directories. Roots the boot
    /// already seeded stay in place.
    pub fn add_plugin_paths(
        &mut self,
        plugin_paths: &[(PathBuf, PluginSource)],
    ) -> anyhow::Result<()> {
        let roots = plugin_paths
            .iter()
            .map(|(path, _)| path)
            .filter(|path| path.exists())
            .cloned()
            .collect();
        self.executor
            .add_module_roots(roots)
            .map_err(|e| anyhow::anyhow!("configure module roots: {e}"))?;
        for (path, source) in plugin_paths {
            self.plugin_manager
                .add_search_path_with_source(path.clone(), *source);
        }
        Ok(())
    }

    /// Discover every plugin on the search paths. Discovery reads each
    /// plugin's fragment in the daemon VM and runs no plugin code.
    fn discover(&mut self) -> anyhow::Result<()> {
        let discovered = self
            .plugin_manager
            .discover(self.executor.lua())
            .map_err(|e| anyhow::anyhow!("plugin discover: {e}"))?;
        for name in discovered {
            if !self.discovery_order.contains(&name) {
                self.discovery_order.push(name);
            }
        }
        Ok(())
    }

    /// The spec-driven pass: discover, then activate every plugin the merged
    /// spec names with a resolved `enabled` of `true`, and every plugin a
    /// boot `require` in `init.lua` loaded, in discovery order.
    ///
    /// A discovered plugin with no entry and no require stays `Discovered`
    /// and inactive: that is the lazy.nvim rule, and the Builtin fragment in
    /// `runtime/defaults/init.luau` is what keeps the shipped set active. A
    /// spec entry with a `Git` source and no directory is the bootstrap's
    /// job, which runs before this. One broken plugin does not stop the
    /// pass: `activate` marks it `Error` and inert, and the pass goes on.
    pub async fn load_plugins_from_spec(&mut self) -> anyhow::Result<()> {
        self.discover()?;
        let spec = crucible_lua::spec_of(self.executor.lua());
        let mut activated = 0usize;
        for name in self.discovery_order.clone() {
            let Some(plugin) = self.plugin_manager.get(&name) else {
                continue;
            };
            let init_path = plugin.main_path();
            let wanted =
                spec.get(&name).is_some() || self.boot_required_module(&init_path).is_some();
            if !wanted {
                debug!("plugin '{name}' has no spec entry; it stays discovered");
                continue;
            }
            match activate::activate(self, &name).await {
                Ok(_) => activated += 1,
                Err(e) => warn!("plugin '{name}' did not activate: {e}"),
            }
        }
        info!("Activated {activated} daemon plugin(s) from the spec");
        Ok(())
    }

    /// Activate one discovered plugin by name: a runtime install, or an
    /// entry the bootstrap cloned. Idempotent.
    pub async fn activate_plugin(&mut self, name: &str) -> anyhow::Result<()> {
        if self.plugin_manager.get(name).is_none() {
            self.discover()?;
        }
        activate::activate(self, name).await.map(|_| ())
    }

    /// Add `plugin_paths`, discover, and activate EVERY discovered plugin
    /// whose resolved `enabled` is `true`, entry or no entry.
    ///
    /// A test policy. Production activates the spec
    /// (`load_plugins_from_spec`); a fixture plugin in a temporary directory
    /// has no entry, and every such test wants it active.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn activate_discovered(
        &mut self,
        plugin_paths: &[(PathBuf, PluginSource)],
    ) -> anyhow::Result<()> {
        self.add_plugin_paths(plugin_paths)?;
        self.discover()?;
        for name in self.discovery_order.clone() {
            if let Err(e) = activate::activate(self, &name).await {
                warn!("plugin '{name}' did not activate: {e}");
            }
        }
        Ok(())
    }

    /// Run `code` as the operator's own `init.lua` would run: under
    /// `LuaSource::UserLua`, with the source restored afterwards.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn eval_user_init(&self, code: &str) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        let previous = crucible_lua::set_source(lua, crucible_lua::LuaSource::UserLua);
        let outcome = lua.load(code).set_name("=init.lua").exec_async().await;
        crucible_lua::set_source(lua, previous);
        outcome.map_err(|e| anyhow::anyhow!("user init: {e}"))
    }

    /// The module name a boot `require` loaded `init_path` under, if any.
    ///
    /// The resolver records which file answered each public name, so this
    /// matches by FILE: a plugin required as `x` or as `x.init` is found
    /// either way, and a table a plugin put in `package.loaded` itself is
    /// not.
    fn boot_required_module(&self, init_path: &Path) -> Option<String> {
        let canonical =
            std::fs::canonicalize(init_path).unwrap_or_else(|_| init_path.to_path_buf());
        self.executor
            .modules()
            .loaded_modules()
            .into_iter()
            .find(|(_, file)| *file == canonical)
            .map(|(name, _)| name)
    }

    /// The `plugins.<name>` config section for a plugin: the directory name
    /// first, then the name the plugin's fragment declares. A repo cloned as
    /// `crucible-discord` whose fragment says `name = "discord"` still gets
    /// its `plugins.discord` section.
    fn config_section(&self, name: &str, declared: Option<&str>) -> Option<serde_json::Value> {
        self.plugin_config
            .get(name)
            .or_else(|| declared.and_then(|d| self.plugin_config.get(d)))
            .cloned()
    }

    /// The config leaf `plugins.<name>.enabled` for the bootstrap, which
    /// knows the manifest name alone: no fragment is discovered before the
    /// clone, so there is no declared name to fall back to.
    pub(crate) fn config_enabled_leaf(&self, name: &str) -> Option<bool> {
        enabled_leaf_of(self.config_section(name, None).as_ref())
    }

    /// The manager's state for `name`, or `None` for a name discovery never
    /// saw.
    pub fn plugin_state(&self, name: &str) -> Option<crucible_lua::manifest::PluginState> {
        self.plugin_manager.get(name).map(|p| p.state)
    }

    /// The daemon VM.
    pub fn lua(&self) -> &mlua::Lua {
        self.executor.lua()
    }

    /// The operator's runtime kill switch: `on_unload`, then inert, then
    /// `Disabled`. `activate` refuses the plugin until something enables
    /// it.
    pub fn disable_plugin(&mut self, name: &str) {
        self.make_plugin_inert(name);
        if let Err(e) = self.plugin_manager.disable(name) {
            warn!("plugin '{name}' could not be disabled: {e}");
        }
    }

    /// Upsert by plugin name. A replacement of the whole map here would drop
    /// every previously loaded plugin's entry the moment an activation pass
    /// runs a second time — which it does once `plugin.install` activates at
    /// runtime: `activate` answers an Active plugin's stored table, so its spec
    /// is absent from any later pass.
    fn remember_spec(&mut self, name: &str, spec: PluginSpec) {
        self.loaded_specs.insert(name.to_string(), spec);
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

    /// Remove every registration attributed to `name`. "Not Active" must imply
    /// "nothing of this plugin's is registered or running." Keep this
    /// synchronous and call it adjacent to `mark_error` with no await between
    /// them — the loader mutex is what makes the Active-but-inert window
    /// unobservable.
    ///
    /// `IsolationRegistry` and `StatusRegistry` are session-keyed, not
    /// plugin-keyed, so no plugin-scoped release exists for them or is needed.
    /// `StatuslineExprRegistry` is session-keyed TOO and still needs one: it
    /// records the source of each value, so the plugin's are nameable, and a
    /// value left in it stays painted in every attached client. Being keyed by
    /// session is not by itself a reason a store needs no plugin-scoped
    /// release, and reading it as one is what let this hide.
    fn make_plugin_inert(&mut self, name: &str) {
        // `on_unload` first, while the plugin's registrations still stand:
        // the hook may flush through them. One generation, one call.
        let lua = self.executor.lua().clone();
        self.plugin_manager.call_on_unload_hook(&lua, name);
        // Not active means no module table: the next `activate` runs the
        // file again.
        if let Some(key) = self.active_modules.remove(name) {
            let _ = lua.remove_registry_value(key);
        }
        self.abort_services(name);
        self.plugin_registry.remove_plugin(name);
        // One call, every store this plugin can have written: `cru.on`
        // handlers, permission hooks, both session hooks, provider auth hooks
        // and its schedules. It used to clear five registries and miss the
        // rest, so a plugin marked Not Active still held live registrations.
        crucible_lua::clear_source(
            self.executor.lua(),
            &self.handler_registry,
            &crucible_lua::LuaSource::Plugin(name.to_string()),
        );
        self.publications.release_plugin(name);
        self.surfaces.release_plugin(name);
        self.options.release_plugin(name);
        // Its statusline values, in every session that has one. `clear_source`
        // above stops the next push; this stops the last one from staying
        // painted, and the release announces itself so a client repaints.
        if let Some(exprs) = self.statusline_exprs.get() {
            let dropped = exprs.release_source(&crucible_lua::LuaSource::Plugin(name.to_string()));
            if dropped > 0 {
                debug!(plugin = %name, dropped, "released statusline expression values");
            }
        }
        // Dropped RegistryKeys only mark their slots; reclaim them so repeated
        // failed reloads don't grow the Lua registry.
        self.executor.lua().expire_registry_values();
    }

    /// Reload a plugin: `on_unload`, inert, forget its module, activate.
    ///
    /// The old generation's services die before the new one's are
    /// extracted, and `activate` forgets the modules cached from under the
    /// plugin's directory, so its `lua/` modules are read again. A reload
    /// that fails leaves the plugin `Error` and inert, never half-alive: the
    /// everyday trigger is saving `init.lua` with a syntax error while the
    /// watcher is on, and the previous generation's tools and handlers must
    /// not stay live behind an `Error` label.
    pub async fn reload_plugin(&mut self, name: &str) -> anyhow::Result<PluginSpec> {
        if self.plugin_manager.get(name).is_none() {
            anyhow::bail!("plugin '{}' not found", name);
        }

        self.make_plugin_inert(name);
        // A boot `require` instance in `package.loaded` would be reused
        // rather than re-read; the reload forgets it with the rest.
        let dir = self.plugin_manager.get(name).map(|p| p.dir.clone());
        if let Some(dir) = dir {
            if let Err(e) = self
                .executor
                .modules()
                .invalidate_under(self.executor.lua(), &dir)
            {
                warn!("plugin '{name}': could not forget its cached modules: {e}");
            }
        }
        if let Err(e) = self.plugin_manager.unload(name) {
            self.plugin_manager.mark_error(name, e.to_string());
            anyhow::bail!("reload plugin '{name}': {e}");
        }

        activate::activate(self, name)
            .await
            .map_err(|e| anyhow::anyhow!("reload plugin '{name}': {e}"))?;
        // The old version's handler keys were just dropped by
        // re-registration; reclaim them so repeated reloads do not grow the
        // Lua registry.
        self.executor.lua().expire_registry_values();

        // Re-running `setup(opts)` replayed the ORIGINAL config, so every
        // value the user changed in the settings pane just reverted. Replayed
        // here, on the loader, because both reload paths — the RPC handler
        // and the file watcher — go through this function.
        if let Some(dir) = &self.option_store_dir {
            option_store::restore_plugin(dir, &self.options, name);
        }

        info!("Reloaded plugin '{}' successfully", name);
        let spec =
            self.loaded_specs.get(name).cloned().ok_or_else(|| {
                anyhow::anyhow!("reload plugin '{name}': no declarations were read")
            })?;
        Ok(spec)
    }

    /// Manager key (manifest `name`) for the plugin discovered at `dir`.
    ///
    /// The installed manifest and clone directories go by the URL's last
    /// segment; the plugin manager goes by the name the spec table declares.
    /// For a repo `crucible-discord` whose entry file returns
    /// `name = "discord"` the two differ, and resolving by URL name silently
    /// misses the running plugin — the directory is the one identity both
    /// sides share.
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
            // Recorded in the installed manifest but never discovered by
            // this daemon (clone deleted by hand, bootstrap failed at boot):
            // nothing to deactivate is not a refusal. Erroring here made a
            // stale record unremovable for as long as the daemon ran — the
            // caller's installed precondition already guards typos.
            Err(crucible_lua::lifecycle::LifecycleError::NotFound(_)) => {}
            Err(e) => return Err(e.into()),
        }
        self.make_plugin_inert(name);
        self.loaded_specs.remove(name);
        self.discovery_order.retain(|n| n != name);
        // Without forget, the entry stays in the manager map (state
        // Discovered): plugin.list still shows it, and — because discover()
        // skips known names — a reinstall loads nothing and reports success.
        self.plugin_manager.forget(name);
        Ok(())
    }

    /// The names of the plugins with a remembered spec, sorted.
    pub fn loaded_plugin_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.loaded_specs.keys().cloned().collect();
        names.sort();
        names
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
                let spec = self.loaded_specs.get(&p.manifest.name);
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
            .map_err(|e| anyhow::anyhow!("stub generation: {e}"))?;
        // The session and config VMs carry a DIFFERENT `cru.*` surface, and
        // shipped files run on them: `runtime/defaults/init.lua` on the session
        // VM, `runtime/themes/*.lua` on the config VM. Checking those against
        // this VM's file reports type errors for working API.
        crate::vm_profiles::write_other_definitions(output_dir)
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
        // An eval arrives over a socket, so it is its own owner. Without this
        // bracket it ran as the user's own `init.lua`: its `cru.on` handler
        // could intercept a tool call, no clear path could ever remove it, and
        // its `cru.config.set` pinned a leaf no file holds.
        let previous = crucible_lua::set_source(lua, crucible_lua::LuaSource::Eval);
        let result = lua
            .load(&code)
            .set_name("=lua.eval")
            .eval_async::<mlua::Value>()
            .await;
        // Restored before the `?`: an owner left behind attributes whatever
        // runs next to the socket.
        crucible_lua::set_source(lua, previous);
        let result =
            result.map_err(|e| anyhow::anyhow!("{}", crucible_lua::format_lua_error(None, &e)))?;

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

/// The `enabled` leaf of a `plugins.<name>` config section, or `None` when
/// no layer wrote it. The one reader of the leaf: activation and the
/// bootstrap both pass its answer to `resolve::resolve_enabled`, so the two
/// answer one question.
pub(crate) fn enabled_leaf_of(section: Option<&serde_json::Value>) -> Option<bool> {
    section
        .and_then(|section| section.get("enabled"))
        .and_then(serde_json::Value::as_bool)
}

#[cfg(test)]
mod tests;
