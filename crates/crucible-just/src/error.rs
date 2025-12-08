//! Error types for crucible-just

use thiserror::Error;

#[derive(Error, Debug)]
pub enum JustError {
    #[error("Failed to parse justfile JSON: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("Failed to execute just command: {0}")]
    CommandError(String),

    #[error("Recipe not found: {0}")]
    RecipeNotFound(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, JustError>;
