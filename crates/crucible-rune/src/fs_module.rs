//! File system module for Rune
//!
//! Provides file system operations for Rune scripts.
//! Builds on rune-modules fs (read_to_string) and adds write operations.
//!
//! # Example
//!
//! ```rune
//! use fs::{read_to_string, write_file, append_file, mkdir, exists, remove};
//!
//! // Read a file
//! let content = read_to_string("input.txt")?;
//!
//! // Write to a file (creates or overwrites)
//! write_file("output.txt", "Hello, world!")?;
//!
//! // Append to a file
//! append_file("log.txt", "New log entry\n")?;
//!
//! // Create directory (with parents)
//! mkdir("path/to/new/dir")?;
//!
//! // Check if path exists
//! if exists("config.toml") {
//!     // ...
//! }
//!
//! // Remove a file
//! remove("temp.txt")?;
//! ```

use rune::{Any, ContextError, Module};
use std::fs;
use std::io::Write;
use std::path::Path;

/// Error type for fs operations (Rune-compatible)
#[derive(Debug, Clone, Any)]
#[rune(item = ::fs, name = FsError)]
pub struct RuneFsError {
    /// Error message
    #[rune(get)]
    pub message: String,
}

impl std::fmt::Display for RuneFsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Read file contents to string
fn read_to_string_impl(path: String) -> Result<String, RuneFsError> {
    fs::read_to_string(&path).map_err(|e| RuneFsError {
        message: format!("Failed to read '{}': {}", path, e),
    })
}

/// Write content to file (creates or overwrites)
fn write_file_impl(path: String, content: String) -> Result<(), RuneFsError> {
    // Ensure parent directory exists
    if let Some(parent) = Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| RuneFsError {
                message: format!("Failed to create parent directory for '{}': {}", path, e),
            })?;
        }
    }

    fs::write(&path, &content).map_err(|e| RuneFsError {
        message: format!("Failed to write '{}': {}", path, e),
    })
}

/// Append content to file (creates if doesn't exist)
fn append_file_impl(path: String, content: String) -> Result<(), RuneFsError> {
    // Ensure parent directory exists
    if let Some(parent) = Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| RuneFsError {
                message: format!("Failed to create parent directory for '{}': {}", path, e),
            })?;
        }
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| RuneFsError {
            message: format!("Failed to open '{}' for append: {}", path, e),
        })?;

    file.write_all(content.as_bytes()).map_err(|e| RuneFsError {
        message: format!("Failed to append to '{}': {}", path, e),
    })
}

/// Create directory and all parent directories
fn mkdir_impl(path: String) -> Result<(), RuneFsError> {
    fs::create_dir_all(&path).map_err(|e| RuneFsError {
        message: format!("Failed to create directory '{}': {}", path, e),
    })
}

/// Check if a path exists
fn exists_impl(path: String) -> bool {
    Path::new(&path).exists()
}

/// Check if path is a file
fn is_file_impl(path: String) -> bool {
    Path::new(&path).is_file()
}

/// Check if path is a directory
fn is_dir_impl(path: String) -> bool {
    Path::new(&path).is_dir()
}

/// Remove a file
fn remove_impl(path: String) -> Result<(), RuneFsError> {
    let p = Path::new(&path);
    if p.is_dir() {
        fs::remove_dir_all(&path).map_err(|e| RuneFsError {
            message: format!("Failed to remove directory '{}': {}", path, e),
        })
    } else {
        fs::remove_file(&path).map_err(|e| RuneFsError {
            message: format!("Failed to remove file '{}': {}", path, e),
        })
    }
}

/// List directory contents
fn list_dir_impl(path: String) -> Result<rune::alloc::Vec<String>, RuneFsError> {
    let entries = fs::read_dir(&path).map_err(|e| RuneFsError {
        message: format!("Failed to read directory '{}': {}", path, e),
    })?;

    let mut result = rune::alloc::Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| RuneFsError {
            message: format!("Failed to read entry in '{}': {}", path, e),
        })?;
        if let Some(name) = entry.file_name().to_str() {
            let _ = result.try_push(name.to_string());
        }
    }
    Ok(result)
}

/// Copy a file
fn copy_impl(src: String, dest: String) -> Result<(), RuneFsError> {
    // Ensure parent directory of dest exists
    if let Some(parent) = Path::new(&dest).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| RuneFsError {
                message: format!("Failed to create parent directory for '{}': {}", dest, e),
            })?;
        }
    }

    fs::copy(&src, &dest).map_err(|e| RuneFsError {
        message: format!("Failed to copy '{}' to '{}': {}", src, dest, e),
    })?;
    Ok(())
}

/// Rename/move a file or directory
fn rename_impl(src: String, dest: String) -> Result<(), RuneFsError> {
    // Ensure parent directory of dest exists
    if let Some(parent) = Path::new(&dest).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| RuneFsError {
                message: format!("Failed to create parent directory for '{}': {}", dest, e),
            })?;
        }
    }

    fs::rename(&src, &dest).map_err(|e| RuneFsError {
        message: format!("Failed to rename '{}' to '{}': {}", src, dest, e),
    })
}

/// Create the fs module for Rune
///
/// # Example
///
/// ```rust
/// use crucible_rune::fs_module;
///
/// let module = fs_module().unwrap();
/// ```
pub fn fs_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("fs")?;

    // Register the error type
    module.ty::<RuneFsError>()?;

    // Register functions
    module.function("read_to_string", read_to_string_impl).build()?;
    module.function("write_file", write_file_impl).build()?;
    module.function("append_file", append_file_impl).build()?;
    module.function("mkdir", mkdir_impl).build()?;
    module.function("exists", exists_impl).build()?;
    module.function("is_file", is_file_impl).build()?;
    module.function("is_dir", is_dir_impl).build()?;
    module.function("remove", remove_impl).build()?;
    module.function("list_dir", list_dir_impl).build()?;
    module.function("copy", copy_impl).build()?;
    module.function("rename", rename_impl).build()?;

    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_context() -> (rune::Context, Arc<rune::runtime::RuntimeContext>) {
        let mut context = rune::Context::with_default_modules().unwrap();
        context.install(fs_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());
        (context, runtime)
    }

    fn run_rune_script(
        context: &rune::Context,
        runtime: Arc<rune::runtime::RuntimeContext>,
        script: &str,
    ) -> rune::Value {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Diagnostics, Source, Sources, Vm};

        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        let unit = result.expect("Should compile");
        let unit = Arc::new(unit);

        let mut vm = Vm::new(runtime, unit);
        vm.call(rune::Hash::type_hash(["main"]), ()).unwrap()
    }

    #[test]
    fn test_fs_module_creation() {
        let module = fs_module();
        assert!(module.is_ok(), "Should create fs module");
    }

    #[test]
    fn test_write_and_read_file() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        let path_str = file_path.to_string_lossy();

        let (context, runtime) = create_test_context();
        let script = format!(
            r#"
            use fs::{{write_file, read_to_string}};

            pub fn main() {{
                write_file("{}", "Hello, Rune!")?;
                read_to_string("{}")?
            }}
            "#,
            path_str, path_str
        );

        let result = run_rune_script(&context, runtime, &script);
        let content: String = rune::from_value(result).unwrap();
        assert_eq!(content, "Hello, Rune!");
    }

    #[test]
    fn test_append_file() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("log.txt");
        let path_str = file_path.to_string_lossy();

        let (context, runtime) = create_test_context();
        let script = format!(
            r#"
            use fs::{{append_file, read_to_string}};

            pub fn main() {{
                append_file("{}", "Line 1\n")?;
                append_file("{}", "Line 2\n")?;
                read_to_string("{}")?
            }}
            "#,
            path_str, path_str, path_str
        );

        let result = run_rune_script(&context, runtime, &script);
        let content: String = rune::from_value(result).unwrap();
        assert_eq!(content, "Line 1\nLine 2\n");
    }

    #[test]
    fn test_mkdir_and_exists() {
        let temp = TempDir::new().unwrap();
        let dir_path = temp.path().join("nested/dirs/here");
        let path_str = dir_path.to_string_lossy();

        let (context, runtime) = create_test_context();
        let script = format!(
            r#"
            use fs::{{mkdir, exists}};

            pub fn main() {{
                let before = exists("{}");
                mkdir("{}")?;
                let after = exists("{}");
                [before, after]
            }}
            "#,
            path_str, path_str, path_str
        );

        let result = run_rune_script(&context, runtime, &script);
        let values: rune::runtime::Vec = rune::from_value(result).unwrap();
        let before: bool = rune::from_value(values.get(0).unwrap().clone()).unwrap();
        let after: bool = rune::from_value(values.get(1).unwrap().clone()).unwrap();

        assert!(!before, "Directory should not exist before mkdir");
        assert!(after, "Directory should exist after mkdir");
    }

    #[test]
    fn test_is_file_and_is_dir() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("file.txt");
        let dir_path = temp.path().join("dir");

        fs::write(&file_path, "content").unwrap();
        fs::create_dir(&dir_path).unwrap();

        let (context, runtime) = create_test_context();
        let script = format!(
            r#"
            use fs::{{is_file, is_dir}};

            pub fn main() {{
                [is_file("{}"), is_dir("{}"), is_file("{}"), is_dir("{}")]
            }}
            "#,
            file_path.to_string_lossy(),
            file_path.to_string_lossy(),
            dir_path.to_string_lossy(),
            dir_path.to_string_lossy()
        );

        let result = run_rune_script(&context, runtime, &script);
        let values: rune::runtime::Vec = rune::from_value(result).unwrap();

        let file_is_file: bool = rune::from_value(values.get(0).unwrap().clone()).unwrap();
        let file_is_dir: bool = rune::from_value(values.get(1).unwrap().clone()).unwrap();
        let dir_is_file: bool = rune::from_value(values.get(2).unwrap().clone()).unwrap();
        let dir_is_dir: bool = rune::from_value(values.get(3).unwrap().clone()).unwrap();

        assert!(file_is_file, "file.txt should be a file");
        assert!(!file_is_dir, "file.txt should not be a dir");
        assert!(!dir_is_file, "dir should not be a file");
        assert!(dir_is_dir, "dir should be a dir");
    }

    #[test]
    fn test_remove() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("remove_me.txt");
        fs::write(&file_path, "temp").unwrap();
        let path_str = file_path.to_string_lossy();

        let (context, runtime) = create_test_context();
        let script = format!(
            r#"
            use fs::{{remove, exists}};

            pub fn main() {{
                let before = exists("{}");
                remove("{}")?;
                let after = exists("{}");
                [before, after]
            }}
            "#,
            path_str, path_str, path_str
        );

        let result = run_rune_script(&context, runtime, &script);
        let values: rune::runtime::Vec = rune::from_value(result).unwrap();
        let before: bool = rune::from_value(values.get(0).unwrap().clone()).unwrap();
        let after: bool = rune::from_value(values.get(1).unwrap().clone()).unwrap();

        assert!(before, "File should exist before remove");
        assert!(!after, "File should not exist after remove");
    }

    #[test]
    fn test_list_dir() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "").unwrap();
        fs::write(temp.path().join("b.txt"), "").unwrap();
        fs::create_dir(temp.path().join("c")).unwrap();
        let dir_path = temp.path().to_string_lossy();

        let (context, runtime) = create_test_context();
        let script = format!(
            r#"
            use fs::list_dir;

            pub fn main() {{
                list_dir("{}")?
            }}
            "#,
            dir_path
        );

        let result = run_rune_script(&context, runtime, &script);
        let entries: rune::alloc::Vec<String> = rune::from_value(result).unwrap();

        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|e| e == "a.txt"));
        assert!(entries.iter().any(|e| e == "b.txt"));
        assert!(entries.iter().any(|e| e == "c"));
    }

    #[test]
    fn test_copy() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src.txt");
        let dest = temp.path().join("dest.txt");
        fs::write(&src, "original").unwrap();

        let (context, runtime) = create_test_context();
        let script = format!(
            r#"
            use fs::{{copy, read_to_string}};

            pub fn main() {{
                copy("{}", "{}")?;
                read_to_string("{}")?
            }}
            "#,
            src.to_string_lossy(),
            dest.to_string_lossy(),
            dest.to_string_lossy()
        );

        let result = run_rune_script(&context, runtime, &script);
        let content: String = rune::from_value(result).unwrap();
        assert_eq!(content, "original");
    }

    #[test]
    fn test_rename() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("old.txt");
        let dest = temp.path().join("new.txt");
        fs::write(&src, "content").unwrap();

        let (context, runtime) = create_test_context();
        let script = format!(
            r#"
            use fs::{{rename, exists}};

            pub fn main() {{
                rename("{}", "{}")?;
                [exists("{}"), exists("{}")]
            }}
            "#,
            src.to_string_lossy(),
            dest.to_string_lossy(),
            src.to_string_lossy(),
            dest.to_string_lossy()
        );

        let result = run_rune_script(&context, runtime, &script);
        let values: rune::runtime::Vec = rune::from_value(result).unwrap();
        let old_exists: bool = rune::from_value(values.get(0).unwrap().clone()).unwrap();
        let new_exists: bool = rune::from_value(values.get(1).unwrap().clone()).unwrap();

        assert!(!old_exists, "Old file should not exist after rename");
        assert!(new_exists, "New file should exist after rename");
    }

    #[test]
    fn test_write_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("nested/path/to/file.txt");
        let path_str = file_path.to_string_lossy();

        let (context, runtime) = create_test_context();
        let script = format!(
            r#"
            use fs::{{write_file, read_to_string}};

            pub fn main() {{
                write_file("{}", "nested content")?;
                read_to_string("{}")?
            }}
            "#,
            path_str, path_str
        );

        let result = run_rune_script(&context, runtime, &script);
        let content: String = rune::from_value(result).unwrap();
        assert_eq!(content, "nested content");
    }
}
