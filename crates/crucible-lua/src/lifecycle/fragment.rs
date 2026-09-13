//! A plugin's fragment: `spec.luau`, read in the daemon VM.
//!
//! A fragment describes a plugin. It does not act. `read_fragment` evaluates
//! the file in the daemon VM, so no second VM exists, with an environment
//! that holds six pure functions and a copy each of `string`, `table` and
//! `math`. The environment and the three copies refuse a write, so a
//! fragment cannot change a table the daemon VM reads. A name outside the
//! environment reads as `nil`, so a call to `cru.on` or `require` raises
//! before it can reach the host. See `docs/Meta/CONTEXT.md`, "Fragment".

use super::error::{LifecycleError, LifecycleResult};
use mlua::{Lua, Table, Value};
use std::path::Path;

/// The one file name of a fragment. There is no `spec.lua` fallback.
pub const FRAGMENT_FILE: &str = "spec.luau";

/// The functions a fragment can call by name. Each is pure: none reaches a
/// file, a process, the registry or the host.
const PURE_FUNCTIONS: [&str; 6] = ["tostring", "tonumber", "ipairs", "pairs", "select", "type"];

/// The libraries a fragment reads through a copy, never through the VM's own
/// table. A write to the VM's `string` table would reach every later chunk.
const COPIED_LIBRARIES: [&str; 3] = ["string", "table", "math"];

/// The `math` entries the copy omits. Each one mutates VM state.
const OMITTED_FROM_MATH: [&str; 2] = ["random", "randomseed"];

/// The keys a fragment table can hold.
const FRAGMENT_FIELDS: [&str; 7] = [
    "name",
    "version",
    "description",
    "author",
    "license",
    "intercepts_tools",
    "opts",
];

/// The keys of the plugin table that `init.luau` returns. A fragment that
/// holds one of these is a plugin table in the wrong file.
const INIT_FIELDS: [&str; 5] = ["tools", "commands", "services", "setup", "handlers"];

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

/// The `_ENV` a fragment runs in. It holds [`PURE_FUNCTIONS`] and a sealed
/// copy of each of [`COPIED_LIBRARIES`]. A name outside that set is `nil`.
/// A write to the environment, or to a copy, raises: see [`sealed`].
///
/// A string method reached through the string metatable, as in
/// `("x"):upper()`, still resolves in the VM's real `string` table. That is
/// safe: a fragment can call such a function, but the metatable gives it no
/// way to reassign one.
fn read_only_env(lua: &Lua) -> mlua::Result<Table> {
    let globals = lua.globals();
    let names = lua.create_table()?;
    for name in PURE_FUNCTIONS {
        names.set(name, globals.get::<Value>(name)?)?;
    }
    for library in COPIED_LIBRARIES {
        let real: Table = globals.get(library)?;
        let copy = lua.create_table()?;
        for pair in real.pairs::<Value, Value>() {
            let (key, value) = pair?;
            if library == "math" && OMITTED_FROM_MATH.contains(&key_name(&key).as_str()) {
                continue;
            }
            copy.set(key, value)?;
        }
        names.set(library, sealed(lua, copy, Some(library))?)?;
    }
    sealed(lua, names, None)
}

/// A proxy over `content`. A read resolves in `content`. A write raises,
/// with the key's name and, for a library copy, the library's name.
///
/// The proxy itself stays empty, so every write reaches `__newindex`, also
/// a write to a key that `content` holds. A plain `__newindex` on `content`
/// would let `string.upper = f` land silently, because Lua only consults
/// `__newindex` for a key the table does not hold.
fn sealed(lua: &Lua, content: Table, library: Option<&str>) -> mlua::Result<Table> {
    let prefix = library.map(|l| format!("{l}.")).unwrap_or_default();
    let refuse = lua.create_function(move |_, (_, key, _): (Table, Value, Value)| {
        Err::<(), _>(mlua::Error::runtime(format!(
            "a fragment cannot assign to `{prefix}{}`",
            key_name(&key)
        )))
    })?;
    let meta = lua.create_table()?;
    meta.set("__index", content)?;
    meta.set("__newindex", refuse)?;
    let proxy = lua.create_table()?;
    proxy.set_metatable(Some(meta))?;
    Ok(proxy)
}

/// The name of a table key, for a message. A key that is not a string reads
/// as Lua's `tostring` renders it.
fn key_name(key: &Value) -> String {
    key.to_string()
        .unwrap_or_else(|_| key.type_name().to_owned())
}

/// Read the fields of a fragment table. Each field is optional. A present
/// field with the wrong type is an error that names the file and the field.
/// A key outside [`FRAGMENT_FIELDS`] is an error that names the key.
fn fragment_from_table(lua: &Lua, table: &Table, path: &Path) -> LifecycleResult<Fragment> {
    for pair in table.pairs::<Value, Value>() {
        let (key, _) = pair.map_err(|e| load_error(path, &e))?;
        let key = key_name(&key);
        if FRAGMENT_FIELDS.contains(&key.as_str()) {
            continue;
        }
        let hint = if INIT_FIELDS.contains(&key.as_str()) {
            "; it belongs in init.luau"
        } else {
            ""
        };
        return Err(LifecycleError::LoadError(format!(
            "{}: `{key}` is not a fragment field{hint}",
            path.display()
        )));
    }
    let opts = match table
        .get::<Value>("opts")
        .map_err(|e| load_error(path, &e))?
    {
        Value::Nil => serde_json::json!({}),
        Value::Table(opts) => {
            crate::json_query::lua_to_json(lua, Value::Table(opts)).map_err(|e| {
                load_error(
                    path,
                    &format!("`opts` holds a value JSON cannot carry: {e}"),
                )
            })?
        }
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
        // The VM has a `cru.on` that accepts the call, so only the
        // environment can refuse the fragment.
        let cru = lua.create_table().unwrap();
        let accept = lua
            .create_function(|_, _: mlua::MultiValue| Ok(()))
            .unwrap();
        cru.set("on", accept).unwrap();
        lua.globals().set("cru", cru).unwrap();
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
        for name in [
            "os",
            "io",
            "_G",
            "print",
            "getfenv",
            "setfenv",
            "loadstring",
        ] {
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
    fn a_fragment_cannot_assign_a_global() {
        let (lua, dir) = vm_and_plugin_dir(r#"leaked = 1; return { name = "x" }"#);
        let err = read_fragment(&lua, dir.path()).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("spec.luau"), "{text}");
        assert!(text.contains("cannot assign to `leaked`"), "{text}");
        let leaked: mlua::Value = lua.globals().get("leaked").unwrap();
        assert!(
            leaked.is_nil(),
            "a fragment wrote a global into the daemon VM"
        );
    }

    #[test]
    fn a_fragment_cannot_poison_the_vm_string_table() {
        let (lua, dir) =
            vm_and_plugin_dir(r#"string.upper = function() return "POISON" end; return {}"#);
        let err = read_fragment(&lua, dir.path()).unwrap_err();
        assert!(
            err.to_string().contains("cannot assign to `string.upper`"),
            "{err}"
        );
        let upper: String = lua.load("return string.upper('a')").eval().unwrap();
        assert_eq!(upper, "A", "the fragment poisoned the VM's string table");
        for probe in ["string.upper", "table.insert", "math.floor"] {
            let kind: String = lua.load(format!("return type({probe})")).eval().unwrap();
            assert_eq!(kind, "function", "{probe} is no longer a function");
        }
    }

    #[test]
    fn a_fragment_has_no_math_random() {
        let (lua, dir) = vm_and_plugin_dir(
            "return { name = type(math.random) .. type(math.randomseed) .. type(math.floor) }",
        );
        let f = read_fragment(&lua, dir.path()).unwrap().unwrap();
        assert_eq!(f.name.as_deref(), Some("nilnilfunction"));
    }

    #[test]
    fn an_unknown_key_is_refused_and_says_where_it_belongs() {
        let (lua, dir) = vm_and_plugin_dir(r#"return { name = "x", tools = {} }"#);
        let text = read_fragment(&lua, dir.path()).unwrap_err().to_string();
        assert!(
            text.contains("spec.luau: `tools` is not a fragment field; it belongs in init.luau"),
            "{text}"
        );

        let (lua, dir) = vm_and_plugin_dir(r#"return { intercepts_tool = true }"#);
        let text = read_fragment(&lua, dir.path()).unwrap_err().to_string();
        assert!(
            text.contains("spec.luau: `intercepts_tool` is not a fragment field"),
            "{text}"
        );
        assert!(!text.contains("init.luau"), "{text}");
    }

    #[test]
    fn an_opts_value_json_cannot_carry_is_named() {
        let (lua, dir) = vm_and_plugin_dir(r#"return { opts = { f = function() end } }"#);
        let text = read_fragment(&lua, dir.path()).unwrap_err().to_string();
        assert!(text.contains("spec.luau"), "{text}");
        assert!(
            text.contains("`opts` holds a value JSON cannot carry"),
            "{text}"
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
