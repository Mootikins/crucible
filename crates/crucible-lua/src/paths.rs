//! Paths module for Lua scripts
//!
//! Provides functions to get standard Crucible paths.
//!
//! ## Usage in Lua
//!
//! ```lua
//! -- Get the kiln root directory
//! local kiln_path = paths.kiln()
//!
//! -- Get the current session directory
//! local session_path = paths.session()
//!
//! -- Get the workspace directory
//! local workspace_path = paths.workspace()
//!
//! -- Get this plugin's own state directory, created on demand
//! local state_dir = paths.state("discord")
//!
//! -- Join path components
//! local plugin_path = paths.join(paths.kiln(), "plugins", "my_plugin.lua")
//! ```

use crate::error::LuaError;
use mlua::{Lua, Value};
use std::path::{Component, Path, PathBuf};

/// Directory under the Crucible home that holds per-plugin state.
const PLUGIN_STATE_DIR: &str = "plugin-state";

/// Paths context containing configured paths
#[derive(Debug, Clone)]
pub struct PathsContext {
    /// The kiln root directory
    pub kiln: Option<PathBuf>,
    /// The current session directory
    pub session: Option<PathBuf>,
    /// The workspace directory
    pub workspace: Option<PathBuf>,
}

impl PathsContext {
    /// Create a new empty paths context
    pub fn new() -> Self {
        Self {
            kiln: None,
            session: None,
            workspace: None,
        }
    }

    /// Set the kiln path
    pub fn with_kiln(mut self, path: PathBuf) -> Self {
        self.kiln = Some(path);
        self
    }

    /// Set the session path
    pub fn with_session(mut self, path: PathBuf) -> Self {
        self.session = Some(path);
        self
    }

    /// Set the workspace path
    pub fn with_workspace(mut self, path: PathBuf) -> Self {
        self.workspace = Some(path);
        self
    }
}

impl Default for PathsContext {
    fn default() -> Self {
        Self::new()
    }
}

/// `<home>/plugin-state/<plugin>/`, created if it is not there yet.
///
/// `plugin` must name exactly one path component. It arrives from Lua, where
/// the caller names *itself* — one VM serves every daemon plugin, so there is
/// no ambient "current plugin" to read it from — and a name like `../..` would
/// otherwise hand a plugin the whole data root.
fn plugin_state_dir(home: &Path, plugin: &str) -> Result<PathBuf, LuaError> {
    let mut components = Path::new(plugin).components();
    let name = match (components.next(), components.next()) {
        (Some(Component::Normal(name)), None) => name,
        _ => {
            return Err(LuaError::Runtime(format!(
                "paths.state: '{plugin}' is not a plugin name"
            )))
        }
    };

    let dir = home.join(PLUGIN_STATE_DIR).join(name);
    std::fs::create_dir_all(&dir).map_err(|e| {
        LuaError::Runtime(format!(
            "paths.state: could not create '{}': {e}",
            dir.display()
        ))
    })?;
    Ok(dir)
}

/// Register the paths module with a Lua state
pub fn register_paths_module(lua: &Lua, context: PathsContext) -> Result<(), LuaError> {
    let paths = lua.create_table()?;

    // paths.kiln() -> string or nil
    let kiln_path = context.kiln.clone();
    let kiln_fn = lua.create_function(move |lua, ()| match &kiln_path {
        Some(path) => Ok(Value::String(
            lua.create_string(path.to_string_lossy().as_ref())?,
        )),
        None => Err(mlua::Error::external(LuaError::Runtime(
            "Kiln path not configured".to_string(),
        ))),
    })?;
    paths.set("kiln", kiln_fn)?;

    // paths.session() -> string or nil
    let session_path = context.session.clone();
    let session_fn = lua.create_function(move |lua, ()| match &session_path {
        Some(path) => Ok(Value::String(
            lua.create_string(path.to_string_lossy().as_ref())?,
        )),
        None => Err(mlua::Error::external(LuaError::Runtime(
            "Session path not configured".to_string(),
        ))),
    })?;
    paths.set("session", session_fn)?;

    // paths.workspace() -> string or nil
    let workspace_path = context.workspace.clone();
    let workspace_fn = lua.create_function(move |lua, ()| match &workspace_path {
        Some(path) => Ok(Value::String(
            lua.create_string(path.to_string_lossy().as_ref())?,
        )),
        None => Err(mlua::Error::external(LuaError::Runtime(
            "Workspace path not configured".to_string(),
        ))),
    })?;
    paths.set("workspace", workspace_fn)?;

    // paths.state(plugin) -> string
    //
    // Not derived from `context`: the daemon loads every plugin into one Lua
    // state, so a state directory baked in at registration would be the same
    // directory for all of them.
    let state_fn = lua.create_function(|lua, plugin: String| {
        let dir = plugin_state_dir(&crucible_core::config::crucible_home(), &plugin)
            .map_err(mlua::Error::external)?;
        Ok(Value::String(
            lua.create_string(dir.to_string_lossy().as_ref())?,
        ))
    })?;
    paths.set("state", state_fn)?;

    // paths.join(base, ...) -> string
    // Joins path components
    let join_fn = lua.create_function(|lua, args: mlua::MultiValue| {
        let mut path = PathBuf::new();
        for arg in args {
            if let Value::String(s) = arg {
                let component: String = s.to_str()?.to_string();
                path.push(&component);
            }
        }
        Ok(Value::String(
            lua.create_string(path.to_string_lossy().as_ref())?,
        ))
    })?;
    paths.set("join", join_fn)?;

    // Register paths module globally
    lua.globals().set("paths", paths.clone())?;
    crate::lua_util::register_in_namespaces(lua, "paths", paths)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_lua_with_paths(ctx: PathsContext) -> Lua {
        let lua = Lua::new();
        register_paths_module(&lua, ctx).unwrap();
        lua
    }

    #[test]
    fn test_kiln_path() {
        let ctx = PathsContext::new().with_kiln(PathBuf::from("/home/user/notes"));
        let lua = create_lua_with_paths(ctx);

        let result: String = lua.load("return paths.kiln()").eval().unwrap();
        assert_eq!(result, "/home/user/notes");
    }

    #[test]
    fn test_session_path() {
        let ctx = PathsContext::new()
            .with_session(PathBuf::from("/home/user/notes/.crucible/sessions/abc123"));
        let lua = create_lua_with_paths(ctx);

        let result: String = lua.load("return paths.session()").eval().unwrap();
        assert_eq!(result, "/home/user/notes/.crucible/sessions/abc123");
    }

    #[test]
    fn test_workspace_path() {
        let ctx =
            PathsContext::new().with_workspace(PathBuf::from("/home/user/projects/myproject"));
        let lua = create_lua_with_paths(ctx);

        let result: String = lua.load("return paths.workspace()").eval().unwrap();
        assert_eq!(result, "/home/user/projects/myproject");
    }

    #[test]
    fn test_path_join() {
        let ctx = PathsContext::new().with_kiln(PathBuf::from("/home/user/notes"));
        let lua = create_lua_with_paths(ctx);

        let result: String = lua
            .load(r#"return paths.join(paths.kiln(), "plugins", "my_plugin.lua")"#)
            .eval()
            .unwrap();
        assert_eq!(result, "/home/user/notes/plugins/my_plugin.lua");
    }

    #[test]
    fn state_dir_is_created_on_demand_and_is_stable() {
        let temp = tempfile::TempDir::new().unwrap();

        let first = plugin_state_dir(temp.path(), "discord").unwrap();
        assert_eq!(first, temp.path().join("plugin-state").join("discord"));
        assert!(first.is_dir());

        // Called again on a directory that already exists — the plugin asks on
        // every save, and the second ask must not be an error.
        assert_eq!(plugin_state_dir(temp.path(), "discord").unwrap(), first);
    }

    #[test]
    fn state_dir_gives_each_plugin_its_own() {
        let temp = tempfile::TempDir::new().unwrap();

        let discord = plugin_state_dir(temp.path(), "discord").unwrap();
        let oci = plugin_state_dir(temp.path(), "oci").unwrap();
        assert_ne!(discord, oci);
    }

    /// A plugin names itself, so the name is untrusted input: anything that is
    /// not a single path component would let it write outside the data root.
    #[test]
    fn state_dir_refuses_a_name_that_is_not_one_component() {
        let temp = tempfile::TempDir::new().unwrap();

        for name in ["", ".", "..", "../escape", "a/b", "/absolute"] {
            assert!(
                plugin_state_dir(temp.path(), name).is_err(),
                "expected '{name}' to be refused"
            );
        }
        assert!(!temp.path().join("plugin-state").join("escape").exists());
    }

    #[test]
    fn test_missing_kiln_error() {
        let ctx = PathsContext::new(); // No paths configured
        let lua = create_lua_with_paths(ctx);

        let result: Result<String, _> = lua.load("return paths.kiln()").eval();
        assert!(result.is_err());
    }
}
