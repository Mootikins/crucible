//! File system module for Lua scripts — the reduced surface.
//!
//! Only what the Lua standard library cannot do (or cannot do safely) lives
//! here. Files are read and written with `io.open`; this module covers
//! directories and file-type queries:
//!
//! ```lua
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
use std::path::Path;

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

    fs.func("exists", "(path: string) -> boolean", |_lua, path: String| {
        Ok(checked(&path)?.exists())
    })?;

    fs.func("is_file", "(path: string) -> boolean", |_lua, path: String| {
        Ok(checked(&path)?.is_file())
    })?;

    fs.func("is_dir", "(path: string) -> boolean", |_lua, path: String| {
        Ok(checked(&path)?.is_dir())
    })?;

    // Recursive delete, under a name that says so. For one file, stdlib
    // `os.remove` is the tool.
    fs.func("remove_all", "(path: string) -> ()", |_lua, path: String| {
        let target = checked(&path)?;
        fs::remove_dir_all(target)
            .lua_runtime()
            .map_err(mlua::Error::external)
    })?;

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

    fs.func("list", "(path: string) -> { string }", |lua, path: String| {
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
    })?;

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
    use mlua::Table;
    use tempfile::TempDir;

    fn create_lua() -> Lua {
        let lua = Lua::new();
        register_fs_module(&lua).unwrap();
        lua
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
