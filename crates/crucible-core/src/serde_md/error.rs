//! Error types for markdown serialization

use serde::ser;
use std::fmt::Display;
use thiserror::Error;

/// Errors that can occur during Markdown serialization
#[derive(Error, Debug)]
pub enum Error {
    /// Custom error message
    #[error("{0}")]
    Message(String),
    /// Formatting error
    #[error("format error: {0}")]
    Fmt(#[from] std::fmt::Error),
}
impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

/// Result type for Markdown serialization
pub type Result<T> = std::result::Result<T, Error>;
