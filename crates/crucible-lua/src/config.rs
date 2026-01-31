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
use crate::statusline::{parse_statusline_config, StatuslineConfig};
use mlua::{Lua, Table, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

const DEFAULT_STATUSLINE_LUA: &str = include_str!("defaults/statusline.lua");

/// Global config state - stores parsed configuration from Lua
#[derive(Debug, Default)]
pub struct ConfigState {
    pub statusline: Option<StatuslineConfig>,
}

/// Thread-safe config registry
static CONFIG: std::sync::OnceLock<Arc<RwLock<ConfigState>>> = std::sync::OnceLock::new();

fn get_config() -> &'static Arc<RwLock<ConfigState>> {
    CONFIG.get_or_init(|| Arc::new(RwLock::new(ConfigState::default())))
}

/// Get the current statusline configuration (if set)
pub fn get_statusline_config() -> Option<StatuslineConfig> {
    get_config().read().ok()?.statusline.clone()
}

/// Set the statusline configuration
fn set_statusline_config(config: StatuslineConfig) {
    if let Ok(mut state) = get_config().write() {
        state.statusline = Some(config);
    }
}

/// Reset config state (for testing)
#[cfg(test)]
pub fn reset_config() {
    if let Ok(mut state) = get_config().write() {
        *state = ConfigState::default();
    }
}

/// Register the crucible.statusline module with setup() support
pub fn register_statusline_namespace(lua: &Lua, crucible: &Table) -> Result<(), LuaError> {
    let statusline = lua.create_table()?;

    // Component factory functions (same as before, but under crucible.statusline)

    // crucible.statusline.mode({ normal = {...}, plan = {...}, auto = {...} })
    let mode_fn = lua.create_function(|lua, config: Option<Table>| {
        let component = lua.create_table()?;
        component.set("type", "mode")?;
        if let Some(cfg) = config {
            if let Ok(v) = cfg.get::<Table>("normal") {
                component.set("normal", v)?;
            }
            if let Ok(v) = cfg.get::<Table>("plan") {
                component.set("plan", v)?;
            }
            if let Ok(v) = cfg.get::<Table>("auto") {
                component.set("auto", v)?;
            }
        } else {
            // Default mode styles
            let normal = lua.create_table()?;
            normal.set("text", " NORMAL ")?;
            normal.set("bg", "green")?;
            normal.set("fg", "black")?;
            component.set("normal", normal)?;

            let plan = lua.create_table()?;
            plan.set("text", " PLAN ")?;
            plan.set("bg", "blue")?;
            plan.set("fg", "black")?;
            component.set("plan", plan)?;

            let auto = lua.create_table()?;
            auto.set("text", " AUTO ")?;
            auto.set("bg", "yellow")?;
            auto.set("fg", "black")?;
            component.set("auto", auto)?;
        }
        Ok(component)
    })?;
    statusline.set("mode", mode_fn)?;

    // crucible.statusline.model({ max_length = 20, fallback = "...", fg = "cyan" })
    let model_fn = lua.create_function(|lua, config: Option<Table>| {
        let component = lua.create_table()?;
        component.set("type", "model")?;
        if let Some(cfg) = config {
            if let Ok(v) = cfg.get::<Value>("max_length") {
                component.set("max_length", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("fallback") {
                component.set("fallback", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("fg") {
                component.set("fg", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("bg") {
                component.set("bg", v)?;
            }
        }
        Ok(component)
    })?;
    statusline.set("model", model_fn)?;

    // crucible.statusline.context({ format = "{percent}% ctx", fg = "gray" })
    let context_fn = lua.create_function(|lua, config: Option<Table>| {
        let component = lua.create_table()?;
        component.set("type", "context")?;
        if let Some(cfg) = config {
            if let Ok(v) = cfg.get::<Value>("format") {
                component.set("format", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("fg") {
                component.set("fg", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("bg") {
                component.set("bg", v)?;
            }
        }
        Ok(component)
    })?;
    statusline.set("context", context_fn)?;

    // crucible.statusline.text("content", { fg = "white" })
    let text_fn = lua.create_function(|lua, (content, style): (String, Option<Table>)| {
        let component = lua.create_table()?;
        component.set("type", "text")?;
        component.set("content", content)?;
        if let Some(s) = style {
            if let Ok(v) = s.get::<Value>("fg") {
                component.set("fg", v)?;
            }
            if let Ok(v) = s.get::<Value>("bg") {
                component.set("bg", v)?;
            }
            if let Ok(v) = s.get::<Value>("bold") {
                component.set("bold", v)?;
            }
        }
        Ok(component)
    })?;
    statusline.set("text", text_fn)?;

    // crucible.statusline.spacer()
    let spacer_fn = lua.create_function(|lua, ()| {
        let component = lua.create_table()?;
        component.set("type", "spacer")?;
        Ok(component)
    })?;
    statusline.set("spacer", spacer_fn)?;

    // crucible.statusline.notification({ fg = "yellow", fallback = crucible.statusline.context() })
    let notification_fn = lua.create_function(|lua, config: Option<Table>| {
        let component = lua.create_table()?;
        component.set("type", "notification")?;
        if let Some(cfg) = config {
            if let Ok(v) = cfg.get::<Value>("fg") {
                component.set("fg", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("bg") {
                component.set("bg", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("fallback") {
                component.set("fallback", v)?;
            }
        }
        Ok(component)
    })?;
    statusline.set("notification", notification_fn)?;

    // crucible.statusline.setup(config) - parses and stores the config
    let setup_fn =
        lua.create_function(
            |_lua, config: Table| match parse_statusline_config(&config) {
                Ok(parsed) => {
                    debug!("Statusline config parsed successfully");
                    set_statusline_config(parsed);
                    Ok(())
                }
                Err(e) => Err(mlua::Error::RuntimeError(format!(
                    "Invalid statusline config: {}",
                    e
                ))),
            },
        )?;
    statusline.set("setup", setup_fn)?;

    crucible.set("statusline", statusline)?;
    Ok(())
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
        // Get or create the crucible table
        let globals = lua.globals();
        let crucible: Table = globals.get("crucible")?;

        // Register crucible.statusline
        register_statusline_namespace(lua, &crucible)?;

        // Load embedded default statusline (user init.lua can override via setup())
        if let Err(e) = lua
            .load(DEFAULT_STATUSLINE_LUA)
            .set_name("[builtin] statusline.lua")
            .exec()
        {
            warn!("Failed to load default statusline: {}", e);
        }

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
    use tempfile::TempDir;

    fn create_test_lua() -> Lua {
        let lua = Lua::new();
        // Set up minimal crucible table (normally done by executor)
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        lua
    }

    #[test]
    fn test_statusline_setup() {
        reset_config();

        let lua = create_test_lua();
        let crucible: Table = lua.globals().get("crucible").unwrap();
        register_statusline_namespace(&lua, &crucible).unwrap();

        lua.load(
            r#"
            crucible.statusline.setup({
                left = { crucible.statusline.mode() },
                center = { crucible.statusline.model({ max_length = 20 }) },
                right = { crucible.statusline.context() },
            })
        "#,
        )
        .exec()
        .unwrap();

        let config = get_statusline_config();
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.left.len(), 1);
        assert_eq!(config.center.len(), 1);
        assert_eq!(config.right.len(), 1);
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
        reset_config();

        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("init.lua"),
            r#"
            crucible.statusline.setup({
                left = { crucible.statusline.mode() },
            })
        "#,
        )
        .unwrap();

        let loader = ConfigLoader::new(tmp.path(), None);
        let lua = create_test_lua();
        loader.load(&lua).unwrap();

        let config = get_statusline_config();
        assert!(config.is_some());
    }

    #[test]
    fn test_mode_defaults() {
        let lua = create_test_lua();
        let crucible: Table = lua.globals().get("crucible").unwrap();
        register_statusline_namespace(&lua, &crucible).unwrap();

        // mode() with no args should have defaults
        let result: Table = lua
            .load("return crucible.statusline.mode()")
            .eval()
            .unwrap();

        let normal: Table = result.get("normal").unwrap();
        assert_eq!(normal.get::<String>("text").unwrap(), " NORMAL ");
        assert_eq!(normal.get::<String>("bg").unwrap(), "green");
    }
}
