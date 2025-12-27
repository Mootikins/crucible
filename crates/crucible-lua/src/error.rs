//! Error types for Lua scripting

use thiserror::Error;

/// Errors that can occur in the Lua scripting system
#[derive(Error, Debug)]
pub enum LuaError {
    /// Lua runtime error
    #[error("Lua error: {0}")]
    Runtime(String),

    /// Fennel compilation error
    #[error("Fennel compile error: {0}")]
    FennelCompile(String),

    /// Tool not found
    #[error("Tool not found: {0}")]
    NotFound(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Invalid tool definition
    #[error("Invalid tool definition: {0}")]
    InvalidTool(String),
}

impl From<mlua::Error> for LuaError {
    fn from(e: mlua::Error) -> Self {
        LuaError::Runtime(e.to_string())
    }
}

impl From<serde_json::Error> for LuaError {
    fn from(e: serde_json::Error) -> Self {
        LuaError::Serialization(e.to_string())
    }
}
