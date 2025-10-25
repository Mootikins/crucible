//! Error types for the file watching system.

use thiserror::Error;

/// Errors that can occur during file watching operations.
#[derive(Error, Debug)]
pub enum Error {
    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// File system watching error.
    #[error("File watching error: {0}")]
    Watch(String),

    /// IO error during file operations.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Pattern matching error.
    #[error("Pattern error: {0}")]
    Pattern(String),

    /// Event handling error.
    #[error("Event handling error: {0}")]
    Handler(String),

    /// Backend not available.
    #[error("Backend '{0}' is not available")]
    BackendUnavailable(String),

    /// Watch already exists.
    #[error("Watch for path '{0}' already exists")]
    WatchExists(String),

    /// Watch not found.
    #[error("Watch for path '{0}' not found")]
    WatchNotFound(String),

    /// Manager is not running.
    #[error("Watch manager is not running")]
    NotRunning,

    /// Manager is already running.
    #[error("Watch manager is already running")]
    AlreadyRunning,

    /// Event queue is full.
    #[error("Event queue is full (capacity: {0})")]
    QueueFull(usize),

    /// Invalid path.
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    /// Permission denied.
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Timeout occurred.
    #[error("Operation timed out after {0}ms")]
    Timeout(u64),

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// Parser error.
    #[error("Parser error: {0}")]
    Parser(String),

    /// Embedding error.
    #[error("Embedding error: {0}")]
    Embedding(String),

    /// Channel error.
    #[error("Channel error: {0}")]
    Channel(String),

    /// Other error.
    #[error("Other error: {0}")]
    Other(String),
}

/// Result type for file watching operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Convert notify errors to our error type.
impl From<notify::Error> for Error {
    fn from(err: notify::Error) -> Self {
        Error::Watch(err.to_string())
    }
}

/// Convert flume send errors to our error type.
impl<T> From<flume::SendError<T>> for Error {
    fn from(err: flume::SendError<T>) -> Self {
        Error::Internal(format!("Channel send error: {}", err))
    }
}

/// Convert globset errors to our error type.
impl From<globset::Error> for Error {
    fn from(err: globset::Error) -> Self {
        Error::Pattern(err.to_string())
    }
}

/// Convert anyhow errors to our error type.
impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Embedding(err.to_string())
    }
}