//! Lua configuration loader
//!
//! Loads `init.lua` from the config directory and provides the `crucible.include()` function.
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
//! -- Built-in modules are under crucible.*
//! crucible.statusline.setup({
//!     left = { crucible.statusline.mode() },
//!     center = { crucible.statusline.model() },
//!     right = { crucible.statusline.context() },
//! })
//!
//! -- Include other config files
//! crucible.include("keymaps.lua")  -- loads ~/.config/crucible/keymaps.lua
//! ```

use crate::error::LuaError;
use crate::theme::ThemeConfig;
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

const DEFAULT_THEME_LUA: &str = include_str!("../../../runtime/themes/default.lua");

/// Global config state - stores parsed configuration from Lua
#[derive(Debug, Default)]
pub struct ConfigState {
    pub theme: Option<ThemeConfig>,
    /// Highlight groups authored via `crucible.hl.set/link`. Open namespace —
    /// plugins name their own — so it is a map, not a fixed struct.
    pub hl: crate::hl::HlRegistry,
    /// Per-surface geometry from `crucible.ui.setup{}`.
    pub ui: Option<crate::ui_geometry::UiGeometry>,
    /// Statusline bars authored as item trees.
    pub bars: Option<crate::statusline_items::StatusBars>,
    /// Daemon/app config values set via cru.config.set() or seeded from TOML.
    /// Stored as JSON for easy extraction by Rust callers.
    pub app_config: Option<serde_json::Value>,
}

/// Thread-safe config registry
static CONFIG: std::sync::OnceLock<Arc<RwLock<ConfigState>>> = std::sync::OnceLock::new();

fn get_config() -> &'static Arc<RwLock<ConfigState>> {
    CONFIG.get_or_init(|| Arc::new(RwLock::new(ConfigState::default())))
}

/// Get the current theme configuration (if set via crucible.theme.setup())
pub fn get_theme_config() -> Option<ThemeConfig> {
    get_config().read().ok()?.theme.clone()
}

/// Set the theme configuration
fn set_theme_config(config: ThemeConfig) {
    if let Ok(mut state) = get_config().write() {
        state.theme = Some(config);
    }
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

/// Statusline bars, if a config defined any.
pub fn get_status_bars() -> Option<crate::statusline_items::StatusBars> {
    get_config().read().ok()?.bars.clone()
}

/// Store statusline bars.
pub fn set_status_bars(bars: crate::statusline_items::StatusBars) {
    if let Ok(mut state) = get_config().write() {
        state.bars = Some(bars);
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

/// Get the app config (set via `cru.config.set()` or seeded from TOML).
pub fn get_app_config() -> Option<serde_json::Value> {
    get_config().read().ok()?.app_config.clone()
}

/// Seed app config from TOML values (called before Lua init.lua runs).
/// Lua's `cru.config.set()` can then override individual fields.
pub fn seed_app_config(config: serde_json::Value) {
    if let Ok(mut state) = get_config().write() {
        state.app_config = Some(config);
    }
}

/// Merge values into app_config from Rust (same top-level-override semantics
/// as Lua's `cru.config.set`). Used by the daemon's `config.set` RPC so the
/// TUI/CLI write into the SAME store `:lua` and plugins read.
pub fn merge_app_config(overlay: serde_json::Value) {
    if let Ok(mut state) = get_config().write() {
        match &mut state.app_config {
            Some(serde_json::Value::Object(base)) => {
                if let serde_json::Value::Object(overlay) = overlay {
                    for (k, v) in overlay {
                        base.insert(k, v);
                    }
                }
            }
            _ => {
                state.app_config = Some(overlay);
            }
        }
    }
}

/// Register `cru.config.set(table)` and `cru.config.get(key)` on the cru namespace.
///
/// - `set(table)`: Deep-merges the table into app_config (TOML values as base, Lua overrides)
/// - `get(key)`: Returns a single top-level value from app_config
///
/// This is the bridge between TOML and Lua config. TOML seeds values first,
/// then Lua's init.lua can override any field via `cru.config.set()`.
pub fn register_app_config_api(lua: &Lua, cru_table: &Table) -> Result<(), LuaError> {
    let config_table = lua.create_table()?;

    // cru.config.set(table) — merge into app_config
    let set_fn = lua.create_function(|lua, table: Table| {
        let json_val: serde_json::Value = lua
            .from_value(Value::Table(table))
            .map_err(mlua::Error::external)?;

        let mut state = get_config()
            .write()
            .map_err(|e| mlua::Error::external(format!("config lock: {e}")))?;

        match &mut state.app_config {
            Some(existing) => {
                // Deep merge: Lua values override TOML values
                if let (serde_json::Value::Object(base), serde_json::Value::Object(overlay)) =
                    (existing, &json_val)
                {
                    for (k, v) in overlay {
                        base.insert(k.clone(), v.clone());
                    }
                }
            }
            None => {
                state.app_config = Some(json_val);
            }
        }
        Ok(())
    })?;
    config_table.set("set", set_fn)?;

    // cru.config.get(key) — read a single top-level value
    let get_fn = lua.create_function(|lua, key: String| {
        let state = get_config()
            .read()
            .map_err(|e| mlua::Error::external(format!("config lock: {e}")))?;

        let val = state.app_config.as_ref().and_then(|c| c.get(&key)).cloned();

        match val {
            Some(v) => lua.to_value(&v),
            None => Ok(Value::Nil),
        }
    })?;
    config_table.set("get", get_fn)?;

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

/// Register the `crucible.statusline` table. The item vocabulary
/// ([`crate::statusline_lua`]) fills it in; this only creates it so both
/// registrations have somewhere to hang.
pub fn register_statusline_namespace(lua: &Lua, crucible: &Table) -> Result<(), LuaError> {
    crucible.set("statusline", lua.create_table()?)?;
    Ok(())
}

/// Register the crucible.theme module with setup() support
pub fn register_theme_namespace(lua: &Lua, crucible: &Table) -> Result<(), LuaError> {
    let theme = lua.create_table()?;

    // crucible.theme.setup(config) — parses and stores the theme config
    let setup_fn = lua.create_function(|lua, config: Table| {
        let theme_config = crate::theme::parse_theme_from_table(lua, &config);
        debug!("Theme config parsed successfully: {}", theme_config.name);
        set_theme_config(theme_config);
        Ok(())
    })?;
    theme.set("setup", setup_fn)?;

    crucible.set("theme", theme)?;
    Ok(())
}

/// List available theme names from a config directory's `themes/` subdirectory.
///
/// Returns sorted theme names (without `.lua` extension) discovered in
/// `config_dir/themes/*.lua`.
pub fn list_available_themes(config_dir: &Path) -> Vec<String> {
    let themes_dir = config_dir.join("themes");
    if !themes_dir.exists() {
        return vec![];
    }
    let mut names = vec![];
    if let Ok(entries) = std::fs::read_dir(&themes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("lua") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    names.sort();
    names
}

/// Register the crucible.include() function
fn register_include(lua: &Lua, crucible: &Table, config_dir: PathBuf) -> Result<(), LuaError> {
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

    crucible.set("include", include_fn)?;
    Ok(())
}

/// Register the UI-config namespaces (`crucible.statusline`, `crucible.theme`)
/// and seed the embedded defaults into the config store.
///
/// Split out of [`ConfigLoader::load`] because the daemon's **plugin VM** needs
/// the registration half *without* the init.lua half. `ConfigLoader::load` also
/// evaluates `init.lua`, but the plugin runtime evaluates it separately (and
/// deliberately later, after plugins, so user `setup()` calls win over plugin
/// TOML). Calling the whole of `load` there would evaluate `init.lua` twice and
/// double-register every hook in it.
///
/// Without this on the plugin VM, `crucible.theme` and `crucible.statusline` are
/// **nil in the VM that actually runs the user's init.lua**, so
/// `crucible.theme.setup{...}` fails with "attempt to index a nil value" and the
/// user's theme never even parses.
pub fn register_ui_namespaces(lua: &Lua) -> Result<(), LuaError> {
    let globals = lua.globals();
    // The plugin VM has a `crucible` table already; a bare VM may not.
    let crucible: Table = match globals.get::<Table>("crucible") {
        Ok(t) => t,
        Err(_) => {
            let t = lua.create_table()?;
            globals.set("crucible", t.clone())?;
            t
        }
    };

    register_statusline_namespace(lua, &crucible)?;
    register_theme_namespace(lua, &crucible)?;
    crate::hl_lua::register_hl_namespace(lua, &crucible)?;
    crate::ui_geometry::register_ui_namespace(lua, &crucible)?;
    // Item vocabulary hangs off the same `crucible.statusline` table the
    // component factories already live on.
    if let Ok(sl) = crucible.get::<Table>("statusline") {
        crate::statusline_lua::register_statusline_items(lua, &sl)?;
    }

    // Embedded defaults. User init.lua overrides these via setup(), which runs
    // after this point in every caller.
    match crate::theme::load_theme_from_lua(DEFAULT_THEME_LUA) {
        Ok(config) => set_theme_config(config),
        Err(e) => {
            warn!("Failed to load default theme: {}, using Rust defaults", e);
            set_theme_config(ThemeConfig::default_dark());
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
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("crucible");

        let kiln_config_dir = kiln_path.map(|p| p.join(".crucible"));

        Self::new(config_dir, kiln_config_dir)
    }

    /// Load configuration into a Lua state
    ///
    /// This:
    /// 1. Registers crucible.* modules
    /// 2. Loads init.lua from config_dir (if exists)
    /// 3. Loads init.lua from kiln_config_dir (if exists, as override)
    pub fn load(&self, lua: &Lua) -> Result<(), LuaError> {
        register_ui_namespaces(lua)?;

        let globals = lua.globals();
        let crucible: Table = globals.get("crucible")?;

        // Register crucible.include()
        register_include(lua, &crucible, self.config_dir.clone())?;

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
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Serialize tests that touch the global CONFIG to avoid race conditions
    static CONFIG_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn create_test_lua() -> Lua {
        let lua = Lua::new();
        // Set up minimal crucible table (normally done by executor)
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        lua
    }

    #[test]
    fn test_statusline_setup() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        let lua = create_test_lua();
        register_ui_namespaces(&lua).unwrap();

        lua.load(
            r#"
            local sl = crucible.statusline
            sl.setup({
                main = {
                    anchor = "footer.below_input",
                    items = { sl.mode, " ", sl.model{ max = 20 }, sl.align, sl.context },
                },
            })
        "#,
        )
        .exec()
        .unwrap();

        let bars = get_status_bars().expect("setup defines bars");
        let main = &bars.get("main").expect("named bar").items;
        assert_eq!(main.len(), 5, "each entry is one item: {main:?}");
        assert_eq!(main[0], crate::statusline_items::StatusItem::Mode);
        assert_eq!(main[3], crate::statusline_items::StatusItem::Align);
    }

    #[test]
    fn test_include() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().to_path_buf();

        // Create a file to include
        std::fs::write(config_dir.join("extra.lua"), "crucible.included = true").unwrap();

        let lua = create_test_lua();
        let crucible: Table = lua.globals().get("crucible").unwrap();
        register_include(&lua, &crucible, config_dir).unwrap();

        lua.load(r#"crucible.include("extra.lua")"#).exec().unwrap();

        let included: bool = lua.load("return crucible.included").eval().unwrap();
        assert!(included);
    }

    #[test]
    fn test_include_missing_file() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().to_path_buf();

        let lua = create_test_lua();
        let crucible: Table = lua.globals().get("crucible").unwrap();
        register_include(&lua, &crucible, config_dir).unwrap();

        let result = lua.load(r#"crucible.include("nonexistent.lua")"#).exec();
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
            crucible.statusline.setup({
                main = { items = { crucible.statusline.mode } },
            })
        "#,
        )
        .unwrap();

        let loader = ConfigLoader::new(tmp.path(), None);
        let lua = create_test_lua();
        loader.load(&lua).unwrap();

        // The pre-item `left/center/right` shape is migrated to an item bar
        // rather than dropped, so this asserts on bars, not on the superseded
        // component-table config.
        let bars = get_status_bars().expect("legacy setup shape defines a bar");
        let main = bars
            .get("main")
            .expect("legacy sections become the main bar");
        assert_eq!(
            main.items,
            vec![crate::statusline_items::StatusItem::Mode],
            "a left-only config needs no alignment split"
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
        let crucible: Table = lua.globals().get("crucible").unwrap();
        register_theme_namespace(&lua, &crucible).unwrap();

        lua.load(
            r##"
            crucible.theme.setup({
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

        let themes = list_available_themes(tmp.path());
        assert_eq!(themes, vec!["dark".to_string(), "light".to_string()]);
    }

    #[test]
    fn test_app_config_seed_then_lua_override() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        // Simulate TOML seeding
        seed_app_config(serde_json::json!({
            "kiln_path": "/home/user/vault",
            "timeout": 30,
            "llm": { "provider": "ollama" }
        }));

        // Simulate Lua override via cru.config.set()
        let lua = create_test_lua();
        let cru = lua.create_table().unwrap();
        register_app_config_api(&lua, &cru).unwrap();
        lua.globals().set("cru", cru).unwrap();

        // Lua overrides timeout but keeps kiln_path
        lua.load(r#"cru.config.set({ timeout = 60, new_field = "from_lua" })"#)
            .exec()
            .unwrap();

        let config = get_app_config().unwrap();
        assert_eq!(config["kiln_path"], "/home/user/vault"); // TOML preserved
        assert_eq!(config["timeout"], 60); // Lua overrode
        assert_eq!(config["new_field"], "from_lua"); // Lua added
        assert_eq!(config["llm"]["provider"], "ollama"); // Nested TOML preserved
    }

    #[test]
    fn test_app_config_lua_get() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        seed_app_config(serde_json::json!({
            "kiln_path": "/vault",
            "count": 42
        }));

        let lua = create_test_lua();
        let cru = lua.create_table().unwrap();
        register_app_config_api(&lua, &cru).unwrap();
        lua.globals().set("cru", cru).unwrap();

        let result: String = lua
            .load(r#"return cru.config.get("kiln_path")"#)
            .eval()
            .unwrap();
        assert_eq!(result, "/vault");

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

    #[test]
    fn test_app_config_set_without_seed() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        // No TOML seed — pure Lua config
        let lua = create_test_lua();
        let cru = lua.create_table().unwrap();
        register_app_config_api(&lua, &cru).unwrap();
        lua.globals().set("cru", cru).unwrap();

        lua.load(r#"cru.config.set({ kiln_path = "~/notes" })"#)
            .exec()
            .unwrap();

        let config = get_app_config().unwrap();
        assert_eq!(config["kiln_path"], "~/notes");
    }

    #[test]
    fn test_theme_fallback_corrupted() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        reset_config();

        let lua = create_test_lua();
        let crucible: Table = lua.globals().get("crucible").unwrap();
        register_theme_namespace(&lua, &crucible).unwrap();

        // Setup with an invalid color — should not panic, should use default for that field
        lua.load(
            r#"
            crucible.theme.setup({
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
