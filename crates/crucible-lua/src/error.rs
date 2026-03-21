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

/// Result type for Lua operations
pub type LuaResult<T> = Result<T, LuaError>;

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

/// Format an mlua error for user display, stripping Rust FFI frames.
///
/// When `plugin_name` is provided, the error is prefixed with the plugin name
/// for quick identification in logs and notifications.
pub fn format_lua_error(plugin_name: Option<&str>, err: &mlua::Error) -> String {
    let raw = err.to_string();

    // Filter out Rust FFI stack frames that are noise for plugin authors
    let cleaned: String = raw
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("[C]:")
                && !trimmed.contains("mlua::")
                && !trimmed.contains("[rust]")
                && !trimmed.starts_with("stack traceback:")
                // Skip empty "in ?" frames from internal transitions
                && (!trimmed.contains("in ?") || trimmed.contains(".lua"))
        })
        .collect::<Vec<_>>()
        .join("\n");

    let message = if cleaned.trim().is_empty() {
        &raw
    } else {
        cleaned.trim()
    };

    match plugin_name {
        Some(name) => format!("[{}] {}", name, message),
        None => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_lua_error_strips_ffi_frames() {
        let err = mlua::Error::RuntimeError(
            "init.lua:42: attempt to index a nil value\nstack traceback:\n\t[C]: in ?\n\tinit.lua:42: in function 'setup'\n\tmlua::lua: in ?\n\t[rust]: in ?".to_string()
        );
        let formatted = format_lua_error(Some("discord"), &err);
        assert!(
            formatted.starts_with("[discord]"),
            "should have plugin prefix: {}",
            formatted
        );
        assert!(
            formatted.contains("init.lua:42"),
            "should keep Lua frame: {}",
            formatted
        );
        assert!(
            !formatted.contains("[C]:"),
            "should strip C frames: {}",
            formatted
        );
        assert!(
            !formatted.contains("mlua::"),
            "should strip mlua frames: {}",
            formatted
        );
        assert!(
            !formatted.contains("[rust]"),
            "should strip rust frames: {}",
            formatted
        );
    }

    #[test]
    fn format_lua_error_no_plugin_name() {
        let err = mlua::Error::RuntimeError("test.lua:1: bad argument".to_string());
        let formatted = format_lua_error(None, &err);
        assert!(
            !formatted.starts_with("["),
            "should have no prefix: {}",
            formatted
        );
        assert!(
            formatted.contains("test.lua:1"),
            "should keep error location: {}",
            formatted
        );
    }

    #[test]
    fn format_lua_error_preserves_simple_errors() {
        let err = mlua::Error::RuntimeError("attempt to call a nil value".to_string());
        let formatted = format_lua_error(Some("myplugin"), &err);
        assert_eq!(
            formatted,
            "[myplugin] runtime error: attempt to call a nil value"
        );
    }
}
