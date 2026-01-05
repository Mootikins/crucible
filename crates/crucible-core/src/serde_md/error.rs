//! Error types for markdown serialization

use serde::ser;
use std::fmt::{self, Display};

/// Errors that can occur during Markdown serialization
#[derive(Debug)]
pub enum Error {
    /// Custom error message
    Message(String),
    /// Formatting error
    Fmt(fmt::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Message(msg) => write!(f, "{msg}"),
            Error::Fmt(e) => write!(f, "format error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl From<fmt::Error> for Error {
    fn from(e: fmt::Error) -> Self {
        Error::Fmt(e)
    }
}

/// Result type for Markdown serialization
pub type Result<T> = std::result::Result<T, Error>;
