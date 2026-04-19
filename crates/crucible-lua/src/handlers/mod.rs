//! Handler execution for Lua scripts
//!
//! Executes Lua functions discovered via `@handler` (or `@hook`) annotations.
//! This module provides the bridge between event bus events and Lua script execution.
//!
//! ## Example
//!
//! ```lua
//! --- Filter search results
//! -- @handler event="tool:after" pattern="search_*" priority=50
//! function filter_results(ctx, event)
//!     -- Modify event.result before returning
//!     return event
//! end
//! ```
//!
//! ## Return Conventions
//!
//! Handlers follow neovim-style return conventions:
//!
//! - **Return event table**: Transform - modified event continues through pipeline
//! - **Return nil**: Pass-through - event unchanged, continues
//! - **Return `{cancel=true, reason="..."}`**: Cancel - abort the pipeline
//!
//! ## Lifecycle
//!
//! 1. Handlers are discovered from Lua/Fennel sources via `AnnotationParser`
//! 2. `LuaScriptHandler` is created from each `DiscoveredHandler`
//! 3. Handlers are registered on the event bus or via `crucible.on()`
//! 4. Events trigger matching handlers in priority order
//!
//! ## Registry
//!
//! The `LuaScriptHandlerRegistry` provides centralized handler management:
//!
//! ```rust,ignore
//! use crucible_lua::LuaScriptHandlerRegistry;
//! use std::path::PathBuf;
//!
//! // Discover handlers from directories
//! let paths = vec![PathBuf::from("./handlers"), PathBuf::from("./plugins")];
//! let registry = LuaScriptHandlerRegistry::discover(&paths)?;
//!
//! // Get handlers matching an event
//! let handlers = registry.handlers_for(&event);
//! ```

mod before_execute;
mod conversion;
mod crucible_on;
mod display_hooks;
mod execution;
mod permission;
mod registry;
mod script_handler;

#[cfg(test)]
mod tests;

pub use before_execute::{
    execute_tool_before_execute_hooks, ToolBeforeExecuteEvent, ToolBeforeExecuteResult,
    TOOL_BEFORE_EXECUTE_EVENT,
};
pub use crucible_on::register_crucible_on_api;
pub use display_hooks::{
    execute_tool_display_complete_hooks, execute_tool_display_start_hooks,
    ToolDisplayCompleteEvent, ToolDisplayCompleteHints, ToolDisplayStartEvent,
    ToolDisplayStartHints, TOOL_DISPLAY_COMPLETE_EVENT, TOOL_DISPLAY_START_EVENT,
};
pub use execution::{execute_handler, run_handler_chain, HandlerExecutionResult};
pub use permission::{
    execute_permission_hooks, register_permission_hook_api, PermissionHook, PermissionHookResult,
    PermissionRequest,
};
pub use registry::{LuaScriptHandlerRegistry, RuntimeHandler};
pub use script_handler::{interpret_handler_result, LuaScriptHandler, ScriptHandlerResult};
