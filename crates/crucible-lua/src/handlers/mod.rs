//! Handler execution for Lua scripts.
//!
//! The bridge between daemon events and Lua. A handler is registered at load
//! by calling [`register_crucible_on_api`]'s `crucible.on(event, opts, fn)` —
//! there is no filesystem scan and no doc-comment form. An `AnnotationParser`
//! that discovered handlers from `-- @handler` comments used to exist; it was
//! a second, weaker loader for something a plugin already does, and a
//! load-bearing comment fails silently when it is misspelt.
//!
//! ## Registration
//!
//! ```lua
//! -- In a plugin's init.lua
//! crucible.on("tool_result", { pattern = "search_*", priority = 50 }, function(ctx, event)
//!     return event  -- transformed
//! end)
//! ```
//!
//! `event` must be an [`EventName`] or a [`StageId`] — an unknown name is an error at
//! registration, not a handler that never fires.
//!
//! ## Return conventions
//!
//! Neovim-style, interpreted by [`interpret_handler_result`]:
//!
//! - **a table** — transform; the modified event continues the chain
//! - **nil** — pass through unchanged
//! - **`{cancel = true, reason = "..."}`** — abort the chain
//! - **`{handled = true, result = ...}`** — replace execution with `result`
//!
//! ## Dispatch
//!
//! [`LuaScriptHandlerRegistry`] holds the registrations. A dispatch site calls
//! `runtime_handlers_for(event_name, identifier)` to select, then
//! `execute_runtime_handler` per match. `opts.pattern` globs the *identifier*
//! (a tool name), not the event name, so a site with no identifier passes
//! `None` and pattern-bearing handlers correctly do not match.

mod before_execute;
mod conversion;
mod crucible_on;
mod display_hooks;
mod hook_name;
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
pub use hook_name::{hook_names, EventName, HookName, StageId};
pub use permission::{
    execute_permission_hooks, register_permission_hook_api, PermissionHook, PermissionHookResult,
    PermissionRequest, SHIPPED_DEFAULT_PRIORITY,
};
pub use registry::{LuaScriptHandlerRegistry, RuntimeHandler};
pub use script_handler::{interpret_handler_result, ScriptHandlerResult};
