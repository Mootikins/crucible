//! File system module for Lua scripts — the reduced surface.
//!
//! Only what the Lua standard library cannot do (or cannot do safely) lives
//! here: directories, file-type queries, and a SCOPED read and write.
//!
//! ## Two properties `io.open` does not have
//!
//! `io.open` is still there, and a plugin may still use it. What it cannot do
//! is say who is calling or where they may reach, so a plugin reading and
//! writing that way is unmarked and unconfined. `cru.fs.read` and
//! `cru.fs.write` are:
//!
//! - **Declared.** Every function here needs the `filesystem` capability, and
//!   a plugin that did not declare it is refused (`Ns::func` installs that
//!   gate from `CruNamespace::required_capability`).
//! - **Scoped.** A plugin's read and write are confined to the roots the host
//!   binds through [`register_fs_roots_resolver`] — the registered kilns, the
//!   workspace, and that plugin's own state directory. Code with no plugin
//!   context is the operator's own and is not confined.
//!
//! ```lua
//! local board = cru.kiln.path("notes", "tickets/one.md")
//! local text = cru.fs.read(board)
//! cru.fs.write(board, text .. "\nstatus: done\n")
//!
//! -- Create directory (with parents)
//! cru.fs.mkdir("path/to/new/dir")
//!
//! -- Check if path exists
//! if cru.fs.exists("config.toml") then
//!     -- ...
//! end
//!
//! -- Check file type
//! if cru.fs.is_file("path") then ... end
//! if cru.fs.is_dir("path") then ... end
//!
//! -- List directory contents
//! local entries = cru.fs.list("path/to/dir")
//!
//! -- Copy a file
//! cru.fs.copy("src.txt", "dest.txt")
//!
//! -- Remove a directory tree (recursive; for one file, use os.remove)
//! cru.fs.remove_all("path/to/dir")
//! ```
//!
//! Two guards survive the reduction, because each prevents a SILENT wrong
//! outcome that a plain removal cannot:
//!
//! - `cru.fs.remove` stays registered as a function that always raises,
//!   naming both replacements. The name's meaning changed (recursive versus
//!   single-file), so a caller must be told which side it is now on rather
//!   than handed a nil — a data-loss guard, not a shim.
//! - Every function refuses the retired `kiln://` scheme. Without that,
//!   `mkdir("kiln://n/x")` silently creates a garbage `./kiln:/n/x` tree
//!   under the daemon's cwd, and `exists` answers a well-formed false. Use
//!   `cru.kiln.path(name, rel)` and plain paths instead.

use crate::error::LuaError;
use crate::error_ext::LuaResultExt;
use mlua::Lua;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The directories one plugin's scoped read and write may reach.
///
/// The host answers, per plugin, because only the host knows: which kilns are
/// registered, where the workspace is, and which state directory belongs to
/// this plugin. `cru.fs` holds no registry of its own — the same rule
/// `cru.kiln.path` follows, where a plugin names a kiln and the daemon
/// resolves it.
pub type FsRootsResolver = Arc<dyn Fn(&str) -> Vec<PathBuf> + Send + Sync>;

/// The installed resolver, in the VM's app data.
///
/// A newtype so the `Option` is the stored value and "no resolver installed"
/// is distinguishable from "installed, and it answers with nothing".
struct FsRoots(FsRootsResolver);

/// Bind the roots a plugin's `cru.fs.read` and `cru.fs.write` may reach.
///
/// A VM with no resolver has no scope to enforce, so a PLUGIN calling either
/// is refused there rather than allowed — an unanswerable "may I" answers no.
/// Code with no plugin context is unaffected either way: the user's own
/// `init.lua` has `io.open` and always has.
pub fn register_fs_roots_resolver(lua: &Lua, resolver: FsRootsResolver) {
    lua.set_app_data(FsRoots(resolver));
}

/// Refuse the retired `kiln://` scheme, permanently.
///
/// A refusal of retired syntax, not a resolver: the scheme used to carry a
/// kiln name through single-string path arguments, and `cru.kiln.path`
/// replaced it. Every registered function checks its path arguments here
/// first, so the scheme can neither resolve nor pass through as a relative
/// path.
fn checked(path: &str) -> Result<&Path, mlua::Error> {
    if path.starts_with("kiln://") {
        return Err(mlua::Error::external(LuaError::Runtime(format!(
            "'{path}': kiln:// is removed; use cru.kiln.path(name, rel)"
        ))));
    }
    Ok(Path::new(path))
}

/// Create the parent directory of `dest` when it does not exist, so `copy`
/// into a new directory keeps working the way it always has.
fn ensure_parent(path: &Path) -> Result<(), LuaError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                LuaError::Runtime(format!(
                    "Failed to create parent directory for '{}': {e}",
                    path.display()
                ))
            })?;
        }
    }
    Ok(())
}

/// The nearest ancestor of `path` that exists, canonicalized, with the part
/// that does not exist yet appended back.
///
/// `canonicalize` fails outright on a path whose last component is not there,
/// which is every first write to a new file. Resolving the existing prefix is
/// what makes `..` and a symlinked directory answer honestly while still
/// letting a write create a file.
fn resolve_for_containment(path: &Path) -> PathBuf {
    let mut tail = Vec::new();
    let mut cursor = path;
    loop {
        if let Ok(canonical) = cursor.canonicalize() {
            let mut resolved = canonical;
            for part in tail.iter().rev() {
                resolved.push(part);
            }
            return resolved;
        }
        match (cursor.file_name(), cursor.parent()) {
            (Some(name), Some(parent)) => {
                tail.push(name.to_os_string());
                cursor = parent;
            }
            // Nothing on this path exists: it cannot be inside any root, and
            // saying so is the containment answer.
            _ => return path.to_path_buf(),
        }
    }
}

/// Resolve `path` for a scoped read or write, refusing anything outside the
/// running plugin's roots.
///
/// **No plugin context means no scope**, exactly as the capability gate reads
/// it: the user's own `init.lua` is the operator's, and it already has
/// `io.open`. What is confined is a PLUGIN, to the kilns, the workspace and
/// the state directory the host says it may reach.
fn scoped(lua: &Lua, path: &str, verb: &str) -> Result<PathBuf, mlua::Error> {
    let target = checked(path)?;
    let Some(plugin) = crate::plugin_context::current_plugin_name(lua) else {
        return Ok(target.to_path_buf());
    };

    let roots = match lua.app_data_ref::<FsRoots>() {
        Some(resolver) => (resolver.0)(&plugin),
        None => {
            return Err(mlua::Error::external(LuaError::Runtime(format!(
                "cru.fs.{verb}('{path}'): this runtime binds no plugin file roots, \
                 so a plugin has no scope to be checked against"
            ))))
        }
    };

    let resolved = resolve_for_containment(target);
    if roots
        .iter()
        .any(|root| resolved.starts_with(resolve_for_containment(root)))
    {
        return Ok(resolved);
    }

    let allowed = roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>();
    let allowed = if allowed.is_empty() {
        "nothing".to_string()
    } else {
        allowed.join(", ")
    };
    Err(mlua::Error::external(LuaError::Runtime(format!(
        "cru.fs.{verb}('{path}'): outside the roots plugin '{plugin}' may reach ({allowed}). \
         Use cru.kiln.path(name, rel), cru.paths.workspace() or cru.paths.state('{plugin}')."
    ))))
}

/// Register the fs module.
///
/// Every function declares its Luau type beside its closure, and `Ns` holds
/// the declaration to the Rust types at registration. See
/// [`crate::host_registry`].
pub fn register_fs_module(lua: &Lua) -> Result<(), LuaError> {
    let mut fs = crate::host_registry::Ns::new(lua, "cru.fs")?;

    // Creates parents, like `mkdir -p`. Answers with nothing and raises on
    // failure.
    fs.func("mkdir", "(path: string) -> ()", |_lua, path: String| {
        let target = checked(&path)?;
        fs::create_dir_all(target)
            .lua_runtime()
            .map_err(mlua::Error::external)
    })?;

    // The two the module never had. Plugins read and write with raw
    // `io.open`, which is unscoped and unmarked; these are gated by the
    // `filesystem` capability (every function in `cru.fs` is — see
    // `CruNamespace::required_capability`) and confined to the roots the host
    // says the running plugin may reach.
    //
    // They RAISE rather than answering `nil, err`, as every other function
    // here does. A refused read that returns nil reads as an empty file at the
    // call site, which is how a scope check becomes silent data loss on the
    // write that follows.
    fs.func("read", "(path: string) -> string", |lua, path: String| {
        let target = scoped(lua, &path, "read")?;
        fs::read_to_string(&target)
            .lua_runtime()
            .map_err(mlua::Error::external)
    })?;
    fs.doc(
        "read",
        "Reads the whole file as a string. Raises when the file is missing, \
         is not UTF-8, or lies outside the roots this plugin may reach.",
    );

    // Creates the parent directory, as `copy` does: a plugin writing its first
    // ticket into a new folder should not have to `mkdir` first.
    fs.func(
        "write",
        "(path: string, contents: string) -> ()",
        |lua, (path, contents): (String, String)| {
            let target = scoped(lua, &path, "write")?;
            ensure_parent(&target).map_err(mlua::Error::external)?;
            fs::write(&target, contents)
                .lua_runtime()
                .map_err(mlua::Error::external)
        },
    )?;
    fs.doc(
        "write",
        "Replaces the file's whole contents, creating its parent directory. \
         Raises when the path lies outside the roots this plugin may reach. \
         Last write wins: there is no compare-and-set.",
    );

    fs.func(
        "exists",
        "(path: string) -> boolean",
        |_lua, path: String| Ok(checked(&path)?.exists()),
    )?;

    fs.func(
        "is_file",
        "(path: string) -> boolean",
        |_lua, path: String| Ok(checked(&path)?.is_file()),
    )?;

    fs.func(
        "is_dir",
        "(path: string) -> boolean",
        |_lua, path: String| Ok(checked(&path)?.is_dir()),
    )?;

    // Recursive delete, under a name that says so. For one file, stdlib
    // `os.remove` is the tool.
    fs.func(
        "remove_all",
        "(path: string) -> ()",
        |_lua, path: String| {
            let target = checked(&path)?;
            fs::remove_dir_all(target)
                .lua_runtime()
                .map_err(mlua::Error::external)
        },
    )?;

    // The data-loss guard. Always raises: the caller expected either a
    // recursive delete or a single-file delete, and silently guessing (or
    // handing back a nil) risks deleting the wrong amount.
    fs.func("remove", "(path: string) -> ()", |_lua, _path: String| {
        Err::<(), _>(mlua::Error::external(LuaError::Runtime(
            "cru.fs.remove is removed: use cru.fs.remove_all (recursive) \
             or os.remove (single file)"
                .to_string(),
        )))
    })?;

    fs.func(
        "list",
        "(path: string) -> { string }",
        |lua, path: String| {
            let target = checked(&path)?;
            let entries = fs::read_dir(target)
                .lua_runtime()
                .map_err(mlua::Error::external)?;
            let table = lua.create_table()?;
            let mut i = 0;
            for entry in entries {
                let entry = entry.lua_runtime().map_err(mlua::Error::external)?;
                if let Some(name) = entry.file_name().to_str() {
                    i += 1;
                    table.set(i, name.to_string())?; // Lua arrays are 1-indexed
                }
            }
            Ok(table)
        },
    )?;

    // Creates dest's parent, as it always has.
    fs.func(
        "copy",
        "(src: string, dest: string) -> ()",
        |_lua, (src, dest): (String, String)| {
            let from = checked(&src)?;
            let to = checked(&dest)?;
            ensure_parent(to).map_err(mlua::Error::external)?;
            fs::copy(from, to)
                .lua_runtime()
                .map_err(mlua::Error::external)?;
            Ok(())
        },
    )?;

    fs.publish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Capability, CapabilitySet};
    use mlua::Table;
    use tempfile::TempDir;

    fn create_lua() -> Lua {
        let lua = Lua::new();
        register_fs_module(&lua).unwrap();
        lua
    }

    /// A VM with `filesystem` granted and one root a plugin may reach.
    fn lua_with_root(root: &Path) -> Lua {
        let lua = create_lua();
        let root = root.to_path_buf();
        register_fs_roots_resolver(&lua, Arc::new(move |_plugin| vec![root.clone()]));
        crate::plugin_context::enter_plugin(
            &lua,
            "scoped",
            [Capability::Filesystem].into_iter().collect(),
        );
        lua
    }

    #[test]
    fn read_and_write_round_trip_inside_a_root() {
        let temp = TempDir::new().unwrap();
        let lua = lua_with_root(temp.path());
        // A file the plugin creates in a directory that does not exist yet:
        // `write` makes the parent, as `copy` does.
        let target = temp.path().join("tickets/one.md");
        let target = target.to_string_lossy().to_string();

        let text: String = lua
            .load(format!(
                r#"
                cru.fs.write("{target}", "status: todo")
                return cru.fs.read("{target}")
                "#
            ))
            .eval()
            .expect("a scoped read and write inside a root must succeed");
        assert_eq!(text, "status: todo");
    }

    /// The scope, and the traversal that would defeat a prefix check done on
    /// the unresolved string.
    #[test]
    fn a_plugin_cannot_read_or_write_outside_its_roots() {
        let temp = TempDir::new().unwrap();
        let inside = temp.path().join("kiln");
        let outside = temp.path().join("secrets");
        fs::create_dir_all(&inside).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("key.txt"), "s3cret").unwrap();

        let lua = lua_with_root(&inside);
        let escapes = [
            outside.join("key.txt").to_string_lossy().to_string(),
            // Same file, reached by walking out of the root.
            inside
                .join("../secrets/key.txt")
                .to_string_lossy()
                .to_string(),
        ];

        for path in escapes {
            let err = lua
                .load(format!(r#"return cru.fs.read("{path}")"#))
                .exec()
                .expect_err("a read outside every root must raise");
            assert!(
                err.to_string().contains("outside the roots"),
                "{path}: expected a scope refusal, got: {err}"
            );

            let err = lua
                .load(format!(r#"cru.fs.write("{path}", "clobbered")"#))
                .exec()
                .expect_err("a write outside every root must raise");
            assert!(
                err.to_string().contains("outside the roots"),
                "{path}: expected a scope refusal, got: {err}"
            );
        }
        assert_eq!(
            fs::read_to_string(outside.join("key.txt")).unwrap(),
            "s3cret",
            "a refused write must not have happened"
        );
    }

    /// The operator's own code has `io.open` and always has, so confining it
    /// would buy nothing and break the user's `init.lua`.
    #[test]
    fn code_with_no_plugin_context_is_not_confined() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("anywhere.txt");
        let target = target.to_string_lossy().to_string();

        let lua = create_lua();
        // No resolver and no plugin context: nothing to confine.
        lua.load(format!(r#"cru.fs.write("{target}", "ok")"#))
            .exec()
            .expect("code outside every plugin writes where it likes");
        let text: String = lua
            .load(format!(r#"return cru.fs.read("{target}")"#))
            .eval()
            .unwrap();
        assert_eq!(text, "ok");
    }

    /// A runtime that binds no roots cannot answer "may this plugin reach
    /// that", and an unanswerable question answers no. Failing open here
    /// would make every embedded VM an unscoped one.
    #[test]
    fn a_plugin_is_refused_when_the_runtime_binds_no_roots() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("x.txt");
        let target = target.to_string_lossy().to_string();

        let lua = create_lua();
        crate::plugin_context::enter_plugin(
            &lua,
            "scoped",
            [Capability::Filesystem].into_iter().collect(),
        );
        let err = lua
            .load(format!(r#"return cru.fs.read("{target}")"#))
            .exec()
            .expect_err("no bound roots must refuse a plugin");
        assert!(
            err.to_string().contains("binds no plugin file roots"),
            "the message must say why: {err}"
        );
    }

    /// The capability half, on the module that gained the read and write.
    /// `cru.fs` is `filesystem`, and a plugin that did not declare it is
    /// refused before its argument is even converted.
    #[test]
    fn the_filesystem_capability_gates_the_whole_module() {
        let temp = TempDir::new().unwrap();
        let lua = create_lua();
        let root = temp.path().to_path_buf();
        register_fs_roots_resolver(&lua, Arc::new(move |_plugin| vec![root.clone()]));
        crate::plugin_context::enter_plugin(&lua, "declares-nothing", CapabilitySet::none());

        for call in [
            r#"cru.fs.read("x")"#,
            r#"cru.fs.write("x", "y")"#,
            r#"cru.fs.mkdir("x")"#,
            r#"cru.fs.exists("x")"#,
        ] {
            let err = match lua.load(call).exec() {
                Ok(()) => panic!("{call}: an ungranted plugin must be refused, not answered"),
                Err(e) => e.to_string(),
            };
            assert!(
                err.contains("did not declare") && err.contains("filesystem"),
                "{call}: expected a `filesystem` refusal, got: {err}"
            );
        }
    }

    /// `cru.fs.remove` is a data-loss guard, not a shim. The name's meaning
    /// changed — `remove_all` is recursive-only, `os.remove` is single-file —
    /// so a caller is told which side it is now on, never handed a nil and
    /// never given a delete with the other semantics.
    #[test]
    fn removed_remove_raises_naming_both_replacements() {
        let temp = TempDir::new().unwrap();
        let victim = temp.path().join("keep.txt");
        fs::write(&victim, "still here").unwrap();

        let lua = create_lua();
        let err = lua
            .load(format!(r#"cru.fs.remove("{}")"#, victim.to_string_lossy()))
            .exec()
            .expect_err("cru.fs.remove must always raise");
        let text = err.to_string();
        assert!(
            text.contains("remove_all"),
            "the error must name the recursive replacement: {text}"
        );
        assert!(
            text.contains("os.remove"),
            "the error must name the single-file replacement: {text}"
        );
        assert!(victim.exists(), "the guard must not delete anything");
    }

    /// `remove_all` is the recursive delete, under a name that says so.
    #[test]
    fn remove_all_removes_a_directory_tree() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("tree");
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("nested/file.txt"), "x").unwrap();

        let lua = create_lua();
        lua.load(format!(r#"cru.fs.remove_all("{}")"#, dir.to_string_lossy()))
            .exec()
            .expect("remove_all must delete a directory tree");
        assert!(!dir.exists());
    }

    /// The `kiln://` scheme is retired syntax, refused by every surviving
    /// function. Without the guard, `mkdir("kiln://n/x")` silently creates a
    /// garbage `./kiln:/n/x` tree under the daemon's cwd, and `exists`
    /// returns a well-formed false.
    #[test]
    fn every_surviving_function_refuses_the_kiln_scheme() {
        let temp = TempDir::new().unwrap();
        // Run against a tempdir so a FAILING guard cannot litter the repo —
        // and so the no-garbage assertion below has a directory to itself.
        let real = temp.path().join("real.txt");
        fs::write(&real, "x").unwrap();
        let real = real.to_string_lossy();

        let lua = create_lua();
        let calls = [
            r#"return cru.fs.exists("kiln://notes/x")"#.to_string(),
            r#"return cru.fs.is_file("kiln://notes/x")"#.to_string(),
            r#"return cru.fs.is_dir("kiln://notes/x")"#.to_string(),
            r#"return cru.fs.list("kiln://notes/x")"#.to_string(),
            r#"cru.fs.mkdir("kiln://notes/x")"#.to_string(),
            r#"cru.fs.remove_all("kiln://notes/x")"#.to_string(),
            format!(r#"cru.fs.copy("kiln://notes/x", "{real}")"#),
            format!(r#"cru.fs.copy("{real}", "kiln://notes/x")"#),
        ];
        for call in &calls {
            // `exec` succeeding would mean the scheme resolved silently — the
            // exact failure the guard exists to prevent.
            let err = match lua.load(call.as_str()).exec() {
                Ok(()) => panic!("{call}: the scheme must raise, not resolve"),
                Err(e) => e,
            };
            let text = err.to_string();
            assert!(
                text.contains("kiln:// is removed"),
                "{call}: the error must say the scheme is gone: {text}"
            );
            assert!(
                text.contains("cru.kiln.path"),
                "{call}: the error must name the replacement: {text}"
            );
        }
        // And no garbage `kiln:` tree appeared anywhere a resolve would put one.
        assert!(
            !temp.path().join("kiln:").exists(),
            "a kiln:// argument must never become a relative directory"
        );
    }

    #[test]
    fn test_mkdir_and_exists() {
        let temp = TempDir::new().unwrap();
        let dir_path = temp.path().join("nested/dirs/here");
        let path_str = dir_path.to_string_lossy().to_string();

        let lua = create_lua();
        let result: Table = lua
            .load(format!(
                r#"
            local before = cru.fs.exists("{0}")
            cru.fs.mkdir("{0}")
            local after = cru.fs.exists("{0}")
            return {{ before = before, after = after }}
            "#,
                path_str
            ))
            .eval()
            .unwrap();

        assert!(!result.get::<bool>("before").unwrap());
        assert!(result.get::<bool>("after").unwrap());
    }

    #[test]
    fn test_is_file_and_is_dir() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("file.txt");
        let dir_path = temp.path().join("dir");

        fs::write(&file_path, "content").unwrap();
        fs::create_dir(&dir_path).unwrap();

        let lua = create_lua();
        let result: Table = lua
            .load(format!(
                r#"
            return {{
                file_is_file = cru.fs.is_file("{}"),
                file_is_dir = cru.fs.is_dir("{}"),
                dir_is_file = cru.fs.is_file("{}"),
                dir_is_dir = cru.fs.is_dir("{}")
            }}
            "#,
                file_path.to_string_lossy(),
                file_path.to_string_lossy(),
                dir_path.to_string_lossy(),
                dir_path.to_string_lossy()
            ))
            .eval()
            .unwrap();

        assert!(result.get::<bool>("file_is_file").unwrap());
        assert!(!result.get::<bool>("file_is_dir").unwrap());
        assert!(!result.get::<bool>("dir_is_file").unwrap());
        assert!(result.get::<bool>("dir_is_dir").unwrap());
    }

    #[test]
    fn test_list() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "").unwrap();
        fs::write(temp.path().join("b.txt"), "").unwrap();
        fs::create_dir(temp.path().join("c")).unwrap();
        let dir_path = temp.path().to_string_lossy().to_string();

        let lua = create_lua();
        let result: Table = lua
            .load(format!(r#"return cru.fs.list("{}")"#, dir_path))
            .eval()
            .unwrap();

        let entries: Vec<String> = result
            .pairs::<i64, String>()
            .filter_map(|r| r.ok())
            .map(|(_, v)| v)
            .collect();

        assert_eq!(entries.len(), 3);
        assert!(entries.contains(&"a.txt".to_string()));
        assert!(entries.contains(&"b.txt".to_string()));
        assert!(entries.contains(&"c".to_string()));
    }

    #[test]
    fn test_copy() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src.txt");
        let dest = temp.path().join("into/new/dir/dest.txt");
        fs::write(&src, "original").unwrap();

        let lua = create_lua();
        lua.load(format!(
            r#"cru.fs.copy("{}", "{}")"#,
            src.to_string_lossy(),
            dest.to_string_lossy()
        ))
        .exec()
        .unwrap();

        assert_eq!(fs::read_to_string(&dest).unwrap(), "original");
    }
}
