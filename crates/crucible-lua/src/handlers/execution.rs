use crate::error::LuaError;
use crucible_core::events::SessionEvent;
use mlua::Lua;
use tracing::debug;

use super::script_handler::LuaScriptHandler;

/// Result of handler execution (legacy compatibility)
#[derive(Debug, Clone)]
pub struct HandlerExecutionResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Modified event if any
    pub event: Option<SessionEvent>,
    /// Error message if failed
    pub error: Option<String>,
    /// Handler name for logging
    pub handler_name: String,
}

impl HandlerExecutionResult {
    /// Create a successful result with modified event
    pub fn ok(handler_name: impl Into<String>, event: SessionEvent) -> Self {
        Self {
            success: true,
            event: Some(event),
            error: None,
            handler_name: handler_name.into(),
        }
    }

    /// Create a successful result with no modification (pass-through)
    pub fn pass_through(handler_name: impl Into<String>) -> Self {
        Self {
            success: true,
            event: None,
            error: None,
            handler_name: handler_name.into(),
        }
    }

    /// Create a failed result
    pub fn err(handler_name: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            success: false,
            event: None,
            error: Some(error.into()),
            handler_name: handler_name.into(),
        }
    }
}

/// Execute a handler and return a structured result
pub fn execute_handler(
    handler: &LuaScriptHandler,
    lua: &Lua,
    event: &SessionEvent,
) -> HandlerExecutionResult {
    match handler.execute(lua, event) {
        Ok(Some(modified_event)) => {
            HandlerExecutionResult::ok(&handler.metadata.name, modified_event)
        }
        Ok(None) => HandlerExecutionResult::pass_through(&handler.metadata.name),
        Err(e) => HandlerExecutionResult::err(&handler.metadata.name, e.to_string()),
    }
}

/// Run a chain of handlers on an event
///
/// Executes handlers in order, passing the (potentially modified) event
/// through each handler. Stops if a handler returns cancel.
///
/// # Returns
/// * `Ok(Some(event))` - All handlers passed, returns final event
/// * `Ok(None)` - A handler cancelled the event
/// * `Err(e)` - A handler failed
pub fn run_handler_chain(
    lua: &Lua,
    handlers: &[&LuaScriptHandler],
    event: SessionEvent,
) -> Result<Option<SessionEvent>, LuaError> {
    let mut current_event = event;

    for handler in handlers {
        match handler.execute(lua, &current_event) {
            Ok(Some(modified)) => {
                current_event = modified;
            }
            Ok(None) => {
                // Pass-through, keep current event
            }
            Err(e) => {
                // Check if this is a cancel
                let err_str = e.to_string();
                if err_str.contains("cancelled") {
                    debug!(
                        "Handler {} cancelled event: {}",
                        handler.metadata.name, err_str
                    );
                    return Ok(None);
                }
                return Err(LuaError::Runtime(err_str));
            }
        }
    }

    Ok(Some(current_event))
}
