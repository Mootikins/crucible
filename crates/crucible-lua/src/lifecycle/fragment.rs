//! A plugin's fragment: `spec.luau`, read in the daemon VM.
//!
//! A fragment describes a plugin. It does not act. `read_fragment` evaluates
//! the file in the daemon VM, so no second VM exists, with an environment
//! that holds nine pure names and nothing else. A name outside that list
//! reads as `nil`, so a call to `cru.on` or `require` raises before it can
//! reach the host. See `docs/Meta/CONTEXT.md`, "Fragment".

use super::error::{LifecycleError, LifecycleResult};
use mlua::{Lua, Table, Value};
use std::path::Path;

/// The one file name of a fragment. There is no `spec.lua` fallback.
pub const FRAGMENT_FILE: &str = "spec.luau";

/// The names a fragment can read. Each is pure: none reaches a file, a
/// process, the registry or the host.
const READ_ONLY_NAMES: [&str; 9] = [
    "string", "table", "math", "tostring", "tonumber", "ipairs", "pairs", "select", "type",
];

/// What a plugin says about itself without running. See CONTEXT.md, "Fragment".
#[derive(Debug, Clone, Default)]
pub struct Fragment {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    /// The intercept grant. `false` when the fragment says nothing.
    pub intercepts_tools: bool,
    /// The default `opts`. An empty object when the fragment says nothing.
    pub opts: serde_json::Value,
}

/// Read the fragment in `plugin_dir`, or `None` when the directory has none.
///
/// An error names the file. A wrong-typed field is an error, not a silent
/// `None`: a fragment that says `version = 3` is a mistake the author must
/// see. A `spec.lua` beside `spec.luau` is refused, with the same wording as
/// an `init.lua` beside `init.luau`.
pub fn read_fragment(lua: &Lua, plugin_dir: &Path) -> LifecycleResult<Option<Fragment>> {
    let path = plugin_dir.join(FRAGMENT_FILE);
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(LifecycleError::Io(e)),
    };
    let lua_twin = path.with_extension("lua");
    if lua_twin.is_file() {
        return Err(LifecycleError::LoadError(
            crate::source_files::Ambiguous(vec![path, lua_twin]).to_string(),
        ));
    }
    let env = read_only_env(lua).map_err(|e| load_error(&path, &e))?;
    let value: Value = lua
        .load(&source)
        .set_name(format!("@{}", path.display()))
        .set_environment(env)
        .eval()
        .map_err(|e| load_error(&path, &e))?;
    let Value::Table(table) = value else {
        return Err(LifecycleError::LoadError(format!(
            "{}: a fragment returns a table, not {}",
            path.display(),
            value.type_name()
        )));
    };
    fragment_from_table(lua, &table, &path).map(Some)
}

/// The `_ENV` a fragment runs in. No `__index`, so a name outside
/// [`READ_ONLY_NAMES`] is `nil`, and a write lands in this table and not in
/// the VM's globals.
fn read_only_env(lua: &Lua) -> mlua::Result<Table> {
    let env = lua.create_table()?;
    let globals = lua.globals();
    for name in READ_ONLY_NAMES {
        env.set(name, globals.get::<Value>(name)?)?;
    }
    Ok(env)
}

/// Read the fields of a fragment table. Each field is optional. A present
/// field with the wrong type is an error that names the file and the field.
fn fragment_from_table(lua: &Lua, table: &Table, path: &Path) -> LifecycleResult<Fragment> {
    let opts = match table
        .get::<Value>("opts")
        .map_err(|e| load_error(path, &e))?
    {
        Value::Nil => serde_json::json!({}),
        Value::Table(opts) => crate::json_query::lua_to_json(lua, Value::Table(opts))
            .map_err(|e| load_error(path, &e))?,
        other => return Err(wrong_type(path, "opts", "a table", &other)),
    };
    Ok(Fragment {
        name: string_field(table, path, "name")?,
        version: string_field(table, path, "version")?,
        description: string_field(table, path, "description")?,
        author: string_field(table, path, "author")?,
        license: string_field(table, path, "license")?,
        intercepts_tools: bool_field(table, path, "intercepts_tools")?.unwrap_or(false),
        opts,
    })
}

fn string_field(table: &Table, path: &Path, field: &str) -> LifecycleResult<Option<String>> {
    match table
        .get::<Value>(field)
        .map_err(|e| load_error(path, &e))?
    {
        Value::Nil => Ok(None),
        Value::String(s) => Ok(Some(
            s.to_str().map_err(|e| load_error(path, &e))?.to_owned(),
        )),
        other => Err(wrong_type(path, field, "a string", &other)),
    }
}

fn bool_field(table: &Table, path: &Path, field: &str) -> LifecycleResult<Option<bool>> {
    match table
        .get::<Value>(field)
        .map_err(|e| load_error(path, &e))?
    {
        Value::Nil => Ok(None),
        Value::Boolean(b) => Ok(Some(b)),
        other => Err(wrong_type(path, field, "a boolean", &other)),
    }
}

fn wrong_type(path: &Path, field: &str, expected: &str, got: &Value) -> LifecycleError {
    LifecycleError::LoadError(format!(
        "{}: `{field}` is {expected}, not {}",
        path.display(),
        got.type_name()
    ))
}

fn load_error(path: &Path, e: &dyn std::fmt::Display) -> LifecycleError {
    LifecycleError::LoadError(format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;
    use tempfile::TempDir;

    /// A plain VM and a plugin directory that holds `init.luau` and the
    /// given fragment.
    fn vm_and_plugin_dir(fragment: &str) -> (Lua, TempDir) {
        let (lua, dir) = vm_and_plugin_dir_without_fragment();
        std::fs::write(dir.path().join(FRAGMENT_FILE), fragment).unwrap();
        (lua, dir)
    }

    fn vm_and_plugin_dir_without_fragment() -> (Lua, TempDir) {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("init.luau"), "return {}").unwrap();
        (Lua::new(), dir)
    }

    #[test]
    fn a_fragment_returns_metadata() {
        let (lua, dir) = vm_and_plugin_dir(
            r#"return { name = "greeter", version = "1.2.0", description = "says hi", author = "ann", license = "MIT", intercepts_tools = true, opts = { greeting = "hi" } }"#,
        );
        let f = read_fragment(&lua, dir.path()).unwrap().unwrap();
        assert_eq!(f.name.as_deref(), Some("greeter"));
        assert_eq!(f.version.as_deref(), Some("1.2.0"));
        assert_eq!(f.description.as_deref(), Some("says hi"));
        assert_eq!(f.author.as_deref(), Some("ann"));
        assert_eq!(f.license.as_deref(), Some("MIT"));
        assert!(f.intercepts_tools);
        assert_eq!(f.opts["greeting"], "hi");
    }

    #[test]
    fn a_fragment_with_no_fields_is_the_default() {
        let (lua, dir) = vm_and_plugin_dir("return {}");
        let f = read_fragment(&lua, dir.path()).unwrap().unwrap();
        assert!(f.name.is_none());
        assert!(!f.intercepts_tools);
        assert_eq!(f.opts, serde_json::json!({}));
    }

    #[test]
    fn a_missing_fragment_is_none_not_an_error() {
        let (lua, dir) = vm_and_plugin_dir_without_fragment();
        assert!(read_fragment(&lua, dir.path()).unwrap().is_none());
    }

    #[test]
    fn a_fragment_that_registers_a_handler_is_refused() {
        let (lua, dir) =
            vm_and_plugin_dir(r#"cru.on("turn:complete", function() end); return { name = "x" }"#);
        let err = read_fragment(&lua, dir.path()).unwrap_err();
        assert!(err.to_string().contains("spec.luau"), "{err}");
        // Luau words the raise as "attempt to index nil", which is the
        // read-only env answering `nil` for `cru`.
        assert!(err.to_string().contains("nil"), "{err}");
    }

    #[test]
    fn a_fragment_cannot_require() {
        let (lua, dir) = vm_and_plugin_dir(r#"local x = require("os"); return { name = "x" }"#);
        assert!(read_fragment(&lua, dir.path()).is_err());
    }

    #[test]
    fn a_fragment_sees_no_os_io_or_globals_table() {
        for name in ["os", "io", "_G", "print", "getfenv", "setfenv", "load"] {
            let (lua, dir) = vm_and_plugin_dir(&format!("return {{ name = type({name}) }}"));
            let f = read_fragment(&lua, dir.path()).unwrap().unwrap();
            assert_eq!(
                f.name.as_deref(),
                Some("nil"),
                "{name} leaked into the fragment env"
            );
        }
    }

    #[test]
    fn a_fragment_does_not_write_the_vm_globals() {
        let (lua, dir) = vm_and_plugin_dir(r#"leaked = 1; return { name = "x" }"#);
        read_fragment(&lua, dir.path()).unwrap().unwrap();
        let leaked: mlua::Value = lua.globals().get("leaked").unwrap();
        assert!(
            leaked.is_nil(),
            "a fragment wrote a global into the daemon VM"
        );
    }

    #[test]
    fn a_fragment_that_returns_no_table_is_refused_with_its_path() {
        let (lua, dir) = vm_and_plugin_dir(r#"return 42"#);
        let err = read_fragment(&lua, dir.path()).unwrap_err();
        assert!(err.to_string().contains("spec.luau"), "{err}");
    }

    #[test]
    fn a_wrong_typed_field_names_the_file_and_field() {
        let (lua, dir) = vm_and_plugin_dir(r#"return { version = 3 }"#);
        let err = read_fragment(&lua, dir.path()).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("spec.luau"), "{text}");
        assert!(text.contains("version"), "{text}");
        assert!(matches!(err, LifecycleError::LoadError(_)), "{err:?}");

        let (lua, dir) = vm_and_plugin_dir(r#"return { intercepts_tools = "yes" }"#);
        let text = read_fragment(&lua, dir.path()).unwrap_err().to_string();
        assert!(text.contains("intercepts_tools"), "{text}");

        let (lua, dir) = vm_and_plugin_dir(r#"return { opts = 7 }"#);
        let text = read_fragment(&lua, dir.path()).unwrap_err().to_string();
        assert!(text.contains("opts"), "{text}");
    }

    #[test]
    fn a_syntax_error_names_the_file() {
        let (lua, dir) = vm_and_plugin_dir("return {");
        let err = read_fragment(&lua, dir.path()).unwrap_err();
        assert!(err.to_string().contains("spec.luau"), "{err}");
    }

    #[test]
    fn a_luau_beside_a_lua_fragment_is_refused_naming_both() {
        let (lua, dir) = vm_and_plugin_dir(r#"return { name = "a" }"#);
        std::fs::write(dir.path().join("spec.lua"), r#"return { name = "b" }"#).unwrap();
        let err = read_fragment(&lua, dir.path()).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("spec.luau"), "{text}");
        assert!(text.contains("spec.lua"), "{text}");
    }

    #[test]
    fn a_lone_spec_lua_is_not_a_fragment() {
        let (lua, dir) = vm_and_plugin_dir_without_fragment();
        std::fs::write(dir.path().join("spec.lua"), r#"return { name = "b" }"#).unwrap();
        assert!(read_fragment(&lua, dir.path()).unwrap().is_none());
    }
}
