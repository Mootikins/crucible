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

/// The hash of what a boot evaluation reads under the given config file's
/// directory: `init.lua`, `init.luau` and `settings.json`.
///
/// The daemon records it at boot and `config.effective` returns it; a client
/// that computes a different value over the same root warns "restart to
/// apply". A missing file hashes as absent, so creating or deleting the file
/// changes the hash too. `config.toml` is deliberately not in it: the boot
/// does not read that file, so editing it is not a reason to restart.
///
/// `settings.json` IS in it. [`load_settings_layer`] reads the file at every
/// boot, and `settings_file` states that a hand edit survives, so a hand edit
/// is a real change to what the running daemon would evaluate. Without it,
/// `cru doctor` reported a stale daemon as current.
pub fn boot_input_hash(config_source: &Path) -> String {
    let config_root = config_source
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut hasher = blake3::Hasher::new();
    // Both `init` names hash in. A user who renames `init.lua` to `init.luau`
    // changes the boot input, and the staleness warning has to notice.
    let boot_inputs: Vec<PathBuf> = crucible_lua::source_files::init_file_names()
        .iter()
        .map(|name| config_root.join(name))
        .chain(std::iter::once(crucible_core::config::settings_path(
            config_root,
        )))
        .collect();
    for file in boot_inputs.iter().map(|p| p.as_path()) {
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
    /// The effective config: defaults, then `settings.json`, then whatever
    /// `init.lua` set. On an evaluation failure this is the seed.
    pub config: CliAppConfig,
    /// THE plugin VM, `init.lua` already evaluated in it.
    pub loader: DaemonPluginLoader,
    /// The config file the `--config` flag named (existing or not), whose
    /// directory is the config root. For refusals and forwarding.
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

impl BootConfig {
    /// The file a user edits to change this config: the `init.lua` under the
    /// config root, whether or not it exists yet.
    ///
    /// Refusals name it. `config_source` is the `--config` path, which
    /// defaults to `config.toml` — a file nothing reads, so a message built
    /// from it sends the user to edit the wrong file.
    pub fn config_file(&self) -> PathBuf {
        crucible_lua::source_files::init_file(&self.config_root)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.config_root.join("init.lua"))
    }
}

/// Why the `init.lua` evaluation stopped, and what the boot owes the user
/// for it.
///
/// The two failures are not the same failure. A file that does not PARSE
/// states no intent: nothing can be read out of it, so the daemon refuses to
/// start and names the line rather than starting as though the file were
/// absent — silence is how a typo survives for weeks. A file that parses and
/// then RAISES states an intent that ran part way; the boot rolls the whole
/// state back and warns, so a broken config means exactly what the warning
/// says.
enum InitFailure {
    /// The file, or a config file it loaded, does not parse. Fatal.
    Syntax(crucible_lua::ConfigSyntaxError),
    /// It parsed, then raised or overran the boot budget. Fail open.
    Runtime(String),
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
/// Evaluation fails two ways. An `init.lua` that does not PARSE is an error
/// naming the line: nothing readable is in the file, so there is no intent
/// to fall back from. An `init.lua` that parses and then raises is warned
/// about, rolled back whole, and the daemon continues on the seed.
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
    // Step 1: the config root — the DIRECTORY the named file sits in, which
    // is where `init.lua` and `settings.json` are read from.
    let explicit = config_file.is_some();
    let config_source = config_file.unwrap_or_else(CliAppConfig::default_config_path);
    let config_root = config_source
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf();
    // A typo'd `--config` must not silently read defaults. What is checked is
    // the DIRECTORY, because that is what the boot reads: `init.lua` and
    // `settings.json` live there, and the named file itself is no longer read
    // at all. An existing directory with neither file in it is a legitimate
    // ask — it is how a test, or a user, boots on the defaults alone.
    if explicit && !config_root.is_dir() {
        anyhow::bail!(
            "Config directory not found: {}. Try: `cru doctor`",
            config_root.display()
        );
    }

    // Step 2: seed the store with the defaults. `config.toml` is NOT a layer
    // any more. v0.30.0 read it under `init.lua` and warned at every boot
    // that it was deprecated; this release drops the reader, so the file sets
    // nothing and — being unread — can no longer refuse a boot either.
    // `cru config migrate` still parses it, which is why
    // `CliAppConfig::load_seed_value` is still here.
    crucible_lua::begin_boot_store();
    let defaults =
        serde_json::to_value(CliAppConfig::default()).context("serialize default config")?;
    crucible_lua::merge_app_config_tagged(defaults, SourceTag::Default);
    if config_source.exists() {
        // Once per boot, naming the one command that ends it. A file whose
        // values silently stopped applying is the failure mode a deprecation
        // exists to prevent, so the warning says what changed, not that
        // something is deprecated.
        warn!(
            "{} is no longer read; run `cru config migrate` to move it into init.lua",
            config_source.display()
        );
    }

    // The machine layer: `settings.json`, beside `init.lua`, which
    // `config.save` writes. It loads BELOW `init.lua` and ABOVE a plugin's
    // declared default: a human's own line beats what a UI saved, and what a
    // UI saved beats what a plugin declared. Plugins run their `setup()`
    // during the evaluation of `init.lua`, which is why this file must load
    // before that evaluation rather than after it.
    //
    // It loads in the boot phase, where `LocationPolicy::Accept` holds, so a
    // HAND EDIT of this file can set the location keys — the same authority
    // `config.toml` has today. `config.save` cannot: it runs under
    // `LocationPolicy::Withhold`, where the store strips those keys and
    // reports them back to the caller.
    load_settings_layer(&config_root);

    // The seed must extract. Only the defaults and `settings.json` are in it
    // now, and `load_settings_layer` probes its own merge, so reaching this
    // error means the DEFAULTS do not extract — a build defect, not a user's
    // file.
    let seed_store = crucible_lua::snapshot_store().expect("the store was just seeded");
    let mut seed_config = seed_store.extract().map_err(|e| {
        anyhow::anyhow!("The default configuration does not extract: {e}. Try: `cru doctor`")
    })?;
    seed_config.source_map = Some(seed_store.provenance().clone());

    // Step 3: THE plugin VM, and the live module search path: the user
    // module entries first, then the default plugin locations (env path,
    // user plugins dir, shipped runtime), then the seed's own runtimepath
    // entries — membership that exists before any user file runs.
    let mut loader = DaemonPluginLoader::new(HashMap::new())?;
    let seed_rtp: Vec<PathBuf> = seed_config.runtimepath.clone();
    let init_failure: Option<InitFailure> = {
        let lua = loader.executor().lua();
        let modules = loader.executor().modules().clone();
        let plugin_dirs = plugin_paths(&seed_rtp);
        // Who each config write belongs to. The same two root lists the
        // module search path gets: a write from the config root is the
        // human's own line and pins the key, a write from a plugin root is
        // that plugin's default and loses to the settings the user saves.
        install_author_roots(&config_root, &plugin_dirs);
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
                // A root added mid-evaluation is a plugin root from its next
                // line on, so the classifier must learn it with the resolver.
                install_author_roots(&extender_root, &dirs);
                if let Err(e) = refresh_plugin_dirs(lua, &extender_modules, &extender_root, &dirs) {
                    warn!("runtimepath change did not reach the module search path: {e}");
                }
            },
        )));

        // Step 3b: the shipped defaults file, FIRST. It declares the modes,
        // the default prompt, the precognition formatter and the plan-mode
        // permission hook. `init.lua` runs after it, so overriding is
        // ordinary assignment and `cru.modes.auto = nil` removes.
        load_shipped_defaults(lua, &seed_rtp);

        // Step 4: evaluate init.lua once, top to bottom, under the boot
        // deadline. EVERY error rolls back onto the seed ENTIRELY: the state
        // snapshot below rolls the store, theme, layout, geometry, syntax
        // and highlight groups back, and the failed VM is dropped after
        // this scope, taking hooks, handlers, `package.loaded` and every
        // `_G` mutation with it. "Seed plus whatever registered before the
        // error line" would depend on WHERE the file failed; the rollback
        // makes a broken config mean exactly what the warning says.
        //
        // What the failure DECIDES differs, and the decision is below the
        // scope: a file that does not parse stops the boot, a file that
        // raised lets it continue on the rolled-back seed.
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
        let failure = if init_path.exists() {
            let pre_eval = crucible_lua::snapshot_state().expect("the config state is live");
            let error = evaluate_init_file(lua, &init_path).await.err();
            // Both failures roll back, and the rollback happens here rather
            // than at the decision below, because the snapshot is only live
            // inside this scope.
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
        if failure.is_none() {
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
        failure
    };

    // The fatal half of the rule. Everything above already rolled back, so
    // the store this leaves behind is the seed either way; what differs is
    // whether the daemon carries on with it. It does not carry on past a
    // file it cannot read: `init.lua` is the only config language a human
    // writes, and a daemon that starts on defaults after a mistyped bracket
    // reports the user's whole config as "no config".
    let mut eval_error: Option<String> = match init_failure {
        Some(InitFailure::Syntax(syntax)) => {
            // The location keys must stop being writable even on the way
            // out: the process that boots is not always the process that
            // called, and a store left in the boot posture accepts a
            // `config.set` that names a data root.
            crucible_lua::end_boot_phase();
            anyhow::bail!(
                "{syntax}. Crucible does not start on a config file it cannot read: \
                 fix the line, or move the file aside."
            );
        }
        Some(InitFailure::Runtime(message)) => Some(message),
        None => None,
    };

    if eval_error.is_some() {
        // The VM is part of the rollback. A fresh loader over the restored
        // state is byte-for-byte the no-init.lua boot.
        loader = DaemonPluginLoader::new(HashMap::new())?;
        let lua = loader.executor().lua();
        let modules = loader.executor().modules().clone();
        seed_boot_search_path(lua, &modules, &config_root, &plugin_paths(&seed_rtp))?;
        // The rollback drops the VM the defaults file ran in, so re-run it
        // here. Without this, ONE syntax error in a user's `init.lua` leaves
        // every session with no system prompt, no declared modes and no
        // plan-mode deny hook — the defaults file is the only definition of
        // all three, and nothing else loads it.
        load_shipped_defaults(lua, &seed_rtp);
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

    // The trees the resolved `runtimepath` puts executable code in, recorded
    // before the daemon binds its socket. `execution_roots::baseline` cannot
    // reach this list: `runtimepath` is a location key, so no supported write
    // puts it in `settings.json`, and the file that does carry it — `init.lua`
    // — takes a VM to read. This is the one point that holds the VM's answer
    // and still runs before any session can be built, so a plugin an agent
    // plants under `<entry>/plugins` is write-refused from the first session
    // on. See [`crate::execution_roots`].
    crate::execution_roots::record_runtimepath(&config.runtimepath);

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

/// Merge `settings.json` into the boot store, when the file is there.
///
/// Fail open twice. An absent file is the normal case — nothing has saved
/// yet — and never an error. A file that does not read is warned about and
/// skipped, because the machine layer holds preferences: a daemon that
/// refuses to start over one leaves the user no door to fix it.
///
/// The probe is the second half of that. A hand edit can give a key the
/// wrong type, and without the probe the failure would surface at
/// `seed_store.extract()`, whose message names `config.toml` — the file the
/// user did not touch.
fn load_settings_layer(config_root: &Path) {
    let settings = match crucible_core::config::load_settings(config_root) {
        Ok(Some(settings)) => settings,
        Ok(None) => return,
        Err(e) => {
            warn!("{e:#}; continuing without the saved settings");
            return;
        }
    };
    let mut probe = crucible_lua::snapshot_store().expect("the store was just seeded");
    probe.merge(settings.clone(), SourceTag::Settings);
    match probe.extract() {
        Ok(_) => {
            crucible_lua::merge_app_config_tagged(settings, SourceTag::Settings);
        }
        Err(e) => warn!(
            "{} does not extract ({e}); continuing without the saved settings",
            crucible_core::config::settings_path(config_root).display()
        ),
    }
}

/// Run the runtimepath's defaults file on `lua`.
///
/// Fail-open: a broken defaults file must not stop the boot. Called twice —
/// once before `init.lua`, and again on the fresh VM the rollback builds when
/// `init.lua` fails — because this file is the only definition of the default
/// prompt, the shipped modes and the plan-mode deny hook.
fn load_shipped_defaults(lua: &Lua, runtimepath: &[PathBuf]) {
    let (src, origin) = crate::runtime_defaults::load_defaults(runtimepath);
    debug!(source = %origin, "Loading Lua defaults");
    // This file ships with the daemon, so its registrations are the host's
    // own. Naming the owner is what lets a later clear tell a shipped mode
    // hook from a plugin's.
    let previous = crucible_lua::set_owner(lua, crucible_lua::Owner::Builtin);
    if let Err(e) = lua.load(&src).set_name(origin.to_string()).exec() {
        warn!(source = %origin, error = %e, "Failed to load Lua defaults (fail-open)");
    }
    crucible_lua::set_owner(lua, previous);
}

/// Evaluate one init.lua in the boot VM: guards on, budget armed. `Err`
/// carries the failure, already classified — see [`InitFailure`].
async fn evaluate_init_file(lua: &Lua, init_path: &Path) -> Result<(), InitFailure> {
    let source = match std::fs::read_to_string(init_path) {
        Ok(source) => source,
        Err(e) => {
            warn!("Failed to read {}: {e}", init_path.display());
            // An unreadable file is not an unparseable one. A permission or
            // I/O fault is the machine's, not the config's, and the daemon
            // must still come up so the user can fix it.
            return Err(InitFailure::Runtime(format!(
                "failed to read {}: {e}",
                init_path.display()
            )));
        }
    };

    let guards = match install_boot_guards(lua) {
        Ok(guards) => guards,
        Err(e) => {
            warn!("boot guards failed to install: {e}");
            Vec::new()
        }
    };

    // The user's own file, named as such. It is also what an unbracketed VM
    // answers, so this bracket buys one thing: an owner a previous load left
    // behind cannot claim the user's registrations.
    let previous = crucible_lua::set_owner(lua, crucible_lua::Owner::UserLua);
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
    crucible_lua::set_owner(lua, previous);

    if let Err(e) = restore_boot_guards(lua, guards) {
        warn!("boot guards failed to restore: {e}");
    }

    match outcome {
        Ok(Ok(_)) => {
            info!("Evaluated user init: {}", init_path.display());
            Ok(())
        }
        Ok(Err(e)) => match crucible_lua::config_syntax_error(&e) {
            Some(syntax) => {
                warn!("{} does not parse: {syntax}", init_path.display());
                Err(InitFailure::Syntax(syntax))
            }
            None => {
                warn!(
                    "User init.lua error ({}): {e}; continuing on the seed",
                    init_path.display()
                );
                Err(InitFailure::Runtime(format!("{e}")))
            }
        },
        Err(_) => {
            warn!(
                "init.lua evaluation exceeded its {} s budget ({}); continuing on the seed",
                BOOT_EVAL_BUDGET.as_secs(),
                init_path.display()
            );
            // The file parsed — it had to, to start running. A budget
            // overrun is a runtime failure.
            Err(InitFailure::Runtime(format!(
                "the evaluation exceeded its {} s budget",
                BOOT_EVAL_BUDGET.as_secs()
            )))
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

/// Install the roots that say which author a `cru.config.set` write belongs
/// to: the config directory pins a key, a plugin directory declares a default.
///
/// The resolver canonicalizes the file it loads, so a plugin's chunk name is
/// canonical and its root must be too. The config directory is registered in
/// both forms, because `init.lua` is loaded by the path the user named and a
/// symlinked config directory would otherwise match neither.
fn install_author_roots(config_root: &Path, plugin_dirs: &[(PathBuf, PluginSource)]) {
    let mut config = vec![config_root.to_path_buf()];
    if let Ok(canonical) = std::fs::canonicalize(config_root) {
        if !config.contains(&canonical) {
            config.push(canonical);
        }
    }
    crucible_lua::set_author_roots(crucible_lua::AuthorRoots::new(
        config,
        plugin_dir_roots(plugin_dirs),
    ));
}

/// Learn the plugin root a runtime install just created.
///
/// The boot resolves the author roots from the directories that EXIST while
/// it runs: `daemon_plugin_paths` and [`plugin_dir_roots`] both drop a
/// missing one. On a fresh machine `~/.config/crucible/plugins` is missing,
/// so only the config root is registered — and a plugin installed into that
/// directory afterwards sits UNDER the config root, matches no plugin root,
/// and every `cru.config.set` its `setup()` makes pins the key as if the
/// human had written it.
///
/// The fix registers on install rather than registering the directory at
/// boot while it does not exist. Two reasons. The install path is the one
/// place that knows the real destination, including a destination the boot
/// never enumerates. And the directory exists by the time this runs, so it
/// canonicalizes to the same form the module resolver gives the plugin's
/// chunk name; a boot-time registration could not canonicalize a missing
/// directory, and the literal path it would store fails to match wherever
/// the config home sits under a symlink.
pub(crate) fn learn_plugin_author_root(dir: &Path) {
    let root = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if crucible_lua::add_plugin_author_root(root.clone()) {
        debug!("Plugin author root learned at runtime: {}", root.display());
    }
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
    // No grants: this runs BEFORE discovery, so nothing has read a
    // `PluginManager` entry for the plugin yet. The one grant read as
    // authority is `intercept_tools`, and a boot-time require must not carry
    // it on a manifest nobody has admitted.
    let previous = crucible_lua::enter_plugin(lua, plugin, false);
    let result: mlua::Result<Value> = lua
        .load(&source)
        .set_name(format!("@{}", file.display()))
        .call(());
    crucible_lua::set_owner(lua, previous);
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
            config_root.join("settings.json"),
            "{ \"default_kiln\": \"seeded\" }",
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

        // The USER's registrations from before the error line are gone with
        // the VM. The shipped defaults file is re-run onto the fresh VM, so
        // its handlers are the ones that remain.
        let handlers = boot.loader.plugin_handlers();
        let names: Vec<String> = handlers.all().iter().map(|h| h.name.to_string()).collect();
        assert!(
            !names.contains(&"turn:complete".to_string()),
            "a hook registered before the error must not survive the rollback: {names:?}"
        );
        assert!(
            names.contains(&"precognition_format".to_string()),
            "the shipped defaults must be re-run onto the rollback VM, or one \
             typo in init.lua costs the user their prompt, modes and plan-mode \
             deny hook: {names:?}"
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
            config_root.join("settings.json"),
            serde_json::json!({ "runtimepath": [rtp.display().to_string()] }).to_string(),
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

    /// The machine layer reaches the effective config, and the human's own
    /// line beats it. Both halves in one test, because the layer is only
    /// worth loading if it loses to `init.lua`: a saved value that shadowed
    /// the user's file would take the file's authorship away silently.
    #[tokio::test]
    async fn a_saved_setting_reaches_the_config_and_the_users_own_line_beats_it() {
        let tmp = tempfile::tempdir().unwrap();
        let config_root = tmp.path().join("config");
        std::fs::create_dir_all(&config_root).unwrap();
        crucible_core::config::save_settings_delta(
            &config_root,
            json!({
                "default_kiln": "from-settings",
                "chat": { "model": "from-settings" },
            }),
        )
        .unwrap();

        let boot = boot_with(
            &config_root,
            "cru.config.set { chat = { model = \"from-init\" } }\n",
        )
        .await;

        assert_eq!(
            boot.config.default_kiln.as_deref(),
            Some("from-settings"),
            "a key no other layer holds must come from settings.json"
        );
        assert_eq!(
            boot.config.chat.model.as_deref(),
            Some("from-init"),
            "the human's own line must beat the saved value"
        );
        let sources = boot.config.source_map.expect("the boot records provenance");
        assert_eq!(
            sources.get("default_kiln").map(SourceTag::short),
            Some("settings")
        );
        assert_eq!(sources.get("chat.model").map(SourceTag::short), Some("lua"));
    }

    /// The boot order inverts the layer order, and the rank has to survive it.
    ///
    /// `settings.json` merges at step 2, before `init.lua` is evaluated, and a
    /// plugin's write happens DURING that evaluation — so the plugin writes
    /// last on every boot. This is the whole reason the store ranks a write
    /// rather than taking the last one: without the rank, a plugin default
    /// replaces the value the settings UI saved, and the user's saved
    /// preference is gone with no sign of it anywhere.
    #[tokio::test]
    async fn a_plugin_default_does_not_replace_a_saved_setting_at_boot() {
        let tmp = tempfile::tempdir().unwrap();
        let rtp = tmp.path().join("extra");
        write_fixture_plugin(
            &rtp,
            "pd_probe",
            "cru.config.set { chat = { model = \"plugin-default\" } }\nreturn {}\n",
        );
        let config_root = tmp.path().join("config");
        std::fs::create_dir_all(&config_root).unwrap();
        crucible_core::config::save_settings_delta(
            &config_root,
            json!({
                "runtimepath": [rtp.display().to_string()],
                "chat": { "model": "saved-by-the-user" },
            }),
        )
        .unwrap();

        let boot = boot_with(&config_root, "require(\"pd_probe\")\n").await;

        let sources = boot
            .config
            .source_map
            .clone()
            .expect("the boot records provenance");
        assert_eq!(
            boot.config.chat.model.as_deref(),
            Some("saved-by-the-user"),
            "a plugin default must not replace what the settings UI saved \
             (provenance says {:?})",
            sources.get("chat.model").map(SourceTag::short)
        );
        assert_eq!(
            sources.get("chat.model").map(SourceTag::short),
            Some("settings"),
            "and the leaf must still name the layer that owns it"
        );
    }

    /// A hand edit can give a key the wrong type. The daemon still starts,
    /// because the machine layer is a preference store and a daemon that
    /// refuses to boot over one leaves the user no door to fix it.
    #[tokio::test]
    async fn a_settings_file_that_does_not_extract_is_skipped_rather_than_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let config_root = tmp.path().join("config");
        std::fs::create_dir_all(&config_root).unwrap();
        std::fs::write(
            crucible_core::config::settings_path(&config_root),
            r#"{"chat": {"show_thinking": "yes please"}}"#,
        )
        .unwrap();

        let boot = boot_with(&config_root, "cru.config.set { default_kiln = \"live\" }\n").await;

        assert!(!boot.config.chat.show_thinking);
        assert_eq!(
            boot.config.default_kiln.as_deref(),
            Some("live"),
            "the boot must run on, and init.lua with it"
        );
    }
}
