//! The one-VM config boot: seed, evaluate `init.lua`, extract.
//!
//! The daemon used to build config-derived state from a config handed in at
//! bind, load plugins, and evaluate `init.lua` LAST. This module inverts
//! that: it creates THE plugin VM, seeds the module search path from the
//! locations that exist before any user file runs, evaluates `init.lua`
//! once, and extracts the config the daemon then binds with. Plugin
//! ACTIVATION stays a deferred phase after the evaluation — Neovim's model:
//! the search space is read live at every `require`; plugin execution is one
//! deferred phase after the user config.

use anyhow::Context;
use crucible_core::config::{CliAppConfig, SourceTag};
use crucible_lua::{ModuleRegistry, ModuleRequest, PluginSource, RootKind};
use mlua::{Lua, Table, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

use crucible_lua::{
    register_options_module, register_publish_module, OptionsRegistry, PublicationRegistry,
};

use super::{daemon_plugin_paths, DaemonPluginLoader};

/// How long `init.lua` may run at boot. Enforced by the T0.3 instruction
/// hook (interrupts a busy loop) plus a tokio timeout (cancels an await).
const BOOT_EVAL_BUDGET: Duration = Duration::from_secs(30);

/// The `cru.*` namespaces that answer from daemon state. The kiln registry
/// is built FROM the evaluation's output, so it cannot exist during it — a
/// top-level read raises and the file moves it into a hook.
const BOOT_GUARDED_NAMESPACES: [&str; 3] = ["kiln", "sessions", "storage"];

/// What the boot `require` machinery learned, stored in the VM's app data.
///
/// The searcher records which plugin entry modules the user's `require`
/// loaded (so activation reuses the same `package.loaded` instance instead
/// of evaluating the file a second time) and which plugins' `setup` the user
/// called directly (so activation skips the default `setup(cfg)` for them).
#[derive(Default)]
pub(crate) struct BootRequireState {
    /// Whether the boot phase is live. The searcher is inert when false.
    active: bool,
    /// The plugin search roots (canonicalized), for attributing a required
    /// file to a plugin.
    plugin_roots: Vec<PathBuf>,
    /// Module name → the plugin entry file the user's `require` loaded.
    loaded_modules: HashMap<String, PathBuf>,
    /// Plugins whose `setup` the user called during the evaluation.
    user_setup: HashSet<String>,
    /// The tables whose `setup` the boot wrapped, with the plugin's ORIGINAL
    /// function — restored when the boot phase ends, so a module table in
    /// `package.loaded` holds the plugin's own function for the process
    /// lifetime, not the recorder.
    wrapped_setups: Vec<(Table, mlua::Function)>,
}

impl BootRequireState {
    fn record_module(lua: &Lua, name: &str, file: PathBuf) {
        if let Some(mut state) = lua.app_data_mut::<BootRequireState>() {
            state.loaded_modules.insert(name.to_string(), file);
        }
    }

    fn record_user_setup(lua: &Lua, plugin: &str) {
        if let Some(mut state) = lua.app_data_mut::<BootRequireState>() {
            state.user_setup.insert(plugin.to_string());
        }
    }

    fn record_wrapped_setup(lua: &Lua, table: Table, original: mlua::Function) {
        if let Some(mut state) = lua.app_data_mut::<BootRequireState>() {
            state.wrapped_setups.push((table, original));
        }
    }

    /// End-of-boot restore: every wrapped `setup` goes back to the plugin's
    /// own function. A plugin that stores `M.setup` and compares it later —
    /// or a user calling it after boot — must see the original.
    fn restore_wrapped_setups(lua: &Lua) {
        let wrapped = match lua.app_data_mut::<BootRequireState>() {
            Some(mut state) => std::mem::take(&mut state.wrapped_setups),
            None => Vec::new(),
        };
        for (table, original) in wrapped {
            if let Err(e) = table.set("setup", original) {
                warn!("could not restore a plugin's own setup after boot: {e}");
            }
        }
    }

    /// Whether the user called this plugin's `setup` during the evaluation.
    pub(crate) fn user_owns_setup(lua: &Lua, plugin: &str) -> bool {
        lua.app_data_ref::<BootRequireState>()
            .is_some_and(|state| state.user_setup.contains(plugin))
    }

    /// The module name whose boot `require` loaded exactly `file`, if any.
    ///
    /// Matched by FILE, not by name: a plugin whose declared name differs
    /// from its directory name is required under the directory-derived
    /// module name, and activation must still find the instance.
    pub(crate) fn module_for_file(lua: &Lua, file: &Path) -> Option<String> {
        let state = lua.app_data_ref::<BootRequireState>()?;
        state
            .loaded_modules
            .iter()
            .find(|(_, loaded)| loaded.as_path() == file)
            .map(|(name, _)| name.clone())
    }

    /// Plugin names the user's boot `require` loaded modules for.
    pub(crate) fn required_plugins(lua: &Lua) -> Vec<String> {
        lua.app_data_ref::<BootRequireState>()
            .map(|state| {
                state
                    .loaded_modules
                    .keys()
                    .map(|name| plugin_of_module(name).to_string())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// The module's owning plugin: the first dotted segment.
fn plugin_of_module(module: &str) -> &str {
    module.split('.').next().unwrap_or(module)
}

/// The hash of what a boot evaluation reads: `config.toml` (while it
/// exists) and `init.lua`, under the given config file's directory.
///
/// The daemon records it at boot and `config.effective` returns it; a client
/// that computes a different value over the same root warns "restart to
/// apply". A missing file hashes as absent, so creating or deleting either
/// file changes the hash too.
pub fn boot_input_hash(config_source: &Path) -> String {
    let config_root = config_source
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut hasher = blake3::Hasher::new();
    // Both names hash in. A user who renames `init.lua` to `init.luau` changes
    // the boot input, and the staleness warning has to notice.
    let init_paths: Vec<PathBuf> = crucible_lua::source_files::init_file_names()
        .iter()
        .map(|name| config_root.join(name))
        .collect();
    for file in std::iter::once(config_source).chain(init_paths.iter().map(|p| p.as_path())) {
        match std::fs::read(file) {
            Ok(bytes) => {
                hasher.update(&(bytes.len() as u64).to_le_bytes());
                hasher.update(&bytes);
            }
            Err(_) => {
                hasher.update(b"absent");
            }
        }
    }
    hasher.finalize().to_hex().to_string()
}

/// The result of the boot evaluation: the extracted config and the VM it
/// was evaluated in, ready to be handed to `Server::bind_with_plugin_config`.
pub struct BootConfig {
    /// The effective config: defaults, then the `config.toml` seed, then
    /// whatever `init.lua` set. On an evaluation failure this is the seed.
    pub config: CliAppConfig,
    /// THE plugin VM, `init.lua` already evaluated in it.
    pub loader: DaemonPluginLoader,
    /// The config FILE the seed came from (existing or not), for refusals
    /// and forwarding.
    pub config_source: PathBuf,
    /// Its directory: where `init.lua` lives.
    pub config_root: PathBuf,
    /// [`boot_input_hash`] over what this evaluation read.
    pub boot_hash: String,
    /// The failure the evaluation failed OPEN on, when there was one: the
    /// `init.lua` error (or budget overrun, or unextractable store) that made
    /// this config fall back to the seed. The boot only warns; `cru doctor`
    /// reports it as a check.
    pub eval_error: Option<String>,
}

/// How the boot resolves `runtimepath` entries to plugin directories.
///
/// Production is [`daemon_plugin_paths`]; a test injects a closure over its
/// fixture directories, so no test's search path can reach the developer's
/// real plugin directories.
pub type PluginPathsFn = Arc<dyn Fn(&[PathBuf]) -> Vec<(PathBuf, PluginSource)> + Send + Sync>;

/// The §9 boot sequence, steps 1-5: resolve the root, seed the store,
/// create the VM with a live search path, evaluate `init.lua` once under
/// the boot deadline, extract.
///
/// Evaluation fails open: a broken `init.lua` is warned about and the
/// daemon continues on the seed. A broken `config.toml` stays an error,
/// exactly as `CliAppConfig::load` treats it today.
pub async fn evaluate_boot_config(
    config_file: Option<PathBuf>,
    embedding_url: Option<String>,
    embedding_model: Option<String>,
) -> anyhow::Result<BootConfig> {
    evaluate_boot_config_with_paths(
        config_file,
        embedding_url,
        embedding_model,
        Arc::new(|rtp: &[PathBuf]| daemon_plugin_paths(rtp)),
    )
    .await
}

/// [`evaluate_boot_config`] with the plugin-path resolution injected as a
/// value — the hermetic door for tests.
pub async fn evaluate_boot_config_with_paths(
    config_file: Option<PathBuf>,
    embedding_url: Option<String>,
    embedding_model: Option<String>,
    plugin_paths: PluginPathsFn,
) -> anyhow::Result<BootConfig> {
    // Step 1: the config root. An explicitly named file must exist — a
    // typo'd `--config` must not silently read defaults (oracle parity).
    let explicit = config_file.is_some();
    let config_source = config_file.unwrap_or_else(CliAppConfig::default_config_path);
    if explicit && !config_source.exists() {
        anyhow::bail!(
            "Config file not found: {}. Try: `cru doctor`",
            config_source.display()
        );
    }
    let config_root = config_source
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf();

    // Step 2: seed the store — defaults, then config.toml when it exists,
    // through the oracle's parse path (legacy-key rejections, include pass).
    crucible_lua::begin_boot_store();
    let defaults =
        serde_json::to_value(CliAppConfig::default()).context("serialize default config")?;
    crucible_lua::merge_app_config_tagged(defaults, SourceTag::Default);
    if config_source.exists() {
        let seed = CliAppConfig::load_seed_value(&config_source)?;
        crucible_lua::merge_app_config_tagged(seed, SourceTag::Toml(config_source.clone()));
        warn!(
            "{} is a deprecated config source; run `cru config migrate` to move it into init.lua",
            config_source.display()
        );
    }

    // The seed must extract — this is where a malformed config.toml erred
    // under `CliAppConfig::load`, and it still errs here, before any Lua.
    let seed_store = crucible_lua::snapshot_store().expect("the store was just seeded");
    let mut seed_config = seed_store.extract().map_err(|e| {
        anyhow::anyhow!(
            "Failed to parse config file {}: {e}. Try: `cru doctor`",
            config_source.display()
        )
    })?;
    seed_config.source_map = Some(seed_store.provenance().clone());

    // Step 3: THE plugin VM, and the live module search path: the user
    // module entries first, then the default plugin locations (env path,
    // user plugins dir, shipped runtime), then the seed's own runtimepath
    // entries — membership that exists before any user file runs.
    let mut loader = DaemonPluginLoader::new(HashMap::new())?;
    let seed_rtp: Vec<PathBuf> = seed_config.runtimepath.clone();
    let mut eval_error: Option<String> = {
        let lua = loader.executor().lua();
        let modules = loader.executor().modules().clone();
        let plugin_dirs = plugin_paths(&seed_rtp);
        seed_boot_search_path(lua, &modules, &config_root, &plugin_dirs)?;
        install_boot_hook(
            &modules,
            PluginBindings {
                publications: loader.publications(),
                options: loader.options(),
            },
        );
        // The UI namespaces must exist on the VM that evaluates the user's
        // file, or `cru.colorscheme.setup{...}` is an index-nil error.
        crucible_lua::config::register_ui_namespaces(lua)?;

        // A `runtimepath` write during evaluation extends the search space
        // INSIDE the `cru.config.set` call, before it returns — Neovim's
        // invalidate-and-rebuild.
        let extender_paths = Arc::clone(&plugin_paths);
        let extender_modules = modules.clone();
        let extender_root = config_root.clone();
        crucible_lua::set_runtimepath_extender(Some(Arc::new(
            move |lua: &Lua, entries: &[String]| {
                let rtp: Vec<PathBuf> = entries.iter().map(PathBuf::from).collect();
                let dirs = extender_paths(&rtp);
                if let Err(e) = refresh_plugin_dirs(lua, &extender_modules, &extender_root, &dirs) {
                    warn!("runtimepath change did not reach the module search path: {e}");
                }
            },
        )));

        // Step 4: evaluate init.lua once, top to bottom, under the boot
        // deadline. Errors fail open onto the seed — ENTIRELY: the state
        // snapshot below rolls the store, theme, layout, geometry, syntax
        // and highlight groups back, and the failed VM is dropped after
        // this scope, taking hooks, handlers, `package.loaded` and every
        // `_G` mutation with it. "Seed plus whatever registered before the
        // error line" would depend on WHERE the file failed; the rollback
        // makes a broken config mean exactly what the warning says.
        // `init.luau` or `init.lua`, preferred first. A config directory
        // holding both is refused rather than resolved: the user edits one and
        // watches nothing happen otherwise.
        let init_path = match crucible_lua::source_files::init_file(&config_root) {
            Ok(Some(path)) => path,
            Ok(None) => config_root.join("init.lua"),
            Err(ambiguous) => {
                warn!("{ambiguous}");
                config_root.join("init.lua")
            }
        };
        let eval_error = if init_path.exists() {
            let pre_eval = crucible_lua::snapshot_state().expect("the config state is live");
            let error = evaluate_init_file(lua, &init_path).await.err();
            if error.is_some() {
                crucible_lua::install_state(pre_eval);
            }
            error
        } else {
            debug!("No init.lua at {}", init_path.display());
            None
        };

        // Before the phase ends: record every module the evaluation loaded
        // from under a plugin root, whatever loader served it. The searcher
        // records the entries it CLAIMED; a module whose name does not match
        // the entry shape (a plugin whose declared name differs from its
        // directory) went through the standard loader unobserved, and
        // without this sweep activation would execute its file a second
        // time — doubling every top-level hook and publish.
        if eval_error.is_none() {
            record_boot_loaded_modules(lua, &modules);
        }

        // The boot phase ends: the searcher goes inert, the extender comes
        // out, every wrapped `setup` is restored to the plugin's own
        // function, and the store withholds location keys from here on.
        crucible_lua::set_runtimepath_extender(None);
        if let Some(mut state) = lua.app_data_mut::<BootRequireState>() {
            state.active = false;
        }
        BootRequireState::restore_wrapped_setups(lua);
        eval_error
    };

    if eval_error.is_some() {
        // The VM is part of the rollback. A fresh loader over the restored
        // state is byte-for-byte the no-init.lua boot.
        loader = DaemonPluginLoader::new(HashMap::new())?;
        let lua = loader.executor().lua();
        let modules = loader.executor().modules().clone();
        seed_boot_search_path(lua, &modules, &config_root, &plugin_paths(&seed_rtp))?;
        if let Some(mut state) = lua.app_data_mut::<BootRequireState>() {
            state.active = false;
        }
        crucible_lua::config::register_ui_namespaces(lua)?;
    }

    // Step 5: extract the effective config. A store the evaluation left
    // unextractable falls back to the seed — fail open, like the
    // evaluation itself.
    let full_store = crucible_lua::snapshot_store().expect("the store is live");
    let mut config = match full_store.extract() {
        Ok(mut config) => {
            // The `--sources` surface reads `source_map`, which IS the
            // store's provenance now — every leaf the boot merged, with its
            // file:line for Lua call sites.
            config.source_map = Some(full_store.provenance().clone());
            config
        }
        Err(e) => {
            warn!("init.lua produced a config that does not extract ({e}); continuing on the seed");
            eval_error.get_or_insert_with(|| format!("the evaluated config does not extract: {e}"));
            crucible_lua::install_store(seed_store);
            seed_config
        }
    };
    config.apply_embedding_overrides(embedding_url, embedding_model);
    crucible_lua::end_boot_phase();

    // The plugin sections feed each plugin's default `setup(cfg)` in the
    // activation phase — from the FINAL store, not the seed.
    let (plugin_sections, _watch) = crate::daemon_plugins::split_plugins_config(&config.plugins);
    let loader = loader.with_plugin_config(plugin_sections)?;

    let boot_hash = boot_input_hash(&config_source);
    Ok(BootConfig {
        config,
        loader,
        config_source,
        config_root,
        boot_hash,
        eval_error,
    })
}

/// Evaluate one init.lua in the boot VM: guards on, budget armed. `Err`
/// carries the failure the caller fails open on (and reports).
async fn evaluate_init_file(lua: &Lua, init_path: &Path) -> Result<(), String> {
    let source = match std::fs::read_to_string(init_path) {
        Ok(source) => source,
        Err(e) => {
            warn!("Failed to read {}: {e}", init_path.display());
            return Err(format!("failed to read {}: {e}", init_path.display()));
        }
    };

    let guards = match install_boot_guards(lua) {
        Ok(guards) => guards,
        Err(e) => {
            warn!("boot guards failed to install: {e}");
            Vec::new()
        }
    };

    let outcome = {
        let _budget = crucible_lua::enter_handler_budget(
            lua,
            BOOT_EVAL_BUDGET,
            "the init.lua boot evaluation".to_string(),
        );
        let chunk_name = format!("@{}", init_path.display());
        tokio::time::timeout(
            BOOT_EVAL_BUDGET,
            lua.load(&source)
                .set_name(chunk_name)
                .eval_async::<mlua::Value>(),
        )
        .await
    };

    if let Err(e) = restore_boot_guards(lua, guards) {
        warn!("boot guards failed to restore: {e}");
    }

    match outcome {
        Ok(Ok(_)) => {
            info!("Evaluated user init: {}", init_path.display());
            Ok(())
        }
        Ok(Err(e)) => {
            warn!(
                "User init.lua error ({}): {e}; continuing on the seed",
                init_path.display()
            );
            Err(format!("{e}"))
        }
        Err(_) => {
            warn!(
                "init.lua evaluation exceeded its {} s budget ({}); continuing on the seed",
                BOOT_EVAL_BUDGET.as_secs(),
                init_path.display()
            );
            Err(format!(
                "the evaluation exceeded its {} s budget",
                BOOT_EVAL_BUDGET.as_secs()
            ))
        }
    }
}

/// Existing plugin directories → their canonical roots.
fn plugin_dir_roots(dirs: &[(PathBuf, PluginSource)]) -> Vec<PathBuf> {
    dirs.iter()
        .filter(|(dir, _)| dir.exists())
        .filter_map(|(dir, _)| std::fs::canonicalize(dir).ok())
        .collect()
}

/// Put the search roots on the VM's resolver: the user's `lua/` directory
/// first, then every plugin root.
///
/// The user entry is first so a user module shadows a same-named plugin
/// module, which is the order `package.path` used to encode as a string. The
/// resolver owns it now, so nothing a plugin runs can reorder it.
fn apply_search_roots(
    modules: &ModuleRegistry,
    config_root: &Path,
    plugin_roots: &[PathBuf],
) -> mlua::Result<()> {
    let mut roots: Vec<(PathBuf, RootKind)> = vec![(config_root.join("lua"), RootKind::User)];
    for root in plugin_roots {
        if !roots.iter().any(|(existing, _)| existing == root) {
            roots.push((root.clone(), RootKind::Plugin));
        }
    }
    modules.set_roots(roots)
}

/// Step 3's seeding: put the user and plugin roots on the resolver, and
/// store the boot require state.
fn seed_boot_search_path(
    lua: &Lua,
    modules: &ModuleRegistry,
    config_root: &Path,
    plugin_dirs: &[(PathBuf, PluginSource)],
) -> mlua::Result<()> {
    let plugin_roots = plugin_dir_roots(plugin_dirs);
    apply_search_roots(modules, config_root, &plugin_roots)?;

    lua.set_app_data(BootRequireState {
        active: true,
        plugin_roots,
        loaded_modules: HashMap::new(),
        user_setup: HashSet::new(),
        wrapped_setups: Vec::new(),
    });
    Ok(())
}

/// A `runtimepath` change during evaluation: recompute the plugin dirs,
/// update the searcher's view, rebuild the search path.
fn refresh_plugin_dirs(
    lua: &Lua,
    modules: &ModuleRegistry,
    config_root: &Path,
    plugin_dirs: &[(PathBuf, PluginSource)],
) -> mlua::Result<()> {
    let plugin_roots = plugin_dir_roots(plugin_dirs);
    if let Some(mut state) = lua.app_data_mut::<BootRequireState>() {
        state.plugin_roots = plugin_roots.clone();
    }
    apply_search_roots(modules, config_root, &plugin_roots)
}

/// Install the boot load hook on the module resolver.
///
/// The hook claims one shape only: a plugin ENTRY module — an undotted name
/// answered by `<plugin root>/<name>.lua` or `<plugin root>/<name>/init.lua`.
/// For those it runs the file under the plugin's context (so `cru.on`
/// registrations are attributed and a later reload can clear them), records
/// the module for activation reuse, and wraps the returned table's `setup` so
/// a direct user call is recorded as ownership.
///
/// Every other module — the user's own `lua/` tree, a plugin's private
/// submodule — is declined, and the resolver loads it the ordinary way. A
/// plugin's nested `require` therefore cannot rebind the publish attribution
/// away from the outer plugin. A user module that shadows a plugin module is
/// logged on the way past.
/// The three registries a plugin's own bindings are attributed to.
///
/// Grouped because they always travel together and always name the same
/// plugin: `publish`, `options` and `views` are one concept — what THIS plugin
/// contributes — split across three stores. Passing them as three arguments
/// made the loader's signature grow by one every time a fourth kind of
/// contribution appeared.
#[derive(Clone)]
pub(crate) struct PluginBindings {
    pub publications: PublicationRegistry,
    pub options: OptionsRegistry,
}

impl PluginBindings {
    /// Release this plugin's previous contributions and rebind all three to it.
    ///
    /// Release-then-bind, in that order and together: a reload's new closures
    /// must not sit beside the old version's state, and doing it per registry
    /// at three call sites is how one of them gets forgotten.
    fn rebind(&self, lua: &Lua, plugin: &str) -> mlua::Result<()> {
        self.publications.release_plugin(plugin);
        register_publish_module(lua, self.publications.clone(), plugin.to_string())?;
        self.options.release_plugin(plugin);
        register_options_module(lua, self.options.clone(), plugin.to_string())
    }
}

fn install_boot_hook(modules: &ModuleRegistry, bindings: PluginBindings) {
    let hook_modules = modules.clone();
    modules.set_load_hook(Some(Arc::new(
        move |lua: &Lua, request: &ModuleRequest| -> Option<mlua::Result<Value>> {
            let active = lua
                .app_data_ref::<BootRequireState>()
                .is_some_and(|state| state.active);
            if !active {
                return None;
            }

            if request.shadows_plugin {
                debug!(
                    module = %request.name,
                    user_file = %request.path.display(),
                    "user lua/ module shadows a plugin module"
                );
            }
            if !request.is_entry {
                return None;
            }

            let plugin = plugin_of_module(&request.name).to_string();
            Some(boot_load_plugin_module(
                lua,
                &hook_modules,
                &plugin,
                &request.path,
                &request.name,
                &bindings,
            ))
        },
    )));
}

/// Load one plugin entry module for a boot-phase `require`: execute the file
/// under the plugin's context, record it, and wrap its `setup`.
fn boot_load_plugin_module(
    lua: &Lua,
    modules: &ModuleRegistry,
    plugin: &str,
    file: &Path,
    module_name: &str,
    bindings: &PluginBindings,
) -> mlua::Result<Value> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| mlua::Error::RuntimeError(format!("read {}: {e}", file.display())))?;

    // Bind `cru.plugin.publish` / `cru.plugin.options` to THIS plugin before
    // its body runs, exactly as activation does — a shipped plugin publishes
    // its channel from its body, and an unbound publish would error the
    // user's `require`.
    bindings.rebind(lua, plugin)?;

    // The plugin's own `lua/` dir is resolvable for the duration of this
    // load, exactly as activation makes it — so an entry module's own
    // `require("submodule")` works, and no other plugin inherits it.
    let _module_scope = match file.parent() {
        Some(plugin_dir) => Some(modules.enter_plugin_root(plugin_dir)?),
        None => None,
    };

    // Context restored on every exit path: an unrestored context would
    // misattribute whatever the user's file registers next.
    //
    // The grants come from the manifest on disk, because this runs BEFORE
    // discovery: the user's `init.lua` requires a plugin, and nothing has read
    // a `PluginManager` entry for it yet. A plugin that declares its
    // capabilities only in the spec table it RETURNS cannot be read here at
    // all — the table does not exist until the body finishes — so its
    // top-level calls hold nothing and its handlers hold everything, once
    // activation merges the two sources.
    let grants = file
        .parent()
        .and_then(|dir| {
            crucible_lua::manifest::PluginManifest::discover(dir)
                .ok()
                .flatten()
        })
        .map(|manifest| manifest.grants())
        .unwrap_or_default();
    let previous = crucible_lua::enter_plugin(lua, plugin, grants);
    let result: mlua::Result<Value> = lua
        .load(&source)
        .set_name(format!("@{}", file.display()))
        .call(());
    crucible_lua::set_plugin_context(lua, previous);
    let value = result?;

    BootRequireState::record_module(lua, module_name, file.to_path_buf());

    if let Value::Table(table) = &value {
        if let Ok(original) = table.get::<mlua::Function>("setup") {
            let plugin_name = plugin.to_string();
            let recorder = lua.create_function(move |lua, ()| {
                BootRequireState::record_user_setup(lua, &plugin_name);
                Ok(())
            })?;
            // A Lua-side wrapper keeps the call semantics (async setups
            // included) exactly as the module wrote them.
            let wrapped: mlua::Function = lua
                .load(
                    r#"
local record, original = ...
return function(...)
    record()
    return original(...)
end
"#,
                )
                .call((recorder, original.clone()))?;
            table.set("setup", wrapped)?;
            BootRequireState::record_wrapped_setup(lua, table.clone(), original);
        }
    }

    Ok(value)
}

/// Record every module the evaluation loaded from under a plugin root.
///
/// The hook records the entries it CLAIMED; a module whose name does not
/// match the entry shape — a plugin whose declared name differs from its
/// directory — was loaded the ordinary way and is invisible to it. The
/// resolver knows both, so this sweep asks the resolver and activation
/// reuses by file identity rather than executing the file a second time.
fn record_boot_loaded_modules(lua: &Lua, modules: &ModuleRegistry) {
    let roots = match lua.app_data_ref::<BootRequireState>() {
        Some(state) => state.plugin_roots.clone(),
        None => return,
    };
    for (name, file) in modules.loaded_modules() {
        let already = lua
            .app_data_ref::<BootRequireState>()
            .is_some_and(|state| state.loaded_modules.contains_key(&name));
        if already {
            continue;
        }
        if roots.iter().any(|root| file.starts_with(root)) {
            BootRequireState::record_module(lua, &name, file);
        }
    }
}

/// Replace the daemon-state namespaces with tables that raise on any index.
fn install_boot_guards(lua: &Lua) -> mlua::Result<Vec<(String, Value)>> {
    let cru: Table = lua.globals().get("cru")?;
    let mut saved = Vec::new();
    for ns in BOOT_GUARDED_NAMESPACES {
        let original: Value = cru.get(ns)?;
        let guard = lua.create_table()?;
        let metatable = lua.create_table()?;
        let ns_name = ns.to_string();
        let index_fn = lua.create_function(move |_, (_t, key): (Table, String)| -> mlua::Result<()> {
            Err(mlua::Error::RuntimeError(format!(
                "daemon state is not ready during init.lua evaluation; use a hook (cru.{ns_name}.{key})"
            )))
        })?;
        metatable.set("__index", index_fn)?;
        guard.set_metatable(Some(metatable))?;
        cru.set(ns, guard)?;
        saved.push((ns.to_string(), original));
    }
    Ok(saved)
}

/// Put the real namespaces back after the evaluation.
fn restore_boot_guards(lua: &Lua, saved: Vec<(String, Value)>) -> mlua::Result<()> {
    let cru: Table = lua.globals().get("cru")?;
    for (ns, original) in saved {
        cru.set(ns.as_str(), original)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Plugin-path resolution over the fixture only: `<entry>/plugins` for
    /// each runtimepath entry, nothing else — no environment, no home.
    fn fixture_paths() -> PluginPathsFn {
        Arc::new(|rtp: &[PathBuf]| {
            rtp.iter()
                .map(|entry| entry.join("plugins"))
                .filter(|dir| dir.exists())
                .map(|dir| (dir, PluginSource::Runtime))
                .collect()
        })
    }

    fn write_fixture_plugin(root: &Path, name: &str, body: &str) {
        let dir = root.join("plugins").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.yaml"),
            format!("name: {name}\nversion: \"0.1.0\"\nmain: init.lua\n"),
        )
        .unwrap();
        std::fs::write(dir.join("init.lua"), body).unwrap();
    }

    async fn boot_with(config_root: &Path, init_lua: &str) -> BootConfig {
        std::fs::create_dir_all(config_root).unwrap();
        let config_file = config_root.join("config.toml");
        if !config_file.exists() {
            std::fs::write(&config_file, "").unwrap();
        }
        std::fs::write(config_root.join("init.lua"), init_lua).unwrap();
        evaluate_boot_config_with_paths(Some(config_file), None, None, fixture_paths())
            .await
            .expect("boot must not error for a Lua-level failure")
    }

    /// The order pin, one fixture file: a `require` BEFORE the runtimepath
    /// addition fails; the same `require` on the line AFTER it succeeds —
    /// the search space extends inside the `cru.config.set` call.
    #[tokio::test]
    async fn a_runtimepath_addition_serves_the_next_require_line() {
        let tmp = tempfile::tempdir().unwrap();
        let rtp = tmp.path().join("extra");
        write_fixture_plugin(&rtp, "op_probe", r#"return { name = "op_probe" }"#);

        let init = format!(
            r#"
local before = pcall(require, "op_probe")
cru.config.set({{ runtimepath = {{ [[{rtp}]] }} }})
local after, mod = pcall(require, "op_probe")
cru.config.set({{ order_pin = {{
    before = before,
    after = after and mod.name == "op_probe",
}} }})
"#,
            rtp = rtp.display()
        );
        let boot = boot_with(&tmp.path().join("config"), &init).await;

        let store = crucible_lua::get_app_config().expect("store is live");
        assert_eq!(
            store["order_pin"]["before"],
            json!(false),
            "the entry added on line N must serve no require BEFORE line N"
        );
        assert_eq!(
            store["order_pin"]["after"],
            json!(true),
            "the entry added on line N must serve the require AFTER line N"
        );
        assert_eq!(boot.config.runtimepath, vec![rtp]);
    }

    /// Daemon-state APIs raise during the evaluation: the kiln registry is
    /// built FROM the evaluation's output, so it cannot exist during it.
    #[tokio::test]
    async fn a_top_level_kiln_read_raises_the_not_ready_error() {
        let tmp = tempfile::tempdir().unwrap();
        let init = r#"
local ok, err = pcall(function() return cru.kiln.list end)
cru.config.set({ guard_probe = { ok = ok, err = tostring(err) } })
"#;
        boot_with(&tmp.path().join("config"), init).await;

        let store = crucible_lua::get_app_config().expect("store is live");
        assert_eq!(store["guard_probe"]["ok"], json!(false));
        let err = store["guard_probe"]["err"].as_str().unwrap();
        assert!(
            err.contains("daemon state is not ready during init.lua evaluation"),
            "the guard must name the phase and the remedy: {err}"
        );
        assert!(err.contains("use a hook"), "{err}");
    }

    /// An evaluation error fails open ENTIRELY: the config is the seed, and
    /// everything the file registered before the error line — hooks, theme —
    /// is rolled back with the VM. "Seed plus whatever ran before the error"
    /// would be a state that depends on WHERE the file failed.
    #[tokio::test]
    async fn a_failed_evaluation_continues_on_the_seed() {
        let tmp = tempfile::tempdir().unwrap();
        let config_root = tmp.path().join("config");
        std::fs::create_dir_all(&config_root).unwrap();
        std::fs::write(
            config_root.join("config.toml"),
            "default_kiln = \"seeded\"\n",
        )
        .unwrap();

        let boot = boot_with(
            &config_root,
            r#"
cru.config.set({ default_kiln = "from-lua" })
cru.on("turn:complete", function() end)
cru.colorscheme.setup({ name = "broken-config-theme" })
error("boom")
"#,
        )
        .await;

        assert_eq!(
            boot.config.default_kiln.as_deref(),
            Some("seeded"),
            "the extracted config must be the seed, not the half-applied Lua"
        );
        let store = crucible_lua::get_app_config().expect("store is live");
        assert_eq!(store["default_kiln"], json!("seeded"));

        // The registrations from before the error line are gone with the VM.
        let handlers = boot.loader.plugin_handlers();
        assert_eq!(
            handlers.runtime_handlers().lock().unwrap().len(),
            0,
            "a hook registered before the error must not survive the rollback"
        );
        assert_ne!(
            crucible_lua::get_theme_config().map(|t| t.name),
            Some("broken-config-theme".to_string()),
            "a theme set before the error must not survive the rollback"
        );
    }

    /// The rollback's rebuild must go through the SAME seeding as a clean
    /// boot: a daemon whose init.lua failed and a daemon with no init.lua
    /// at all must agree on the module search path. If the two paths could
    /// diverge, the bug the rollback exists to prevent would be relocated
    /// into the rollback itself.
    #[tokio::test]
    async fn a_failed_init_boot_and_a_no_init_boot_agree_on_the_search_path() {
        let tmp = tempfile::tempdir().unwrap();
        let rtp = tmp.path().join("extra");
        write_fixture_plugin(&rtp, "sp_probe", r#"return { name = "sp_probe" }"#);
        let config_root = tmp.path().join("config");
        std::fs::create_dir_all(&config_root).unwrap();
        std::fs::write(
            config_root.join("config.toml"),
            format!(
                "runtimepath = [{:?}]
",
                rtp.display().to_string()
            ),
        )
        .unwrap();

        // The search path is the resolver's root list now, not a string.
        let path_of = |boot: &BootConfig| -> Vec<std::path::PathBuf> {
            boot.loader.executor().modules().plugin_roots()
        };

        // Failed evaluation → fresh VM.
        std::fs::write(
            config_root.join("init.lua"),
            "error('boom')
",
        )
        .unwrap();
        let failed_boot = evaluate_boot_config_with_paths(
            Some(config_root.join("config.toml")),
            None,
            None,
            fixture_paths(),
        )
        .await
        .expect("fail-open boot");
        let failed_path = path_of(&failed_boot);
        drop(failed_boot);

        // No init.lua at all.
        std::fs::remove_file(config_root.join("init.lua")).unwrap();
        let clean_boot = evaluate_boot_config_with_paths(
            Some(config_root.join("config.toml")),
            None,
            None,
            fixture_paths(),
        )
        .await
        .expect("clean boot");
        let clean_path = path_of(&clean_boot);

        assert_eq!(
            failed_path, clean_path,
            "the rebuilt VM must be seeded exactly as a no-init boot"
        );
        // And the seeded path is real: the fixture root reached both.
        assert!(
            failed_path.iter().any(|root| root.starts_with(&rtp)),
            "precondition: the seed runtimepath reached the search roots: {failed_path:?}"
        );
    }

    /// After the boot phase, today's rules resume: a location key through
    /// the runtime `config.set` door is withheld and reported.
    #[tokio::test]
    async fn location_keys_are_withheld_again_after_the_boot_phase() {
        let tmp = tempfile::tempdir().unwrap();
        boot_with(&tmp.path().join("config"), "").await;

        let withheld =
            crucible_lua::merge_app_config(json!({ "kiln_path": "/elsewhere", "chat": {} }));
        assert_eq!(withheld, vec!["kiln_path".to_string()]);
        let store = crucible_lua::get_app_config().expect("store is live");
        assert!(store.get("kiln_path").is_none());
    }
}
