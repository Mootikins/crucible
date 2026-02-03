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
use mlua::Lua;
#[cfg(test)]
use mlua::Table;
use std::fs;
use std::io::Write;
use std::path::Path;

/// Read file contents to string
fn read_file(path: &str) -> Result<String, LuaError> {
    fs::read_to_string(path)
        .map_err(|e| LuaError::Runtime(format!("Failed to read '{}': {}", path, e)))
}

/// Write content to file (creates or overwrites)
fn write_file(path: &str, content: &str) -> Result<(), LuaError> {
    // Ensure parent directory exists
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                LuaError::Runtime(format!(
                    "Failed to create parent directory for '{}': {}",
                    path, e
                ))
            })?;
        }
    }

    fs::write(path, content)
        .map_err(|e| LuaError::Runtime(format!("Failed to write '{}': {}", path, e)))
}

/// Append content to file (creates if doesn't exist)
fn append_file(path: &str, content: &str) -> Result<(), LuaError> {
    // Ensure parent directory exists
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                LuaError::Runtime(format!(
                    "Failed to create parent directory for '{}': {}",
                    path, e
                ))
            })?;
        }
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| LuaError::Runtime(format!("Failed to open '{}' for append: {}", path, e)))?;

    file.write_all(content.as_bytes())
        .map_err(|e| LuaError::Runtime(format!("Failed to append to '{}': {}", path, e)))
}

/// Create directory and all parent directories
fn mkdir(path: &str) -> Result<(), LuaError> {
    fs::create_dir_all(path)
        .map_err(|e| LuaError::Runtime(format!("Failed to create directory '{}': {}", path, e)))
}

/// Remove a file or directory
fn remove(path: &str) -> Result<(), LuaError> {
    let p = Path::new(path);
    if p.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|e| LuaError::Runtime(format!("Failed to remove directory '{}': {}", path, e)))
    } else {
        fs::remove_file(path)
            .map_err(|e| LuaError::Runtime(format!("Failed to remove file '{}': {}", path, e)))
    }
}

/// List directory contents
fn list_dir(path: &str) -> Result<Vec<String>, LuaError> {
    let entries = fs::read_dir(path)
        .map_err(|e| LuaError::Runtime(format!("Failed to read directory '{}': {}", path, e)))?;

    let mut result = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|e| LuaError::Runtime(format!("Failed to read entry in '{}': {}", path, e)))?;
        if let Some(name) = entry.file_name().to_str() {
            result.push(name.to_string());
        }
    }
    Ok(result)
}

/// Copy a file
fn copy_file(src: &str, dest: &str) -> Result<(), LuaError> {
    // Ensure parent directory of dest exists
    if let Some(parent) = Path::new(dest).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                LuaError::Runtime(format!(
                    "Failed to create parent directory for '{}': {}",
                    dest, e
                ))
            })?;
        }
    }

    fs::copy(src, dest)
        .map_err(|e| LuaError::Runtime(format!("Failed to copy '{}' to '{}': {}", src, dest, e)))?;
    Ok(())
}

/// Rename/move a file or directory
fn rename_file(src: &str, dest: &str) -> Result<(), LuaError> {
    // Ensure parent directory of dest exists
    if let Some(parent) = Path::new(dest).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                LuaError::Runtime(format!(
                    "Failed to create parent directory for '{}': {}",
                    dest, e
                ))
            })?;
        }
    }

    fs::rename(src, dest)
        .map_err(|e| LuaError::Runtime(format!("Failed to rename '{}' to '{}': {}", src, dest, e)))
}

/// Register the fs module with a Lua state
pub fn register_fs_module(lua: &Lua) -> Result<(), LuaError> {
    let fs_table = lua.create_table()?;

    // fs.read(path) -> string
    let read_fn =
        lua.create_function(|_lua, path: String| read_file(&path).map_err(mlua::Error::external))?;
    fs_table.set("read", read_fn)?;

    // fs.write(path, content) -> nil
    let write_fn = lua.create_function(|_lua, (path, content): (String, String)| {
        write_file(&path, &content).map_err(mlua::Error::external)
    })?;
    fs_table.set("write", write_fn)?;

    // fs.append(path, content) -> nil
    let append_fn = lua.create_function(|_lua, (path, content): (String, String)| {
        append_file(&path, &content).map_err(mlua::Error::external)
    })?;
    fs_table.set("append", append_fn)?;

    // fs.mkdir(path) -> nil
    let mkdir_fn =
        lua.create_function(|_lua, path: String| mkdir(&path).map_err(mlua::Error::external))?;
    fs_table.set("mkdir", mkdir_fn)?;

    // fs.exists(path) -> bool
    let exists_fn = lua.create_function(|_lua, path: String| Ok(Path::new(&path).exists()))?;
    fs_table.set("exists", exists_fn)?;

    // fs.is_file(path) -> bool
    let is_file_fn = lua.create_function(|_lua, path: String| Ok(Path::new(&path).is_file()))?;
    fs_table.set("is_file", is_file_fn)?;

    // fs.is_dir(path) -> bool
    let is_dir_fn = lua.create_function(|_lua, path: String| Ok(Path::new(&path).is_dir()))?;
    fs_table.set("is_dir", is_dir_fn)?;

    // fs.remove(path) -> nil
    let remove_fn =
        lua.create_function(|_lua, path: String| remove(&path).map_err(mlua::Error::external))?;
    fs_table.set("remove", remove_fn)?;

    // fs.list(path) -> table of strings
    let list_fn = lua.create_function(|lua, path: String| {
        let entries = list_dir(&path).map_err(mlua::Error::external)?;
        let table = lua.create_table()?;
        for (i, entry) in entries.into_iter().enumerate() {
            table.set(i + 1, entry)?; // Lua arrays are 1-indexed
        }
        Ok(table)
    })?;
    fs_table.set("list", list_fn)?;

    // fs.copy(src, dest) -> nil
    let copy_fn = lua.create_function(|_lua, (src, dest): (String, String)| {
        copy_file(&src, &dest).map_err(mlua::Error::external)
    })?;
    fs_table.set("copy", copy_fn)?;

    // fs.rename(src, dest) -> nil
    let rename_fn = lua.create_function(|_lua, (src, dest): (String, String)| {
        rename_file(&src, &dest).map_err(mlua::Error::external)
    })?;
    fs_table.set("rename", rename_fn)?;

    // Register fs module globally
    lua.globals().set("fs", fs_table.clone())?;
    crate::lua_util::register_in_namespaces(lua, "fs", fs_table)?;

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

    #[test]
    fn test_write_and_read() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        let path_str = file_path.to_string_lossy().to_string();

        let lua = create_lua();
        lua.load(&format!(
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
        lua.load(&format!(
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
            .load(&format!(
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
            .load(&format!(
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
            .load(&format!(
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
            .load(&format!(r#"return fs.list("{}")"#, dir_path))
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
            .load(&format!(
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
            .load(&format!(
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
            .load(&format!(
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
