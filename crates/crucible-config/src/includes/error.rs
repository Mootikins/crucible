use std::path::PathBuf;

/// Errors that can occur during include processing
#[derive(Debug, Clone, thiserror::Error)]
pub enum IncludeError {
    /// Include file not found
    #[error("Include file not found: {}", .0.display())]
    FileNotFound(PathBuf),

    /// Include directory not found
    #[error("Include directory not found: {}", .0.display())]
    DirNotFound(PathBuf),

    /// Path is not a directory
    #[error("Path is not a directory: {}", .0.display())]
    NotADirectory(PathBuf),

    /// IO error reading include file
    #[error("IO error reading {}: {}", path.display(), error)]
    Io {
        /// Path to the file
        path: PathBuf,
        /// Error message
        error: String,
    },

    /// Parse error in include file
    #[error("Parse error in {}: {}", path.display(), error)]
    Parse {
        /// Path to the file
        path: PathBuf,
        /// Error message
        error: String,
    },

    /// Environment variable not found
    #[error("Environment variable not found: {var_name} (referenced as {{env:{var_name}}})")]
    EnvVarNotFound {
        /// Name of the environment variable
        var_name: String,
    },
}
