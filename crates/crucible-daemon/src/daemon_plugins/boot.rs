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
use crucible_lua::PluginSource;
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
const BOOT_GUARDED_NAMESPACES: [&str; 4] = ["kiln", "sessions", "storage", "graph"];

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
    /// `<config_root>/lua/?.lua` and `<config_root>/lua/?/init.lua` — always
    /// first on `package.path`, so a user module shadows a same-named plugin
    /// module.
    user_patterns: Vec<String>,
    /// The plugin search roots (canonicalized), for attributing a required
    /// file to a plugin.
    plugin_roots: Vec<PathBuf>,
    /// The module patterns those roots contribute, for the shadow check.
    plugin_patterns: Vec<String>,
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
    for file in [config_source, &config_root.join("init.lua")] {
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
    let eval_failed = {
        let lua = loader.executor().lua();
        let plugin_dirs = plugin_paths(&seed_rtp);
        seed_boot_search_path(lua, &config_root, &plugin_dirs)?;
        install_boot_searcher(lua, loader.publications(), loader.options())?;
        // The UI namespaces must exist on the VM that evaluates the user's
        // file, or `cru.colorscheme.setup{...}` is an index-nil error.
        crucible_lua::config::register_ui_namespaces(lua)?;

        // A `runtimepath` write during evaluation extends the search space
        // INSIDE the `cru.config.set` call, before it returns — Neovim's
        // invalidate-and-rebuild.
        let extender_paths = Arc::clone(&plugin_paths);
        crucible_lua::set_runtimepath_extender(Some(Arc::new(
            move |lua: &Lua, entries: &[String]| {
                let rtp: Vec<PathBuf> = entries.iter().map(PathBuf::from).collect();
                let dirs = extender_paths(&rtp);
                if let Err(e) = refresh_plugin_dirs(lua, &dirs) {
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
        let init_path = config_root.join("init.lua");
        let failed = if init_path.exists() {
            let pre_eval = crucible_lua::snapshot_state().expect("the config state is live");
            let ok = evaluate_init_file(lua, &init_path).await;
            if !ok {
                crucible_lua::install_state(pre_eval);
            }
            !ok
        } else {
            debug!("No init.lua at {}", init_path.display());
            false
        };

        // Before the phase ends: record every module the evaluation loaded
        // from under a plugin root, whatever loader served it. The searcher
        // records the entries it CLAIMED; a module whose name does not match
        // the entry shape (a plugin whose declared name differs from its
        // directory) went through the standard loader unobserved, and
        // without this sweep activation would execute its file a second
        // time — doubling every top-level hook and publish.
        if !failed {
            record_boot_loaded_modules(lua);
        }

        // The boot phase ends: the searcher goes inert, the extender comes
        // out, every wrapped `setup` is restored to the plugin's own
        // function, and the store withholds location keys from here on.
        crucible_lua::set_runtimepath_extender(None);
        if let Some(mut state) = lua.app_data_mut::<BootRequireState>() {
            state.active = false;
        }
        BootRequireState::restore_wrapped_setups(lua);
        failed
    };

    if eval_failed {
        // The VM is part of the rollback. A fresh loader over the restored
        // state is byte-for-byte the no-init.lua boot.
        loader = DaemonPluginLoader::new(HashMap::new())?;
        let lua = loader.executor().lua();
        seed_boot_search_path(lua, &config_root, &plugin_paths(&seed_rtp))?;
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
    })
}

/// Evaluate one init.lua in the boot VM: guards on, budget armed. Returns
/// whether the evaluation SUCCEEDED; the caller owns the fail-open rollback.
async fn evaluate_init_file(lua: &Lua, init_path: &Path) -> bool {
    let source = match std::fs::read_to_string(init_path) {
        Ok(source) => source,
        Err(e) => {
            warn!("Failed to read {}: {e}", init_path.display());
            return false;
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
            true
        }
        Ok(Err(e)) => {
            warn!(
                "User init.lua error ({}): {e}; continuing on the seed",
                init_path.display()
            );
            false
        }
        Err(_) => {
            warn!(
                "init.lua evaluation exceeded its {} s budget ({}); continuing on the seed",
                BOOT_EVAL_BUDGET.as_secs(),
                init_path.display()
            );
            false
        }
    }
}

/// The two module patterns a directory contributes.
fn patterns_for_dir(dir: &Path) -> [String; 2] {
    let dir = dir.to_string_lossy().replace('\\', "/");
    [format!("{dir}/?.lua"), format!("{dir}/?/init.lua")]
}

/// Existing plugin directories → their module patterns and canonical roots.
fn plugin_dir_patterns(dirs: &[(PathBuf, PluginSource)]) -> (Vec<String>, Vec<PathBuf>) {
    let mut patterns = Vec::new();
    let mut roots = Vec::new();
    for (dir, _source) in dirs {
        if !dir.exists() {
            continue;
        }
        patterns.extend(patterns_for_dir(dir));
        if let Ok(canonical) = std::fs::canonicalize(dir) {
            roots.push(canonical);
        }
    }
    (patterns, roots)
}

/// Set `package.path` to: the user patterns, then `patterns`, then whatever
/// the path already held (deduplicated, first position wins).
///
/// This is the invalidate-and-rebuild: every change reconstructs the same
/// ordered structure, so the user's `<config_root>/lua` entries stay first
/// and a rebuilt path never grows duplicate entries.
fn rebuild_package_path(
    lua: &Lua,
    user_patterns: &[String],
    plugin_patterns: &[String],
) -> mlua::Result<()> {
    let package: Table = lua.globals().get("package")?;
    let current: String = package.get("path")?;

    let mut seen: HashSet<&str> = HashSet::new();
    let mut parts: Vec<&str> = Vec::new();
    for pattern in user_patterns.iter().chain(plugin_patterns.iter()) {
        if seen.insert(pattern.as_str()) {
            parts.push(pattern.as_str());
        }
    }
    for pattern in current.split(';') {
        if !pattern.is_empty() && seen.insert(pattern) {
            parts.push(pattern);
        }
    }
    package.set("path", parts.join(";"))
}

/// Step 3's seeding: compute the user patterns, put them and the plugin
/// patterns on `package.path`, and store the boot require state.
fn seed_boot_search_path(
    lua: &Lua,
    config_root: &Path,
    plugin_dirs: &[(PathBuf, PluginSource)],
) -> mlua::Result<()> {
    let user_lua = config_root.join("lua");
    let user_patterns: Vec<String> = patterns_for_dir(&user_lua).into();
    let (plugin_patterns, plugin_roots) = plugin_dir_patterns(plugin_dirs);

    rebuild_package_path(lua, &user_patterns, &plugin_patterns)?;

    lua.set_app_data(BootRequireState {
        active: true,
        user_patterns,
        plugin_roots,
        plugin_patterns,
        loaded_modules: HashMap::new(),
        user_setup: HashSet::new(),
        wrapped_setups: Vec::new(),
    });
    Ok(())
}

/// A `runtimepath` change during evaluation: recompute the plugin dirs,
/// update the searcher's view, rebuild the search path.
fn refresh_plugin_dirs(lua: &Lua, plugin_dirs: &[(PathBuf, PluginSource)]) -> mlua::Result<()> {
    let (plugin_patterns, plugin_roots) = plugin_dir_patterns(plugin_dirs);
    let user_patterns = match lua.app_data_mut::<BootRequireState>() {
        Some(mut state) => {
            state.plugin_patterns = plugin_patterns.clone();
            state.plugin_roots = plugin_roots;
            state.user_patterns.clone()
        }
        None => Vec::new(),
    };
    rebuild_package_path(lua, &user_patterns, &plugin_patterns)
}

/// Install the boot searcher at `package.searchers[2]` — ahead of the
/// standard Lua loader, resolving through the same `package.path`.
///
/// For a module that resolves under a plugin search root it returns a loader
/// that stamps the plugin context around the file's execution (so `cru.on`
/// registrations are attributed and a later reload can clear them), records
/// the module for activation reuse, and wraps the returned table's `setup`
/// so a direct user call is recorded as ownership. For every other module it
/// declines and the standard loader proceeds identically — logging a debug
/// line when a user `lua/` module shadows a same-named plugin module.
fn install_boot_searcher(
    lua: &Lua,
    publications: PublicationRegistry,
    options: OptionsRegistry,
) -> mlua::Result<()> {
    let searcher = lua.create_function(move |lua, name: String| {
        let publications = publications.clone();
        let options = options.clone();
        let (active, plugin_patterns) = match lua.app_data_ref::<BootRequireState>() {
            Some(state) => (state.active, state.plugin_patterns.clone()),
            None => (false, Vec::new()),
        };
        if !active {
            return Ok(Value::Nil);
        }

        let package: Table = lua.globals().get("package")?;
        let searchpath: mlua::Function = package.get("searchpath")?;
        let path: String = package.get("path")?;
        let found: (Option<String>, Option<String>) = searchpath.call((name.clone(), path))?;
        let Some(file) = found.0 else {
            return Ok(Value::Nil);
        };
        let Ok(canonical) = std::fs::canonicalize(&file) else {
            return Ok(Value::Nil);
        };

        // A plugin ENTRY module is `<root>/<name>.lua` or
        // `<root>/<name>/init.lua` for the whole module name. A plugin's own
        // `lua/` submodule (or a dotted module under a root) is NOT claimed:
        // it loads through the standard loader under whatever plugin context
        // is already ambient, so a nested `require` inside a plugin body
        // cannot rebind the publish attribution away from the outer plugin.
        let is_plugin_entry = lua.app_data_ref::<BootRequireState>().is_some_and(|state| {
            state.plugin_roots.iter().any(|root| {
                canonical.strip_prefix(root).is_ok_and(|rel| {
                    rel == Path::new(&format!("{name}.lua"))
                        || rel == Path::new(&name).join("init.lua")
                })
            })
        });

        if !is_plugin_entry {
            // The user's lua/ (or some other path entry) answered. Say so
            // when a plugin module of the same name is being shadowed.
            if !plugin_patterns.is_empty() {
                let shadowed: (Option<String>, Option<String>) =
                    searchpath.call((name.clone(), plugin_patterns.join(";")))?;
                if let Some(plugin_file) = shadowed.0 {
                    debug!(
                        module = %name,
                        user_file = %file,
                        plugin_file = %plugin_file,
                        "user lua/ module shadows a plugin module"
                    );
                }
            }
            return Ok(Value::Nil);
        }

        let plugin = plugin_of_module(&name).to_string();
        let module_name = name.clone();
        let loader = lua.create_function(move |lua, _args: mlua::MultiValue| {
            boot_load_plugin_module(
                lua,
                &plugin,
                &canonical,
                &module_name,
                &publications,
                &options,
            )
        })?;
        Ok(Value::Function(loader))
    })?;

    lua.load(
        r#"
local searcher = ...
table.insert(package.searchers, 2, searcher)
"#,
    )
    .call::<()>(searcher)
}

/// Load one plugin entry module for a boot-phase `require`: execute the file
/// under the plugin's context, record it, and wrap its `setup`.
fn boot_load_plugin_module(
    lua: &Lua,
    plugin: &str,
    file: &Path,
    module_name: &str,
    publications: &PublicationRegistry,
    options: &OptionsRegistry,
) -> mlua::Result<Value> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| mlua::Error::RuntimeError(format!("read {}: {e}", file.display())))?;

    // Bind `cru.plugin.publish` / `cru.plugin.options` to THIS plugin before
    // its body runs, exactly as activation does — a shipped plugin publishes
    // its channel from its body, and an unbound publish would error the
    // user's `require`.
    publications.release_plugin(plugin);
    register_publish_module(lua, publications.clone(), plugin.to_string())?;
    options.release_plugin(plugin);
    register_options_module(lua, options.clone(), plugin.to_string())?;

    // The plugin's lua/ dir joins the search space exactly as activation
    // would add it, so an entry module's own `require("submodule")` works.
    if let Some(plugin_dir) = file.parent() {
        let lua_dir = plugin_dir.join("lua");
        if lua_dir.exists() {
            let (user_patterns, mut plugin_patterns) = lua
                .app_data_ref::<BootRequireState>()
                .map(|s| (s.user_patterns.clone(), s.plugin_patterns.clone()))
                .unwrap_or_default();
            plugin_patterns.extend(patterns_for_dir(&lua_dir));
            rebuild_package_path(lua, &user_patterns, &plugin_patterns)?;
        }
    }

    // Context restored on every exit path: an unrestored context would
    // misattribute whatever the user's file registers next.
    let previous = crucible_lua::enter_plugin(lua, plugin, false);
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

/// Record every `package.loaded` module whose file lies under a plugin
/// search root — the generalized half of the searcher's bookkeeping, run
/// once after the evaluation so activation can reuse by FILE identity.
fn record_boot_loaded_modules(lua: &Lua) {
    let roots = match lua.app_data_ref::<BootRequireState>() {
        Some(state) => state.plugin_roots.clone(),
        None => return,
    };
    let Ok(package) = lua.globals().get::<Table>("package") else {
        return;
    };
    let (Ok(loaded), Ok(searchpath), Ok(path)) = (
        package.get::<Table>("loaded"),
        package.get::<mlua::Function>("searchpath"),
        package.get::<String>("path"),
    ) else {
        return;
    };

    for pair in loaded.pairs::<Value, Value>() {
        let Ok((Value::String(name), _)) = pair else {
            continue;
        };
        let Ok(name) = name.to_str().map(|s| s.to_string()) else {
            continue;
        };
        let already = lua
            .app_data_ref::<BootRequireState>()
            .is_some_and(|state| state.loaded_modules.contains_key(&name));
        if already {
            continue;
        }
        let Ok((Some(file), _)) =
            searchpath.call::<(Option<String>, Option<String>)>((name.clone(), path.clone()))
        else {
            continue;
        };
        let Ok(canonical) = std::fs::canonicalize(&file) else {
            continue;
        };
        if roots.iter().any(|root| canonical.starts_with(root)) {
            BootRequireState::record_module(lua, &name, canonical);
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

        let path_of = |boot: &BootConfig| -> String {
            boot.loader
                .executor()
                .lua()
                .load("return package.path")
                .eval::<String>()
                .expect("package.path")
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
        // And the seeded path is real: the fixture plugin resolves on both.
        assert!(
            failed_path.contains("sp_probe") || failed_path.contains("extra"),
            "precondition: the seed runtimepath reached the search path: {failed_path}"
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
