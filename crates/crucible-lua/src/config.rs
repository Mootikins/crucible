//! Lua configuration loader
//!
//! Loads `init.lua` from the config directory and provides the `cru.include()` function.
//!
//! ## Config Locations
//!
//! - Global config: `~/.config/crucible/init.lua`
//! - Kiln config: `<kiln>/.crucible/init.lua` (optional override)
//!
//! ## Usage
//!
//! ```lua
//! -- ~/.config/crucible/init.lua
//!
//! -- Built-in modules are under cru.*
//! cru.statusline.setup({
//!     left = { cru.statusline.mode() },
//!     center = { cru.statusline.model() },
//!     right = { cru.statusline.context() },
//! })
//!
//! -- Include other config files
//! cru.include("keymaps.lua")  -- loads ~/.config/crucible/keymaps.lua
//! ```

use crate::error::LuaError;
use crate::theme::ThemeConfig;
use crucible_core::config::{ConfigStore, LocationPolicy, SourceTag};
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use tracing::{debug, info, warn};

const DEFAULT_THEME_LUA: &str = include_str!("../../../runtime/themes/default.luau");

/// Global config state - stores parsed configuration from Lua
#[derive(Debug, Default, Clone)]
pub struct ConfigState {
    pub theme: Option<ThemeConfig>,
    /// Highlight groups authored via `cru.hl.set/link`. Open namespace —
    /// plugins name their own — so it is a map, not a fixed struct.
    pub hl: crate::hl::HlRegistry,
    /// Per-surface geometry from `cru.geometry.setup{}`.
    pub ui: Option<crate::ui_geometry::UiGeometry>,
    /// Screen layout authored as ordered region lists.
    pub layout: Option<crate::statusline_items::Layout>,
    /// Code-highlighting config from `cru.syntax.setup{}`.
    pub syntax: Option<serde_json::Value>,
    /// The app-config store: one JSON object, per-leaf provenance, and the
    /// boot-phase flag. Written by `cru.config.set()`, the `config.set` RPC,
    /// and the daemon's seed; `None` until something seeds it.
    pub app_config: Option<ConfigStore>,
}

/// Thread-safe config registry
static CONFIG: std::sync::OnceLock<Arc<RwLock<ConfigState>>> = std::sync::OnceLock::new();

fn get_config() -> &'static Arc<RwLock<ConfigState>> {
    CONFIG.get_or_init(|| Arc::new(RwLock::new(ConfigState::default())))
}

/// Get the current theme configuration (if set via cru.colorscheme.setup())
pub fn get_theme_config() -> Option<ThemeConfig> {
    get_config().read().ok()?.theme.clone()
}

/// Set the theme configuration
fn set_theme_config(config: ThemeConfig) {
    if let Ok(mut state) = get_config().write() {
        state.theme = Some(config);
    }
}

/// Install a theme from outside the Lua evaluation path (the `ui.set_theme`
/// RPC). Same store `cru.colorscheme.setup{}` writes, so a switch and a config
/// reload cannot disagree.
pub fn set_theme_config_public(config: ThemeConfig) {
    set_theme_config(config);
}

/// Snapshot the highlight-group table.
pub fn get_hl_registry() -> crate::hl::HlRegistry {
    get_config()
        .read()
        .ok()
        .map(|s| s.hl.clone())
        .unwrap_or_default()
}

/// Define or replace one highlight group.
pub fn set_hl_group(name: String, group: crate::hl::HlGroup) {
    if let Ok(mut state) = get_config().write() {
        state.hl.insert(name, group);
    }
}

/// The authored layout, if a config defined one.
pub fn get_layout() -> Option<crate::statusline_items::Layout> {
    get_config().read().ok()?.layout.clone()
}

/// Store the screen layout.
pub fn set_layout(layout: crate::statusline_items::Layout) {
    if let Ok(mut state) = get_config().write() {
        state.layout = Some(layout);
    }
}

/// Per-surface geometry, if a theme set any.
pub fn get_ui_geometry() -> Option<crate::ui_geometry::UiGeometry> {
    get_config().read().ok()?.ui.clone()
}

/// Store per-surface geometry.
pub fn set_ui_geometry(geometry: crate::ui_geometry::UiGeometry) {
    if let Ok(mut state) = get_config().write() {
        state.ui = Some(geometry);
    }
}

/// Get the app config (set via `cru.config.set()` or seeded by the daemon).
pub fn get_app_config() -> Option<serde_json::Value> {
    Some(
        get_config()
            .read()
            .ok()?
            .app_config
            .as_ref()?
            .value()
            .clone(),
    )
}

/// Per-leaf provenance for the app config, for `cru config show --sources`.
pub fn get_app_config_provenance() -> Option<crucible_core::config::ProvenanceMap> {
    Some(
        get_config()
            .read()
            .ok()?
            .app_config
            .as_ref()?
            .provenance()
            .clone(),
    )
}

/// Seed app config from an already-loaded config value.
///
/// Installs a RUNTIME store: the location-naming keys are withheld from the
/// plugin-visible view (see `LOCATION_CONFIG_KEYS` — a plugin is told which
/// kilns a session reaches by *name*). The daemon's real boot path does not
/// come through here; it seeds through [`begin_boot_store`] and merges the
/// layers itself, so provenance here is deliberately blank-ish: the value
/// arrives as one opaque layer.
pub fn seed_app_config(config: serde_json::Value) {
    if let Ok(mut state) = get_config().write() {
        let mut store = ConfigStore::runtime();
        store.merge(config, SourceTag::Default);
        state.app_config = Some(store);
    }
}

/// Deep-merge values into the app config from Rust. Used by the daemon's
/// `config.set` RPC so the TUI/CLI write into the SAME store `:lua` and
/// plugins read.
///
/// Returns the top-level location keys the store's policy withheld (empty
/// during the boot phase). One door, one rule: the store decides, and every
/// writer inherits the decision.
pub fn merge_app_config(overlay: serde_json::Value) -> Vec<String> {
    merge_app_config_tagged(overlay, SourceTag::Rpc)
}

/// [`merge_app_config`] with the caller's own provenance tag.
pub fn merge_app_config_tagged(overlay: serde_json::Value, tag: SourceTag) -> Vec<String> {
    match get_config().write() {
        Ok(mut state) => state
            .app_config
            .get_or_insert_with(ConfigStore::runtime)
            .merge(overlay, tag),
        Err(_) => Vec::new(),
    }
}

/// Install a boot-phase store: [`crucible_core::config::LocationPolicy::Accept`],
/// empty. The daemon calls this before it seeds defaults and `config.toml`
/// and evaluates `init.lua`; [`end_boot_phase`] flips it to the runtime rules.
pub fn begin_boot_store() {
    if let Ok(mut state) = get_config().write() {
        state.app_config = Some(ConfigStore::for_load());
    }
}

/// Whether the store is in the boot phase (location keys accepted).
pub fn in_boot_phase() -> bool {
    get_config()
        .read()
        .ok()
        .and_then(|state| {
            state
                .app_config
                .as_ref()
                .map(|store| store.location_policy() == LocationPolicy::Accept)
        })
        .unwrap_or(false)
}

/// A clone of the whole store — the boot path snapshots the seed before the
/// evaluation so a failed evaluation can fall back to it.
pub fn snapshot_store() -> Option<ConfigStore> {
    get_config().read().ok()?.app_config.clone()
}

/// Install a store wholesale — the fail-open half of [`snapshot_store`].
pub fn install_store(store: ConfigStore) {
    if let Ok(mut state) = get_config().write() {
        state.app_config = Some(store);
    }
}

/// A copy of the WHOLE config state — store, theme, layout, geometry,
/// syntax, highlight groups. The boot takes one before it evaluates
/// `init.lua`, so a failed evaluation can be rolled back entirely: the
/// daemon after a broken config is byte-for-byte the daemon with none.
pub fn snapshot_state() -> Option<ConfigState> {
    Some(get_config().read().ok()?.clone())
}

/// The fail-open half of [`snapshot_state`]: reinstall a pre-evaluation
/// copy wholesale.
pub fn install_state(state: ConfigState) {
    if let Ok(mut current) = get_config().write() {
        *current = state;
    }
}

/// End the boot phase: location keys are withheld from every later merge and
/// dropped from the plugin-visible value. The daemon extracts the full config
/// BEFORE calling this.
pub fn end_boot_phase() {
    if let Ok(mut state) = get_config().write() {
        if let Some(store) = state.app_config.as_mut() {
            store.end_boot_phase();
        }
    }
}

/// One throwaway evaluation of a config CHUNK: defaults as layer 0, the
/// chunk in a fresh executor, extract. The verification half of
/// `cru config migrate` — the same store-and-evaluate construction as the
/// daemon's boot, minus the files and the plugin search path.
pub fn evaluate_config_source(source: &str) -> anyhow::Result<crucible_core::config::CliAppConfig> {
    begin_boot_store();
    let defaults = serde_json::to_value(crucible_core::config::CliAppConfig::default())?;
    merge_app_config_tagged(defaults, SourceTag::Default);
    let executor = crate::LuaExecutor::new().map_err(|e| anyhow::anyhow!("executor: {e}"))?;
    executor
        .lua()
        .load(source)
        .exec()
        .map_err(|e| anyhow::anyhow!("the generated Lua does not evaluate: {e}"))?;
    let store = snapshot_store().expect("the store was just seeded");
    let config = store
        .extract()
        .map_err(|e| anyhow::anyhow!("the evaluated config does not extract: {e}"))?;
    end_boot_phase();
    Ok(config)
}

/// The hook the daemon installs so a `runtimepath` write during the boot
/// evaluation extends the module search space INSIDE the `cru.config.set`
/// call — Neovim's invalidate-and-rebuild, before the call returns, so a
/// `require` on the next line finds the new entry.
type RuntimepathExtender = Arc<dyn Fn(&Lua, &[String]) + Send + Sync>;

fn runtimepath_extender_slot() -> &'static RwLock<Option<RuntimepathExtender>> {
    static SLOT: OnceLock<RwLock<Option<RuntimepathExtender>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

/// Install (or clear) the runtimepath extender for the boot phase.
pub fn set_runtimepath_extender(extender: Option<RuntimepathExtender>) {
    if let Ok(mut slot) = runtimepath_extender_slot().write() {
        *slot = extender;
    }
}

/// The full `runtimepath` array the store currently holds, as strings.
fn store_runtimepath() -> Vec<String> {
    get_config()
        .read()
        .ok()
        .and_then(|state| {
            state.app_config.as_ref().map(|store| {
                store
                    .value()
                    .get("runtimepath")
                    .and_then(|v| v.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            })
        })
        .unwrap_or_default()
}

/// The Lua call site of the frame that invoked the current Rust callback,
/// for `file:line` provenance.
fn lua_call_site(lua: &Lua) -> (String, Option<u32>) {
    lua.inspect_stack(1, |debug| {
        let source = debug.source();
        let file = source
            .short_src
            .map(|s| s.into_owned())
            .unwrap_or_else(|| "?".to_string());
        let line = debug.current_line().and_then(|l| u32::try_from(l).ok());
        (file, line)
    })
    .unwrap_or_else(|| ("?".to_string(), None))
}

/// One Lua write into the store: tag with the call site, warn per withheld
/// key, and — during the boot phase — extend the live search space when the
/// write touched `runtimepath`.
fn merge_from_lua(lua: &Lua, overlay: serde_json::Value) {
    let (file, line) = lua_call_site(lua);
    let touched_runtimepath = overlay
        .as_object()
        .is_some_and(|map| map.contains_key("runtimepath"));

    let withheld = merge_app_config_tagged(
        overlay,
        SourceTag::Lua {
            file: file.clone(),
            line,
        },
    );
    for key in &withheld {
        warn!(
            key = %key,
            call_site = %match line {
                Some(line) => format!("{file}:{line}"),
                None => file.clone(),
            },
            "location keys freeze at daemon boot; restart the daemon to change this key"
        );
    }

    if touched_runtimepath && in_boot_phase() {
        let extender = runtimepath_extender_slot()
            .read()
            .ok()
            .and_then(|slot| slot.clone());
        if let Some(extender) = extender {
            extender(lua, &store_runtimepath());
        }
    }
}

/// Register `cru.config.set(table)` and `cru.config.get(key)` on the cru
/// namespace.
///
/// - `set(table)`: DEEP-merges the table into the store — objects merge key
///   by key, arrays and scalars replace, `__replace = true` inside a table
///   replaces that table wholesale. The marker is the ONE replacement
///   mechanism, spelled the same by hand here, in a TOML seed, and over the
///   `config.set` RPC; a `cru.config.replace` sugar existed briefly and was
///   removed as a second spelling of the same thing.
/// - `get(key)`: returns a single top-level value.
///
/// During the daemon's boot phase the store accepts location keys and a
/// `runtimepath` write extends the live module search space before `set`
/// returns. After boot, location keys are withheld and reported here with a
/// warning naming the key and the call site.
pub fn register_app_config_api(lua: &Lua, cru_table: &Table) -> Result<(), LuaError> {
    let config_table = lua.create_table()?;
    let mut ns = crate::host_registry::Ns::over(lua, "cru.config", config_table.clone());

    // cru.config.set(table) — deep-merge into the store.
    //
    // Delegates rather than reimplementing the merge. It used to carry its own
    // copy of the same top-level-insert loop, which made it a second door into
    // one store — and after the location keys started being withheld, only one
    // of the two doors dropped them.
    //
    // The table is not narrowed: it carries whatever keys a config has, plus
    // the `__replace` marker at any depth, so naming fields here would reject
    // correct config. It answers with NOTHING — a withheld location key is
    // reported by a warning, not by a return value.
    ns.func(
        "set",
        "(config: { [string]: any }) -> ()",
        |lua, table: Table| {
            let json_val: serde_json::Value = lua
                .from_value(Value::Table(table))
                .map_err(mlua::Error::external)?;
            merge_from_lua(lua, json_val);
            Ok(())
        },
    )?;

    // cru.config.get(key) — read a single TOP-LEVEL value. The key is one
    // name, not a dotted path, and an unset key reads `nil` rather than
    // raising.
    ns.func("get", "(key: string) -> any", |lua, key: String| {
        let state = get_config()
            .read()
            .map_err(|e| mlua::Error::external(format!("config lock: {e}")))?;

        let val = state
            .app_config
            .as_ref()
            .and_then(|store| store.value().get(&key))
            .cloned();

        match val {
            Some(v) => lua.to_value(&v),
            None => Ok(Value::Nil),
        }
    })?;

    cru_table.set("config", config_table)?;
    Ok(())
}

/// Reset config state (for testing)
#[cfg(test)]
pub fn reset_config() {
    if let Ok(mut state) = get_config().write() {
        *state = ConfigState::default();
    }
}

/// Ensure the `cru.statusline` table exists. The item vocabulary
/// ([`crate::statusline_lua`]) fills it in; this only creates it so both
/// registrations have somewhere to hang.
///
/// Get-or-create, never overwrite: on the daemon VMs
/// [`crate::statusline_exprs::register_statusline_exprs`] may already have put
/// `set`/`clear` on the table, and statusline is ONE module — the factories
/// and the runtime setters share it.
pub fn register_statusline_namespace(lua: &Lua, cru: &Table) -> Result<(), LuaError> {
    if cru.get::<Table>("statusline").is_err() {
        cru.set("statusline", lua.create_table()?)?;
    }
    Ok(())
}

/// Register `cru.colorscheme` — the colour palette.
///
/// Named for what it is. "Theme" had come to mean three different things: this
/// palette, the surface geometry in `cru.geometry`, and the syntect theme used
/// for code highlighting. `colorscheme` is also the word Neovim uses for
/// exactly this — the thing highlight groups resolve against.
pub fn register_theme_namespace(lua: &Lua, cru: &Table) -> Result<(), LuaError> {
    let mut ns = crate::host_registry::Ns::new(lua, "cru.colorscheme")?;

    ns.func(
        "setup",
        "(palette: { [string]: any }) -> ()",
        |lua, config: Table| {
            let theme_config = crate::theme::parse_theme_from_table(lua, &config);
            debug!("Theme config parsed successfully: {}", theme_config.name);
            set_theme_config(theme_config);
            Ok(())
        },
    )?;
    ns.doc(
        "setup",
        "Replace the colour palette. Never raises: a key it does not recognise \
         is ignored and the rest of the table still applies, so a palette \
         written for a newer Crucible still loads on an older one.",
    );

    cru.set("colorscheme", ns.table().clone())?;
    Ok(())
}

/// The roots whose `themes/` subdirectory holds themes, highest priority first.
///
/// The config directory comes first, so a user's own theme wins. Every runtime
/// root follows — `cru setup` copies the shipped tree into
/// `~/.config/crucible/runtime`, which is a runtime root and is NOT the config
/// directory. Reading only the config directory is why a shipped theme was
/// never listed.
///
/// Impure by design: this is the seam that reads `current_exe()`. Tests call
/// [`list_available_themes`] with roots of their own, exactly as
/// `runtime_skill_paths` is split from `default_discovery_paths`.
pub fn theme_roots(config_dir: &Path) -> Vec<PathBuf> {
    let mut roots = vec![config_dir.to_path_buf()];
    roots.extend(crucible_core::runtime_roots::for_current_exe());
    roots
}

/// Theme names across every root's `themes/` subdirectory.
///
/// Sorted, de-duplicated, extension stripped. A name present in more than one
/// root is listed once; `roots` order decides which file
/// [`resolve_theme_file`] then loads.
///
/// Takes the root list rather than one directory. `runtime_roots.rs` was
/// written because four subsystems open-coded their candidate lists and
/// drifted; themes were the fifth and were never wired up, so `cru setup`
/// wrote to a directory no reader looked in.
pub fn list_available_themes(roots: &[PathBuf]) -> Vec<String> {
    let mut names = vec![];
    for root in roots {
        let themes_dir = root.join("themes");
        let Ok(entries) = std::fs::read_dir(&themes_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if crate::source_files::is_lua_source(&path) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// The file a theme name resolves to, or `None` when no root supplies it.
///
/// Both extensions at every root, highest-priority root first. One function so
/// the lister and the loader cannot disagree — `rpc/ui.rs` built its own path
/// and reported "available: default, opencode" for themes it then failed to
/// open.
pub fn resolve_theme_file(roots: &[PathBuf], name: &str) -> Option<PathBuf> {
    roots.iter().find_map(|root| {
        let themes_dir = root.join("themes");
        crate::source_files::SOURCE_EXTENSIONS
            .iter()
            .map(|ext| themes_dir.join(format!("{name}.{ext}")))
            .find(|candidate| candidate.is_file())
    })
}

/// The global config directory: where `init.lua` lives.
///
/// One function, so `cru.paths.config()` and the loader cannot name
/// different directories.
///
/// The directory is the parent of `CliAppConfig::default_config_path()`,
/// which is the only place that reads `$CRUCIBLE_CONFIG_DIR`. A test that
/// points that variable at a temporary directory must also move `init.lua`,
/// or the test writes into the developer's real `~/.config/crucible`.
pub fn default_config_dir() -> PathBuf {
    let config_file = crucible_core::config::CliAppConfig::default_config_path();
    match config_file.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Register the cru.include() function
fn register_include(lua: &Lua, ns: &Table, config_dir: PathBuf) -> Result<(), LuaError> {
    let include_fn = lua.create_function(move |lua, path: String| {
        let full_path = config_dir.join(&path);

        if !full_path.exists() {
            return Err(mlua::Error::RuntimeError(format!(
                "Config file not found: {}",
                full_path.display()
            )));
        }

        let source = std::fs::read_to_string(&full_path).map_err(|e| {
            mlua::Error::RuntimeError(format!("Failed to read {}: {}", full_path.display(), e))
        })?;

        debug!("Including config file: {}", full_path.display());
        lua.load(&source)
            .set_name(full_path.to_string_lossy())
            .exec()
    })?;

    ns.set("include", include_fn)?;
    Ok(())
}

/// Register `cru.syntax` — code-highlighting colours.
///
/// Separate from `cru.colorscheme` because it addresses grammar scopes
/// rather than UI slots, and separate from `cru.geometry` because it is colour,
/// not geometry. `theme = "name"` picks a syntect theme by name; `colors = {}`
/// overrides individual scopes on top of whatever the colorscheme derives.
pub fn register_syntax_namespace(lua: &Lua, cru: &Table) -> Result<(), LuaError> {
    let mut ns = crate::host_registry::Ns::new(lua, "cru.syntax")?;
    ns.func(
        "setup",
        "(config: { [string]: any }) -> ()",
        |lua, config: Table| {
            let json: serde_json::Value = lua
                .from_value(Value::Table(config))
                .map_err(mlua::Error::external)?;
            if let Ok(mut state) = get_config().write() {
                state.syntax = Some(json);
            }
            Ok(())
        },
    )?;
    ns.doc(
        "setup",
        "Store the syntax-highlighting configuration. RAISES when the table \
         holds a value that does not convert to JSON — a function, or a cycle.",
    );
    cru.set("syntax", ns.table().clone())?;
    Ok(())
}

/// Syntax configuration, if a config set any.
pub fn get_syntax_config() -> Option<serde_json::Value> {
    get_config().read().ok()?.syntax.clone()
}

/// Register the UI-config namespaces (`cru.statusline`, `cru.colorscheme`)
/// and seed the embedded defaults into the config store.
///
/// Split out of [`ConfigLoader::load`] because the daemon's **plugin VM** needs
/// the registration half *without* the init.lua half. `ConfigLoader::load` also
/// evaluates `init.lua`, but the plugin runtime evaluates it separately (and
/// deliberately later, after plugins, so user `setup()` calls win over plugin
/// TOML). Calling the whole of `load` there would evaluate `init.lua` twice and
/// double-register every hook in it.
///
/// Without this on the plugin VM, `cru.colorscheme` and `cru.statusline` are
/// **nil in the VM that actually runs the user's init.lua**, so
/// `cru.colorscheme.setup{...}` fails with "attempt to index a nil value" and the
/// user's theme never even parses.
pub fn register_ui_namespaces(lua: &Lua) -> Result<(), LuaError> {
    let cru = crate::lua_util::get_or_create_namespace(lua, "cru")?;

    register_statusline_namespace(lua, &cru)?;
    register_theme_namespace(lua, &cru)?;
    crate::hl_lua::register_hl_namespace(lua, &cru)?;
    crate::ui_geometry::register_geometry_namespace(lua, &cru)?;
    register_syntax_namespace(lua, &cru)?;
    // Item vocabulary hangs off the same `cru.statusline` table the runtime
    // `set`/`clear` functions live on — statusline is one module.
    if let Ok(sl) = cru.get::<Table>("statusline") {
        crate::statusline_lua::register_statusline_items(lua, &sl)?;
    }

    // Embedded defaults, seeded only when nothing is installed yet. User
    // init.lua overrides these via setup(), which runs after this point in
    // every caller.
    //
    // Seeding unconditionally would make *registration* mutate live state:
    // `lua.init_session` re-enters here on a throwaway VM for every `cru chat`,
    // so a theme installed at runtime by `ui.set_theme` would be silently
    // reset to the built-in default for every attached client.
    if get_theme_config().is_none() {
        match crate::theme::load_theme_from_lua(DEFAULT_THEME_LUA) {
            Ok(config) => set_theme_config(config),
            Err(e) => {
                warn!("Failed to load default theme: {}, using Rust defaults", e);
                set_theme_config(ThemeConfig::default_dark());
            }
        }
    }

    if get_layout().is_none() {
        match crate::statusline_lua::default_layout_from_lua() {
            Ok(layout) => set_layout(layout),
            Err(e) => {
                warn!("Failed to load default statusline: {e}, using Rust defaults");
                set_layout(crate::statusline_items::builtin_default());
            }
        }
    }

    Ok(())
}

/// Configuration loader
pub struct ConfigLoader {
    config_dir: PathBuf,
    kiln_config_dir: Option<PathBuf>,
}

impl ConfigLoader {
    /// Create a new config loader
    ///
    /// - `config_dir`: Global config directory (e.g., `~/.config/crucible`)
    /// - `kiln_config_dir`: Optional kiln-specific config (e.g., `<kiln>/.crucible`)
    pub fn new(config_dir: impl Into<PathBuf>, kiln_config_dir: Option<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
            kiln_config_dir,
        }
    }

    /// Create a loader using default XDG paths
    pub fn with_defaults(kiln_path: Option<&Path>) -> Self {
        let kiln_config_dir = kiln_path.map(|p| p.join(".crucible"));

        Self::new(default_config_dir(), kiln_config_dir)
    }

    /// Load configuration into a Lua state
    ///
    /// This:
    /// 1. Registers cru.* modules
    /// 2. Loads init.lua from config_dir (if exists)
    /// 3. Loads init.lua from kiln_config_dir (if exists, as override)
    pub fn load(&self, lua: &Lua) -> Result<(), LuaError> {
        register_ui_namespaces(lua)?;

        // Register cru.include()
        let cru = crate::lua_util::get_or_create_namespace(lua, "cru")?;
        register_include(lua, &cru, self.config_dir.clone())?;

        // Load global init.lua
        let global_init = self.config_dir.join("init.lua");
        if global_init.exists() {
            info!("Loading config from {}", global_init.display());
            let source = std::fs::read_to_string(&global_init)?;
            lua.load(&source)
                .set_name(global_init.to_string_lossy())
                .exec()?;
        } else {
            debug!("No global init.lua found at {}", global_init.display());
        }

        // Load kiln-specific init.lua (overrides global)
        if let Some(ref kiln_dir) = self.kiln_config_dir {
            let kiln_init = kiln_dir.join("init.lua");
            if kiln_init.exists() {
                info!("Loading kiln config from {}", kiln_init.display());
                let source = std::fs::read_to_string(&kiln_init)?;
                lua.load(&source)
                    .set_name(kiln_init.to_string_lossy())
                    .exec()?;
            }
        }

        Ok(())
    }

    /// Get the global config directory path
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::LOCATION_CONFIG_KEYS;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Serialize tests that touch the global CONFIG to avoid race conditions
    static CONFIG_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn create_test_lua() -> Lua {
        let lua = Lua::new();
        // Set up a minimal cru table (normally done by executor)
        let cru = lua.create_table().unwrap();
        lua.globals().set("cru", cru).unwrap();
        lua
    }

    /// `lua.init_session` builds a throwaway executor and calls `load_config`,
    /// which re-enters `register_ui_namespaces`. Seeding the embedded default
    /// unconditionally there would wipe a theme installed at runtime by
    /// `ui.set_theme` — every `cru chat` would snap the palette back to default
    /// for every attached client.
    #[test]
    fn registering_namespaces_again_keeps_a_theme_set_at_runtime() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        let lua = create_test_lua();
        register_ui_namespaces(&lua).unwrap();

        let mut chosen = ThemeConfig::default_dark();
        chosen.name = "chosen-at-runtime".to_string();
        set_theme_config_public(chosen);

        // A second client connects; its executor re-registers on a fresh VM.
        let other = create_test_lua();
        register_ui_namespaces(&other).unwrap();

        assert_eq!(
            get_theme_config().map(|t| t.name),
            Some("chosen-at-runtime".to_string()),
            "re-registering namespaces reset the active theme to the built-in default"
        );
    }

    #[test]
    fn test_statusline_setup() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        let lua = create_test_lua();
        register_ui_namespaces(&lua).unwrap();

        lua.load(
            r#"
            local sl = cru.statusline
            sl.setup({
                prompt = {
                    sl.input,
                    { sl.mode, " ", sl.model{ max = 20 }, sl.align, sl.context },
                },
            })
        "#,
        )
        .exec()
        .unwrap();

        use crate::statusline_items::Element;
        let layout = get_layout().expect("setup defines a layout");
        assert_eq!(layout.prompt[0], Element::Input);
        let Element::Row(main) = &layout.prompt[1] else {
            panic!("the second element is a row: {:?}", layout.prompt[1]);
        };
        assert_eq!(main.len(), 5, "each entry is one item: {main:?}");
        assert_eq!(main[0], crate::statusline_items::StatusItem::Mode);
        assert_eq!(main[3], crate::statusline_items::StatusItem::Align);
    }

    #[test]
    fn test_include() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().to_path_buf();

        // Create a file to include
        std::fs::write(config_dir.join("extra.lua"), "cru.included = true").unwrap();

        let lua = create_test_lua();
        let cru: Table = lua.globals().get("cru").unwrap();
        register_include(&lua, &cru, config_dir).unwrap();

        lua.load(r#"cru.include("extra.lua")"#).exec().unwrap();

        let included: bool = lua.load("return cru.included").eval().unwrap();
        assert!(included);
    }

    #[test]
    fn test_include_missing_file() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().to_path_buf();

        let lua = create_test_lua();
        let cru: Table = lua.globals().get("cru").unwrap();
        register_include(&lua, &cru, config_dir).unwrap();

        let result = lua.load(r#"cru.include("nonexistent.lua")"#).exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_loader_no_init() {
        let tmp = TempDir::new().unwrap();
        let loader = ConfigLoader::new(tmp.path(), None);

        let lua = create_test_lua();
        // Should not error even without init.lua
        loader.load(&lua).unwrap();
    }

    #[test]
    fn test_config_loader_with_init() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("init.lua"),
            r#"
            cru.statusline.setup({
                prompt = { cru.statusline.input, { cru.statusline.mode } },
            })
        "#,
        )
        .unwrap();

        let loader = ConfigLoader::new(tmp.path(), None);
        let lua = create_test_lua();
        loader.load(&lua).unwrap();

        // init.lua reached the store: the authored layout replaced the default.
        use crate::statusline_items::Element;
        let layout = get_layout().expect("init.lua defines a layout");
        assert_eq!(
            layout.prompt,
            vec![
                Element::Input,
                Element::Row(vec![crate::statusline_items::StatusItem::Mode]),
            ]
        );
    }

    #[test]
    fn test_theme_pipeline_default() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        let tmp = TempDir::new().unwrap();
        let loader = ConfigLoader::new(tmp.path(), None);
        let lua = create_test_lua();
        loader.load(&lua).unwrap();

        let config = get_theme_config();
        assert!(config.is_some(), "theme config should be set after load");
        let config = config.unwrap();
        assert_eq!(config.name, "default");
        assert!(config.is_dark);
    }

    #[test]
    fn test_theme_setup_via_lua() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        let lua = create_test_lua();
        let cru: Table = lua.globals().get("cru").unwrap();
        register_theme_namespace(&lua, &cru).unwrap();

        lua.load(
            r##"
            cru.colorscheme.setup({
                colors = { error = "#ff0000" },
                name = "custom",
            })
        "##,
        )
        .exec()
        .unwrap();

        let config = get_theme_config();
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.name, "custom");
        // Error color should be overridden to red
        use crucible_oil::style::{AdaptiveColor, Color};
        assert_eq!(
            config.colors.error,
            AdaptiveColor::from_single(Color::Rgb(255, 0, 0))
        );
    }

    #[test]
    fn test_list_available_themes() {
        let tmp = TempDir::new().unwrap();
        let themes_dir = tmp.path().join("themes");
        std::fs::create_dir_all(&themes_dir).unwrap();
        std::fs::write(themes_dir.join("dark.lua"), "return {}").unwrap();
        std::fs::write(themes_dir.join("light.lua"), "return {}").unwrap();
        std::fs::write(themes_dir.join("not_a_theme.txt"), "").unwrap();

        let themes = list_available_themes(&[tmp.path().to_path_buf()]);
        assert_eq!(themes, vec!["dark".to_string(), "light".to_string()]);
    }

    /// A theme `cru setup` copied is listed.
    ///
    /// `cru setup` writes the shipped tree to `~/.config/crucible/runtime`,
    /// which is a runtime root and not the config directory. While this
    /// function took one `config_dir`, no shipped theme was ever listed.
    #[test]
    fn themes_are_listed_from_every_root() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join("crucible");
        let runtime_dir = config_dir.join("runtime");
        std::fs::create_dir_all(config_dir.join("themes")).unwrap();
        std::fs::create_dir_all(runtime_dir.join("themes")).unwrap();
        std::fs::write(config_dir.join("themes").join("mine.luau"), "return {}").unwrap();
        std::fs::write(runtime_dir.join("themes").join("shipped.luau"), "return {}").unwrap();

        let themes = list_available_themes(&[config_dir.clone(), runtime_dir.clone()]);
        assert_eq!(
            themes,
            vec!["mine".to_string(), "shipped".to_string()],
            "a theme under a runtime root must be listed beside the user's own"
        );
    }

    /// The same name in two roots is listed once, and the first root wins.
    #[test]
    fn a_user_theme_shadows_a_shipped_one_of_the_same_name() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join("crucible");
        let runtime_dir = config_dir.join("runtime");
        std::fs::create_dir_all(config_dir.join("themes")).unwrap();
        std::fs::create_dir_all(runtime_dir.join("themes")).unwrap();
        std::fs::write(config_dir.join("themes").join("default.luau"), "-- mine").unwrap();
        std::fs::write(
            runtime_dir.join("themes").join("default.luau"),
            "-- shipped",
        )
        .unwrap();

        let roots = [config_dir.clone(), runtime_dir.clone()];
        assert_eq!(list_available_themes(&roots), vec!["default".to_string()]);
        assert_eq!(
            resolve_theme_file(&roots, "default"),
            Some(config_dir.join("themes").join("default.luau")),
            "the config directory outranks a runtime root"
        );
    }

    /// Both extensions resolve, and an absent name resolves to nothing.
    #[test]
    fn resolve_theme_file_takes_either_extension() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("themes")).unwrap();
        std::fs::write(tmp.path().join("themes").join("old.lua"), "return {}").unwrap();

        let roots = [tmp.path().to_path_buf()];
        assert_eq!(
            resolve_theme_file(&roots, "old"),
            Some(tmp.path().join("themes").join("old.lua"))
        );
        assert_eq!(resolve_theme_file(&roots, "absent"), None);
    }

    #[test]
    fn test_app_config_seed_then_lua_override() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        // Simulate TOML seeding
        seed_app_config(serde_json::json!({
            "syntax_theme": "dracula",
            "timeout": 30,
            "llm": { "provider": "ollama" }
        }));

        // Simulate Lua override via cru.config.set()
        let lua = create_test_lua();
        let cru = lua.create_table().unwrap();
        register_app_config_api(&lua, &cru).unwrap();
        lua.globals().set("cru", cru).unwrap();

        // Lua overrides timeout but keeps syntax_theme
        lua.load(r#"cru.config.set({ timeout = 60, new_field = "from_lua" })"#)
            .exec()
            .unwrap();

        let config = get_app_config().unwrap();
        assert_eq!(config["syntax_theme"], "dracula"); // TOML preserved
        assert_eq!(config["timeout"], 60); // Lua overrode
        assert_eq!(config["new_field"], "from_lua"); // Lua added
        assert_eq!(config["llm"]["provider"], "ollama"); // Nested TOML preserved
    }

    #[test]
    fn test_app_config_lua_get() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        seed_app_config(serde_json::json!({
            "syntax_theme": "dracula",
            "count": 42
        }));

        let lua = create_test_lua();
        let cru = lua.create_table().unwrap();
        register_app_config_api(&lua, &cru).unwrap();
        lua.globals().set("cru", cru).unwrap();

        let result: String = lua
            .load(r#"return cru.config.get("syntax_theme")"#)
            .eval()
            .unwrap();
        assert_eq!(result, "dracula");

        let count: i64 = lua
            .load(r#"return cru.config.get("count")"#)
            .eval()
            .unwrap();
        assert_eq!(count, 42);

        // Missing key returns nil
        let missing: Value = lua
            .load(r#"return cru.config.get("nonexistent")"#)
            .eval()
            .unwrap();
        assert!(matches!(missing, Value::Nil));
    }

    /// The store is the plugin-visible view of the config, so the keys that
    /// name directories never enter it — including through the daemon's own
    /// seed, which is where the user's real kiln paths would otherwise arrive.
    ///
    /// Asserted key by key over [`LOCATION_CONFIG_KEYS`] rather than over a
    /// hand-written list, so a key added to the shape is covered here without
    /// anyone remembering to come back.
    #[test]
    fn seeding_withholds_the_keys_that_name_where_crucible_acts() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        let mut seed = serde_json::Map::new();
        for key in LOCATION_CONFIG_KEYS {
            seed.insert(key.to_string(), serde_json::json!("/home/user/private"));
        }
        seed.insert("timeout".to_string(), serde_json::json!(30));
        seed_app_config(serde_json::Value::Object(seed));

        let config = get_app_config().expect("seeded");
        for key in LOCATION_CONFIG_KEYS {
            assert!(
                config.get(key).is_none(),
                "{key} names a directory and must not reach a plugin"
            );
        }
        // The seed still happened — otherwise the assertions above would hold
        // for a store that is simply empty.
        assert_eq!(config["timeout"], 30);
    }

    /// Same shape on the merge path, which is where the `config.set` RPC lands.
    #[test]
    fn merging_withholds_the_keys_that_name_where_crucible_acts() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        seed_app_config(serde_json::json!({ "timeout": 30 }));
        let mut overlay = serde_json::Map::new();
        for key in LOCATION_CONFIG_KEYS {
            overlay.insert(key.to_string(), serde_json::json!("/home/user/private"));
        }
        overlay.insert("timeout".to_string(), serde_json::json!(60));
        merge_app_config(serde_json::Value::Object(overlay));

        let config = get_app_config().expect("seeded");
        for key in LOCATION_CONFIG_KEYS {
            assert!(
                config.get(key).is_none(),
                "{key} names a directory and must not reach a plugin"
            );
        }
        assert_eq!(config["timeout"], 60);
    }

    #[test]
    fn test_app_config_set_without_seed() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        // No TOML seed — pure Lua config
        let lua = create_test_lua();
        let cru = lua.create_table().unwrap();
        register_app_config_api(&lua, &cru).unwrap();
        lua.globals().set("cru", cru).unwrap();

        lua.load(r#"cru.config.set({ syntax_theme = "dracula", kiln_path = "~/notes" })"#)
            .exec()
            .unwrap();

        let config = get_app_config().unwrap();
        assert_eq!(config["syntax_theme"], "dracula");
        // `cru.config.set` merges through the same function the `config.set`
        // RPC does, so the keys that name a place are withheld on this door
        // too. Two doors into one store is how one of them ends up without the
        // rule.
        assert!(config.get("kiln_path").is_none());
    }

    #[test]
    fn test_theme_fallback_corrupted() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        let lua = create_test_lua();
        let cru: Table = lua.globals().get("cru").unwrap();
        register_theme_namespace(&lua, &cru).unwrap();

        // Setup with an invalid color — should not panic, should use default for that field
        lua.load(
            r#"
            cru.colorscheme.setup({
                colors = { error = "not_a_valid_color_xyz" },
            })
        "#,
        )
        .exec()
        .unwrap();

        let config = get_theme_config();
        assert!(config.is_some());
        let config = config.unwrap();
        // Invalid color falls back to default
        let default = crate::theme::ThemeConfig::default_dark();
        assert_eq!(config.colors.error, default.colors.error);
    }
}
