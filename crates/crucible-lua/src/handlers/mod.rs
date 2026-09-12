//! Handler execution for Lua scripts.
//!
//! The bridge between daemon events and Lua. A handler is registered at load
//! by calling [`register_cru_on_api`]'s `crucible.on(event, opts, fn)` —
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
//! `runtime_handlers_for(event_name, identifier, firing)` to select, then
//! `execute_runtime_handler` per match. `opts.pattern` globs the *identifier*
//! (a tool name), not the event name, so a site with no identifier passes
//! `None` and pattern-bearing handlers correctly do not match.
//!
//! `firing` is the session the dispatch belongs to, and `opts.session`
//! filters on it the same way — see [`SessionScope`]. A site names the session it is
//! in ([`Firing::InSession`]) or says it has none
//! ([`Firing::Sessionless`]); it never reads a scope itself.

use mlua::Lua;

mod before_execute;
mod conversion;
mod cru_clear;
mod cru_on;
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
pub use cru_clear::register_cru_clear_api;
pub use cru_on::register_cru_on_api;
pub use display_hooks::{
    execute_tool_display_complete_hooks, execute_tool_display_start_hooks,
    ToolDisplayCompleteEvent, ToolDisplayCompleteHints, ToolDisplayStartEvent,
    ToolDisplayStartHints, TOOL_DISPLAY_COMPLETE_EVENT, TOOL_DISPLAY_START_EVENT,
};
pub use hook_name::{hook_names, EventName, HookName, StageId};
pub use permission::{
    execute_permission_hooks, register_permission_hook_api, PermissionHookResult,
    PermissionRequest, PERMISSION_REQUEST_HOOK, SHIPPED_DEFAULT_PRIORITY,
};
/// Crate-internal: the registration APIs share one option parse, and no
/// caller outside this crate registers a handler.
pub(crate) use registry::scope_from_opts;
pub use registry::{
    clear_source, ClearFilter, Firing, LuaScriptHandlerRegistry, Registration, RegistrationSpec,
    SessionScope, DEFAULT_PRIORITY,
};
pub use script_handler::{interpret_handler_result, EventOutcome, ScriptHandlerResult};

/// The VM's registration store, in its app data.
///
/// A newtype so the slot holds exactly one registry and nothing else can be
/// mistaken for it.
struct InstalledRegistry(LuaScriptHandlerRegistry);

/// Install `registry` as the store every registration API on this VM writes.
///
/// The host calls this once, while it builds the VM. Answers the same handle,
/// so a caller can install and keep it in one expression.
pub fn install_registry(lua: &Lua, registry: LuaScriptHandlerRegistry) -> LuaScriptHandlerRegistry {
    lua.set_app_data(InstalledRegistry(registry.clone()));
    registry
}

/// The store this VM's registration APIs write to.
///
/// A VM whose host installed none gets one on demand. Three VMs are built
/// without a handler dispatcher — the config VM, the throwaway
/// `lua.init_session` executor, and a bare `Lua` in a test — and a Lua file
/// running there may still call `cru.on_session_start`. Refusing that would
/// turn a harmless registration nobody reads into a load error. An
/// on-demand store keeps the previous behaviour exactly: the registration
/// lands, and no dispatcher on that VM ever selects it.
pub fn registry_of(lua: &Lua) -> mlua::Result<LuaScriptHandlerRegistry> {
    if let Some(installed) = lua.app_data_ref::<InstalledRegistry>() {
        return Ok(installed.0.clone());
    }
    Ok(install_registry(lua, LuaScriptHandlerRegistry::new()))
}
