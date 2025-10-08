//! Error types for the Obsidian HTTP client

use thiserror::Error;

/// Errors that can occur when interacting with the Obsidian plugin API
#[derive(Debug, Error)]
pub enum ObsidianError {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    /// Server returned an error status
    #[error("HTTP {status}: {message}")]
    HttpError { status: u16, message: String },

    /// JSON serialization/deserialization failed
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// Invalid response from server
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Timeout occurred
    #[error("Request timeout")]
    Timeout,

    /// Too many retry attempts
    #[error("Too many retry attempts")]
    TooManyRetries,

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Other errors
    #[error("Obsidian client error: {0}")]
    Other(String),
}

/// Result type for Obsidian client operations
pub type Result<T> = std::result::Result<T, ObsidianError>;
