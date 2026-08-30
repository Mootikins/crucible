//! Paths module for Lua scripts
//!
//! Provides functions to get standard Crucible paths.
//!
//! ## Usage in Lua
//!
//! ```lua
//! -- Get the current session directory
//! local session_path = cru.paths.session()
//!
//! -- Get the workspace directory
//! local workspace_path = cru.paths.workspace()
//!
//! -- Get this plugin's own state directory, created on demand
//! local state_dir = cru.paths.state("discord")
//! ```
//!
//! Path JOINING is plain string concatenation in Lua — the accessors return
//! absolute directories, so `root .. "/" .. name` is the whole join.

use crate::error::LuaError;
use mlua::{Lua, Value};
use std::path::{Component, Path, PathBuf};

/// Directory under the Crucible home that holds per-plugin state.
const PLUGIN_STATE_DIR: &str = "plugin-state";

/// Paths context containing configured paths
#[derive(Debug, Clone)]
pub struct PathsContext {
    /// The current session directory
    pub session: Option<PathBuf>,
    /// The workspace directory
    pub workspace: Option<PathBuf>,
}

impl PathsContext {
    /// Create a new empty paths context
    pub fn new() -> Self {
        Self {
            session: None,
            workspace: None,
        }
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

/// Register the paths module with a Lua state.
///
/// Every function declares its Luau type beside its closure, and `Ns` holds
/// the declaration to the Rust types at registration. See
/// [`crate::host_registry`].
pub fn register_paths_module(lua: &Lua, context: PathsContext) -> Result<(), LuaError> {
    let mut paths = crate::host_registry::Ns::new(lua, "cru.paths")?;

    // `-> string`, never `string?`: an unconfigured session RAISES. A declared
    // `string?` would make every caller nil-check what cannot be nil.
    let session_path = context.session.clone();
    paths.func(
        "session",
        "() -> string",
        move |lua, ()| match &session_path {
            Some(path) => Ok(Value::String(
                lua.create_string(path.to_string_lossy().as_ref())?,
            )),
            None => Err(mlua::Error::external(LuaError::Runtime(
                "Session path not configured".to_string(),
            ))),
        },
    )?;

    // Raises when no workspace is configured, exactly as `session` does.
    let workspace_path = context.workspace.clone();
    paths.func(
        "workspace",
        "() -> string",
        move |lua, ()| match &workspace_path {
            Some(path) => Ok(Value::String(
                lua.create_string(path.to_string_lossy().as_ref())?,
            )),
            None => Err(mlua::Error::external(LuaError::Runtime(
                "Workspace path not configured".to_string(),
            ))),
        },
    )?;

    // Not derived from `context`: the daemon loads every plugin into one Lua
    // state, so a state directory baked in at registration would be the same
    // directory for all of them.
    //
    // The plugin names ITSELF, so the name is untrusted: `plugin_state_dir`
    // raises for anything that is not one path component.
    paths.func(
        "state",
        "(plugin: string) -> string",
        |lua, plugin: String| {
            // `?` converts a `LuaError` on its own (`error.rs`).
            let dir = plugin_state_dir(&crucible_core::config::crucible_home(), &plugin)?;
            Ok(Value::String(
                lua.create_string(dir.to_string_lossy().as_ref())?,
            ))
        },
    )?;

    // Where `init.lua` lives. Not derived from `context`: the config root is
    // the same directory for every VM, and the loader computes it from the
    // one function this calls.
    paths.func("config", "() -> string", |lua, ()| {
        let dir = crate::config::default_config_dir();
        Ok(Value::String(
            lua.create_string(dir.to_string_lossy().as_ref())?,
        ))
    })?;

    paths.publish()?;

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
    fn test_session_path() {
        let ctx = PathsContext::new()
            .with_session(PathBuf::from("/home/user/notes/.crucible/sessions/abc123"));
        let lua = create_lua_with_paths(ctx);

        let result: String = lua.load("return cru.paths.session()").eval().unwrap();
        assert_eq!(result, "/home/user/notes/.crucible/sessions/abc123");
    }

    #[test]
    fn test_workspace_path() {
        let ctx =
            PathsContext::new().with_workspace(PathBuf::from("/home/user/projects/myproject"));
        let lua = create_lua_with_paths(ctx);

        let result: String = lua.load("return cru.paths.workspace()").eval().unwrap();
        assert_eq!(result, "/home/user/projects/myproject");
    }

    /// `cru.paths.config()` must name the directory the loader reads
    /// `init.lua` from — one function answers both.
    #[test]
    fn config_path_equals_the_loader_config_dir() {
        let lua = create_lua_with_paths(PathsContext::new());

        let result: String = lua.load("return cru.paths.config()").eval().unwrap();
        assert_eq!(
            PathBuf::from(result),
            crate::config::ConfigLoader::with_defaults(None)
                .config_dir()
                .to_path_buf()
        );
    }

    /// `$CRUCIBLE_CONFIG_DIR` redirects the config root for test isolation.
    /// `cru.paths.config()` must follow it, or a test that redirects the
    /// variable still reads the developer's real `~/.config/crucible`.
    ///
    /// This test reads the environment on purpose, which is the one case
    /// `EnvVarGuard` exists for. nextest gives each test its own process,
    /// so the guard cannot race a second test.
    #[test]
    fn config_path_follows_the_config_dir_env_var() {
        let temp = tempfile::tempdir().unwrap();
        let _guard = crucible_core::test_support::EnvVarGuard::set(
            "CRUCIBLE_CONFIG_DIR",
            temp.path().to_string_lossy().into_owned(),
        );

        let lua = create_lua_with_paths(PathsContext::new());

        let result: String = lua.load("return cru.paths.config()").eval().unwrap();
        assert_eq!(PathBuf::from(result), temp.path());
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
    /// An unconfigured path RAISES. That is why `cru.paths.session` is
    /// declared `-> string` and not `-> string?`: there is no nil answer to
    /// nil-check for.
    ///
    /// This asked `cru.paths.kiln()` until 2026-08-30 — a function this
    /// module has never registered. It passed because indexing nil raises,
    /// so it proved nothing about paths at all.
    fn an_unconfigured_path_raises() {
        let ctx = PathsContext::new(); // No paths configured
        let lua = create_lua_with_paths(ctx);

        for call in ["cru.paths.session()", "cru.paths.workspace()"] {
            let result: Result<String, _> = lua.load(format!("return {call}")).eval();
            let err = result.expect_err("an unconfigured path must raise");
            assert!(
                err.to_string().contains("not configured"),
                "{call} must say what is missing: {err}"
            );
        }
    }
}
