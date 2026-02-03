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
//! -- Join path components
//! local plugin_path = paths.join(paths.kiln(), "plugins", "my_plugin.lua")
//! ```

use crate::error::LuaError;
use mlua::{Lua, Value};
use std::path::PathBuf;

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
    fn test_missing_kiln_error() {
        let ctx = PathsContext::new(); // No paths configured
        let lua = create_lua_with_paths(ctx);

        let result: Result<String, _> = lua.load("return paths.kiln()").eval();
        assert!(result.is_err());
    }
}
