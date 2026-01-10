//! Paths module for Rune scripts
//!
//! Provides functions to get standard Crucible paths.
//!
//! # Example
//!
//! ```rune
//! use paths::{kiln, session, workspace};
//!
//! // Get the kiln root directory
//! let kiln_path = paths::kiln()?;
//!
//! // Get the current session directory
//! let session_path = paths::session()?;
//!
//! // Get the workspace directory
//! let workspace_path = paths::workspace()?;
//! ```

use rune::{Any, ContextError, Module};
use std::path::PathBuf;
use std::sync::Arc;

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

/// Error type for paths operations (Rune-compatible)
#[derive(Debug, Clone, Any)]
#[rune(item = ::paths, name = PathsError)]
pub struct RunePathsError {
    /// Error message
    #[rune(get)]
    pub message: String,
}

impl std::fmt::Display for RunePathsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Create the paths module for Rune with configured context
///
/// # Example
///
/// ```rust
/// use crucible_rune::paths_module;
/// use crucible_rune::PathsContext;
/// use std::path::PathBuf;
///
/// let ctx = PathsContext::new()
///     .with_kiln(PathBuf::from("/home/user/notes"))
///     .with_session(PathBuf::from("/home/user/notes/.crucible/sessions/abc123"));
///
/// let module = paths_module(ctx).unwrap();
/// ```
pub fn paths_module(context: PathsContext) -> Result<Module, ContextError> {
    let mut module = Module::with_crate("paths")?;

    // Register the error type
    module.ty::<RunePathsError>()?;

    let ctx = Arc::new(context);

    // paths::kiln() -> Result<String, PathsError>
    let ctx_kiln = ctx.clone();
    module
        .function("kiln", move || -> Result<String, RunePathsError> {
            ctx_kiln
                .kiln
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .ok_or_else(|| RunePathsError {
                    message: "Kiln path not configured".to_string(),
                })
        })
        .build()?;

    // paths::session() -> Result<String, PathsError>
    let ctx_session = ctx.clone();
    module
        .function("session", move || -> Result<String, RunePathsError> {
            ctx_session
                .session
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .ok_or_else(|| RunePathsError {
                    message: "Session path not configured".to_string(),
                })
        })
        .build()?;

    // paths::workspace() -> Result<String, PathsError>
    let ctx_workspace = ctx.clone();
    module
        .function("workspace", move || -> Result<String, RunePathsError> {
            ctx_workspace
                .workspace
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .ok_or_else(|| RunePathsError {
                    message: "Workspace path not configured".to_string(),
                })
        })
        .build()?;

    // paths::join(base, parts...) -> String
    // Helper to join path components
    module
        .function(
            "join",
            |base: String, parts: rune::alloc::Vec<String>| -> String {
                let mut path = PathBuf::from(base);
                for part in parts {
                    path.push(part);
                }
                path.to_string_lossy().to_string()
            },
        )
        .build()?;

    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn create_test_context(
        ctx: PathsContext,
    ) -> (rune::Context, Arc<rune::runtime::RuntimeContext>) {
        let mut context = rune::Context::with_default_modules().unwrap();
        context.install(paths_module(ctx).unwrap()).unwrap();
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
    fn test_paths_module_creation() {
        let module = paths_module(PathsContext::new());
        assert!(module.is_ok(), "Should create paths module");
    }

    #[test]
    fn test_kiln_path() {
        let ctx = PathsContext::new().with_kiln(PathBuf::from("/home/user/notes"));
        let (context, runtime) = create_test_context(ctx);

        let script = r#"
            use paths::kiln;

            pub fn main() {
                kiln()?
            }
        "#;

        let result = run_rune_script(&context, runtime, script);
        let path: String = rune::from_value(result).unwrap();
        assert_eq!(path, "/home/user/notes");
    }

    #[test]
    fn test_session_path() {
        let ctx = PathsContext::new().with_session(PathBuf::from("/home/user/notes/.crucible/sessions/abc123"));
        let (context, runtime) = create_test_context(ctx);

        let script = r#"
            use paths::session;

            pub fn main() {
                session()?
            }
        "#;

        let result = run_rune_script(&context, runtime, script);
        let path: String = rune::from_value(result).unwrap();
        assert_eq!(path, "/home/user/notes/.crucible/sessions/abc123");
    }

    #[test]
    fn test_workspace_path() {
        let ctx = PathsContext::new().with_workspace(PathBuf::from("/home/user/projects/myproject"));
        let (context, runtime) = create_test_context(ctx);

        let script = r#"
            use paths::workspace;

            pub fn main() {
                workspace()?
            }
        "#;

        let result = run_rune_script(&context, runtime, script);
        let path: String = rune::from_value(result).unwrap();
        assert_eq!(path, "/home/user/projects/myproject");
    }

    #[test]
    fn test_path_join() {
        let ctx = PathsContext::new().with_kiln(PathBuf::from("/home/user/notes"));
        let (context, runtime) = create_test_context(ctx);

        let script = r#"
            use paths::{kiln, join};

            pub fn main() {
                let base = kiln()?;
                join(base, ["plugins", "my_plugin.rn"])
            }
        "#;

        let result = run_rune_script(&context, runtime, script);
        let path: String = rune::from_value(result).unwrap();
        assert_eq!(path, "/home/user/notes/plugins/my_plugin.rn");
    }

    #[test]
    fn test_missing_path_error() {
        let ctx = PathsContext::new(); // No paths configured
        let (context, runtime) = create_test_context(ctx);

        let script = r#"
            use paths::kiln;

            pub fn main() {
                kiln()
            }
        "#;

        let result = run_rune_script(&context, runtime, script);
        // Result should be Err
        let res: Result<String, RunePathsError> = rune::from_value(result).unwrap();
        assert!(res.is_err());
        assert!(res.unwrap_err().message.contains("not configured"));
    }
}
