//! File system module for Lua scripts
//!
//! Provides file system operations with async support.
//!
//! ## Usage in Lua
//!
//! ```lua
//! -- Read a file
//! local content = fs.read("input.txt")
//!
//! -- Write to a file (creates or overwrites)
//! fs.write("output.txt", "Hello, world!")
//!
//! -- Append to a file
//! fs.append("log.txt", "New log entry\n")
//!
//! -- Create directory (with parents)
//! fs.mkdir("path/to/new/dir")
//!
//! -- Check if path exists
//! if fs.exists("config.toml") then
//!     -- ...
//! end
//!
//! -- Remove a file or directory
//! fs.remove("temp.txt")
//!
//! -- List directory contents
//! local entries = fs.list("path/to/dir")
//!
//! -- Copy a file
//! fs.copy("src.txt", "dest.txt")
//!
//! -- Move/rename a file
//! fs.rename("old.txt", "new.txt")
//!
//! -- Check file type
//! if fs.is_file("path") then ... end
//! if fs.is_dir("path") then ... end
//! ```

use crate::error::LuaError;
use crate::error_ext::LuaResultExt;
use mlua::Lua;
#[cfg(test)]
use mlua::Table;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Maps a kiln NAME to its root directory.
///
/// The host injects it — the daemon passes a registry-backed closure — so
/// this crate never learns where kilns live. The error string reaches Lua
/// as-is: it must name the kiln, never a directory.
pub type KilnPathResolver = Arc<dyn Fn(&str) -> Result<PathBuf, String> + Send + Sync>;

/// The scheme that addresses a kiln by name: `kiln://<name>/<relative>`.
const KILN_SCHEME: &str = "kiln://";

/// Resolve one caller-supplied path.
///
/// A plain path passes through unchanged. A `kiln://` path resolves through
/// the injected resolver, and containment holds here, once, for every
/// `cru.fs` function:
///
/// - The part after the kiln name must be plain relative components — no
///   `..`, no `.`, no absolute part.
/// - The deepest existing ancestor of the result must canonicalize into the
///   canonicalized kiln root. This refuses a symlinked intermediate
///   directory, and a dangling symlink at the target.
///
/// No error here echoes a resolved directory. The scheme exists so a plugin
/// addresses a kiln it knows by NAME without learning where it lives; an
/// error that prints the root would undo that.
fn resolve_path(path: &str, resolver: Option<&KilnPathResolver>) -> Result<PathBuf, LuaError> {
    let Some(rest) = path.strip_prefix(KILN_SCHEME) else {
        return Ok(PathBuf::from(path));
    };
    let Some(resolver) = resolver else {
        return Err(LuaError::Runtime(format!(
            "'{path}': kiln:// paths are not available in this runtime"
        )));
    };
    let (name, relative) = rest.split_once('/').unwrap_or((rest, ""));
    if name.is_empty() {
        return Err(LuaError::Runtime(
            "a kiln:// path needs a kiln name: kiln://<name>/<relative>".to_string(),
        ));
    }
    let root = resolver(name).map_err(LuaError::Runtime)?;

    let mut resolved = root.clone();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            _ => {
                return Err(LuaError::Runtime(format!(
                    "'{path}': the part after the kiln name must be plain \
                     relative components — no '..', '.' or absolute part"
                )));
            }
        }
    }

    let canonical_root = fs::canonicalize(&root)
        .map_err(|e| LuaError::Runtime(format!("kiln '{name}' is not reachable: {e}")))?;
    // The deepest ancestor that exists, by lstat. A dangling symlink counts
    // as existing here, so `canonicalize` fails on it below — the loop must
    // not step past it, or a write would create the link's target.
    let mut existing = resolved.as_path();
    while existing.symlink_metadata().is_err() {
        match existing.parent() {
            Some(parent) => existing = parent,
            None => break,
        }
    }
    let canonical = fs::canonicalize(existing).map_err(|e| {
        LuaError::Runtime(format!(
            "'{path}': cannot resolve inside kiln '{name}': {e}"
        ))
    })?;
    if !canonical.starts_with(&canonical_root) {
        return Err(LuaError::Runtime(format!(
            "'{path}': resolves outside kiln '{name}'; refused"
        )));
    }
    Ok(resolved)
}

/// Create the parent directory of `path` when it does not exist.
///
/// `shown` is the caller's own spelling of the path. Error text uses it
/// instead of `path`, so a `kiln://` caller never sees the resolved
/// directory.
fn ensure_parent(path: &Path, shown: &str) -> Result<(), LuaError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                LuaError::Runtime(format!(
                    "Failed to create parent directory for '{shown}': {e}"
                ))
            })?;
        }
    }
    Ok(())
}

/// Read file contents to string
fn read_file(path: &Path) -> Result<String, LuaError> {
    fs::read_to_string(path).lua_runtime()
}

/// Write content to file (creates or overwrites)
fn write_file(path: &Path, shown: &str, content: &str) -> Result<(), LuaError> {
    ensure_parent(path, shown)?;

    fs::write(path, content).lua_runtime()
}

/// Append content to file (creates if doesn't exist)
fn append_file(path: &Path, shown: &str, content: &str) -> Result<(), LuaError> {
    ensure_parent(path, shown)?;

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .lua_runtime()?;

    file.write_all(content.as_bytes()).lua_runtime()
}

/// Create directory and all parent directories
fn mkdir(path: &Path) -> Result<(), LuaError> {
    fs::create_dir_all(path).lua_runtime()
}

/// Remove a file or directory
fn remove(path: &Path) -> Result<(), LuaError> {
    if path.is_dir() {
        fs::remove_dir_all(path).lua_runtime()
    } else {
        fs::remove_file(path).lua_runtime()
    }
}

/// List directory contents
fn list_dir(path: &Path) -> Result<Vec<String>, LuaError> {
    let entries = fs::read_dir(path).lua_runtime()?;

    let mut result = Vec::new();
    for entry in entries {
        let entry = entry.lua_runtime()?;
        if let Some(name) = entry.file_name().to_str() {
            result.push(name.to_string());
        }
    }
    Ok(result)
}

/// Copy a file
fn copy_file(src: &Path, dest: &Path, shown_dest: &str) -> Result<(), LuaError> {
    ensure_parent(dest, shown_dest)?;

    fs::copy(src, dest).lua_runtime()?;
    Ok(())
}

/// Rename/move a file or directory
fn rename_file(src: &Path, dest: &Path, shown_dest: &str) -> Result<(), LuaError> {
    ensure_parent(dest, shown_dest)?;

    fs::rename(src, dest).lua_runtime()
}

/// Register the fs module with no kiln resolver.
///
/// `kiln://` paths are refused with a clear error. A host that can map a
/// kiln name to a directory uses [`register_fs_module_with_resolver`].
pub fn register_fs_module(lua: &Lua) -> Result<(), LuaError> {
    register_fs(lua, None)
}

/// Register the fs module with a kiln-name resolver, so every `cru.fs`
/// function also accepts `kiln://<name>/<relative>` beside plain paths.
///
/// Registration replaces an earlier fs table, so a host upgrades the plain
/// registration once it can resolve names.
pub fn register_fs_module_with_resolver(
    lua: &Lua,
    resolver: KilnPathResolver,
) -> Result<(), LuaError> {
    register_fs(lua, Some(resolver))
}

/// One registration body for both entry points. Every function resolves its
/// path arguments through [`resolve_path`], so `kiln://` support and its
/// containment cannot differ between functions.
fn register_fs(lua: &Lua, resolver: Option<KilnPathResolver>) -> Result<(), LuaError> {
    let fs_table = lua.create_table()?;

    // fs.read(path) -> string
    let r = resolver.clone();
    let read_fn = lua.create_function(move |_lua, path: String| {
        let target = resolve_path(&path, r.as_ref()).map_err(mlua::Error::external)?;
        read_file(&target).map_err(mlua::Error::external)
    })?;
    fs_table.set("read", read_fn)?;

    // fs.write(path, content) -> nil
    let r = resolver.clone();
    let write_fn = lua.create_function(move |_lua, (path, content): (String, String)| {
        let target = resolve_path(&path, r.as_ref()).map_err(mlua::Error::external)?;
        write_file(&target, &path, &content).map_err(mlua::Error::external)
    })?;
    fs_table.set("write", write_fn)?;

    // fs.append(path, content) -> nil
    let r = resolver.clone();
    let append_fn = lua.create_function(move |_lua, (path, content): (String, String)| {
        let target = resolve_path(&path, r.as_ref()).map_err(mlua::Error::external)?;
        append_file(&target, &path, &content).map_err(mlua::Error::external)
    })?;
    fs_table.set("append", append_fn)?;

    // fs.mkdir(path) -> nil
    let r = resolver.clone();
    let mkdir_fn = lua.create_function(move |_lua, path: String| {
        let target = resolve_path(&path, r.as_ref()).map_err(mlua::Error::external)?;
        mkdir(&target).map_err(mlua::Error::external)
    })?;
    fs_table.set("mkdir", mkdir_fn)?;

    // fs.exists(path) -> bool
    let r = resolver.clone();
    let exists_fn = lua.create_function(move |_lua, path: String| {
        let target = resolve_path(&path, r.as_ref()).map_err(mlua::Error::external)?;
        Ok(target.exists())
    })?;
    fs_table.set("exists", exists_fn)?;

    // fs.is_file(path) -> bool
    let r = resolver.clone();
    let is_file_fn = lua.create_function(move |_lua, path: String| {
        let target = resolve_path(&path, r.as_ref()).map_err(mlua::Error::external)?;
        Ok(target.is_file())
    })?;
    fs_table.set("is_file", is_file_fn)?;

    // fs.is_dir(path) -> bool
    let r = resolver.clone();
    let is_dir_fn = lua.create_function(move |_lua, path: String| {
        let target = resolve_path(&path, r.as_ref()).map_err(mlua::Error::external)?;
        Ok(target.is_dir())
    })?;
    fs_table.set("is_dir", is_dir_fn)?;

    // fs.remove(path) -> nil
    let r = resolver.clone();
    let remove_fn = lua.create_function(move |_lua, path: String| {
        let target = resolve_path(&path, r.as_ref()).map_err(mlua::Error::external)?;
        remove(&target).map_err(mlua::Error::external)
    })?;
    fs_table.set("remove", remove_fn)?;

    // fs.list(path) -> table of strings
    let r = resolver.clone();
    let list_fn = lua.create_function(move |lua, path: String| {
        let target = resolve_path(&path, r.as_ref()).map_err(mlua::Error::external)?;
        let entries = list_dir(&target).map_err(mlua::Error::external)?;
        let table = lua.create_table()?;
        for (i, entry) in entries.into_iter().enumerate() {
            table.set(i + 1, entry)?; // Lua arrays are 1-indexed
        }
        Ok(table)
    })?;
    fs_table.set("list", list_fn)?;

    // fs.copy(src, dest) -> nil
    let r = resolver.clone();
    let copy_fn = lua.create_function(move |_lua, (src, dest): (String, String)| {
        let from = resolve_path(&src, r.as_ref()).map_err(mlua::Error::external)?;
        let to = resolve_path(&dest, r.as_ref()).map_err(mlua::Error::external)?;
        copy_file(&from, &to, &dest).map_err(mlua::Error::external)
    })?;
    fs_table.set("copy", copy_fn)?;

    // fs.rename(src, dest) -> nil
    let r = resolver.clone();
    let rename_fn = lua.create_function(move |_lua, (src, dest): (String, String)| {
        let from = resolve_path(&src, r.as_ref()).map_err(mlua::Error::external)?;
        let to = resolve_path(&dest, r.as_ref()).map_err(mlua::Error::external)?;
        rename_file(&from, &to, &dest).map_err(mlua::Error::external)
    })?;
    fs_table.set("rename", rename_fn)?;

    // Register fs module globally
    lua.globals().set("fs", fs_table.clone())?;
    crate::lua_util::register_module(lua, "fs", fs_table)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_lua() -> Lua {
        let lua = Lua::new();
        register_fs_module(&lua).unwrap();
        lua
    }

    /// A resolver that knows one kiln, `notes`, rooted at `root`.
    fn create_lua_with_kiln(root: &Path) -> Lua {
        let lua = Lua::new();
        let root = root.to_path_buf();
        let resolver: KilnPathResolver = Arc::new(move |name: &str| {
            if name == "notes" {
                Ok(root.clone())
            } else {
                Err(format!("kiln '{name}' is not registered"))
            }
        });
        register_fs_module_with_resolver(&lua, resolver).unwrap();
        lua
    }

    #[test]
    fn a_kiln_path_resolves_into_the_kiln_root() {
        let kiln = TempDir::new().unwrap();
        let lua = create_lua_with_kiln(kiln.path());

        let read: String = lua
            .load(
                r#"
                fs.mkdir("kiln://notes/.crucible/proposals")
                fs.write("kiln://notes/.crucible/proposals/p.md", "proposed")
                return fs.read("kiln://notes/.crucible/proposals/p.md")
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(read, "proposed");
        let on_disk = kiln.path().join(".crucible/proposals/p.md");
        assert_eq!(std::fs::read_to_string(on_disk).unwrap(), "proposed");
    }

    /// Containment is one check in `resolve_path`, so one traversing
    /// spelling per class is enough: a `..` component, and an absolute
    /// relative part.
    #[test]
    fn a_traversing_kiln_path_is_refused() {
        let kiln = TempDir::new().unwrap();
        let lua = create_lua_with_kiln(kiln.path());

        for bad in ["kiln://notes/../evil.md", "kiln://notes//abs.md"] {
            let err = lua
                .load(format!(r#"fs.write("{bad}", "x")"#))
                .exec()
                .expect_err("a traversing path must be refused");
            let text = err.to_string();
            assert!(
                !text.contains(&kiln.path().to_string_lossy().to_string()),
                "the error must not echo the kiln root: {text}"
            );
        }
        assert!(!kiln.path().parent().unwrap().join("evil.md").exists());
    }

    /// The name allowlist cannot see a directory that lies. A symlinked
    /// intermediate directory resolves the write outside the kiln, and the
    /// canonicalize check is what refuses it.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_directory_inside_the_kiln_is_refused() {
        let kiln = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::create_dir_all(kiln.path().join(".crucible")).unwrap();
        std::os::unix::fs::symlink(outside.path(), kiln.path().join(".crucible/proposals"))
            .unwrap();
        let lua = create_lua_with_kiln(kiln.path());

        let err = lua
            .load(r#"fs.write("kiln://notes/.crucible/proposals/p.md", "x")"#)
            .exec()
            .expect_err("a symlinked directory must be refused");
        let text = err.to_string();
        assert!(text.contains("outside kiln 'notes'"), "unhelpful: {text}");
        assert!(
            !text.contains(&outside.path().to_string_lossy().to_string()),
            "the error must not echo the resolved directory: {text}"
        );
        assert!(!outside.path().join("p.md").exists());
    }

    #[test]
    fn an_unknown_kiln_name_errors_cleanly() {
        let kiln = TempDir::new().unwrap();
        let lua = create_lua_with_kiln(kiln.path());

        let err = lua
            .load(r#"fs.write("kiln://other/x.md", "x")"#)
            .exec()
            .expect_err("an unknown kiln must be refused");
        assert!(err.to_string().contains("other"), "unhelpful: {err}");
    }

    /// A runtime with no resolver must refuse the scheme, not treat
    /// `kiln://…` as a relative directory named `kiln:` under the cwd.
    #[test]
    fn a_plain_runtime_refuses_kiln_paths_with_a_clear_error() {
        let lua = create_lua();
        let err = lua
            .load(r#"fs.write("kiln://notes/x.md", "hi")"#)
            .exec()
            .expect_err("no resolver, no kiln:// paths");
        assert!(
            err.to_string().contains("kiln://"),
            "the error must name the scheme: {err}"
        );
        assert!(!std::path::Path::new("kiln:").exists());
    }

    #[test]
    fn test_write_and_read() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        let path_str = file_path.to_string_lossy().to_string();

        let lua = create_lua();
        lua.load(format!(
            r#"
            fs.write("{}", "Hello, Lua!")
            return fs.read("{}")
            "#,
            path_str, path_str
        ))
        .eval::<String>()
        .map(|s| assert_eq!(s, "Hello, Lua!"))
        .unwrap();
    }

    #[test]
    fn test_append() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("log.txt");
        let path_str = file_path.to_string_lossy().to_string();

        let lua = create_lua();
        lua.load(format!(
            r#"
            fs.append("{0}", "Line 1\n")
            fs.append("{0}", "Line 2\n")
            return fs.read("{0}")
            "#,
            path_str
        ))
        .eval::<String>()
        .map(|s| assert_eq!(s, "Line 1\nLine 2\n"))
        .unwrap();
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
            local before = fs.exists("{0}")
            fs.mkdir("{0}")
            local after = fs.exists("{0}")
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
                file_is_file = fs.is_file("{}"),
                file_is_dir = fs.is_dir("{}"),
                dir_is_file = fs.is_file("{}"),
                dir_is_dir = fs.is_dir("{}")
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
    fn test_remove() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("remove_me.txt");
        fs::write(&file_path, "temp").unwrap();
        let path_str = file_path.to_string_lossy().to_string();

        let lua = create_lua();
        let result: Table = lua
            .load(format!(
                r#"
            local before = fs.exists("{0}")
            fs.remove("{0}")
            local after = fs.exists("{0}")
            return {{ before = before, after = after }}
            "#,
                path_str
            ))
            .eval()
            .unwrap();

        assert!(result.get::<bool>("before").unwrap());
        assert!(!result.get::<bool>("after").unwrap());
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
            .load(format!(r#"return fs.list("{}")"#, dir_path))
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
        let dest = temp.path().join("dest.txt");
        fs::write(&src, "original").unwrap();

        let lua = create_lua();
        let content: String = lua
            .load(format!(
                r#"
            fs.copy("{}", "{}")
            return fs.read("{}")
            "#,
                src.to_string_lossy(),
                dest.to_string_lossy(),
                dest.to_string_lossy()
            ))
            .eval()
            .unwrap();

        assert_eq!(content, "original");
    }

    #[test]
    fn test_rename() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("old.txt");
        let dest = temp.path().join("new.txt");
        fs::write(&src, "content").unwrap();

        let lua = create_lua();
        let result: Table = lua
            .load(format!(
                r#"
            fs.rename("{}", "{}")
            return {{ old = fs.exists("{}"), new = fs.exists("{}") }}
            "#,
                src.to_string_lossy(),
                dest.to_string_lossy(),
                src.to_string_lossy(),
                dest.to_string_lossy()
            ))
            .eval()
            .unwrap();

        assert!(!result.get::<bool>("old").unwrap());
        assert!(result.get::<bool>("new").unwrap());
    }

    #[test]
    fn test_write_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("nested/path/to/file.txt");
        let path_str = file_path.to_string_lossy().to_string();

        let lua = create_lua();
        let content: String = lua
            .load(format!(
                r#"
            fs.write("{0}", "nested content")
            return fs.read("{0}")
            "#,
                path_str
            ))
            .eval()
            .unwrap();

        assert_eq!(content, "nested content");
    }
}
