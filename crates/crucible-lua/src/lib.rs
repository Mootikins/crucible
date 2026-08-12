//! Lua 5.4 scripting integration for Crucible
//!
//! This crate provides Lua scripting with optional Fennel support:
//! - **LLM-friendly**: Simple syntax, massive training data
//! - **Spec tables**: Plugins declare exports by returning a table from `init.lua`
//! - **Threading**: `send` feature enables Send+Sync
//! - **Fennel**: Lisp syntax with macros (compiles to Lua)
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │  init.lua (returns spec table)              │
//! │                                             │
//! │  return {                                   │
//! │    name = "my-plugin",                      │
//! │    tools = { ... },                         │
//! │    commands = { ... },                      │
//! │  }                                          │
//! └─────────────────────────────────────────────┘
//!                       │
//!                       ▼
//!             ┌─────────────────────────┐
//!             │  PluginManager          │
//!             │  Lua runtime loading    │
//!             └─────────────────────────┘
//!                       │
//!                       ▼
//!             ┌─────────────────────────────────┐
//!             │  Tool/Command/Handler Registry  │
//!             │  JSON Schema generation         │
//!             └─────────────────────────────────┘
//!                       │
//!                       ▼
//!             ┌─────────────────────────────────┐
//!             │  mlua/Lua 5.4 Runtime           │
//!             │  + data, shell, json modules    │
//!             └─────────────────────────────────┘
//! ```
//!
//! ## Feature Flags
//!
//! - `fennel` (default): Bundle the Fennel compiler (~255KB)
//! - `send`: Enable `Send+Sync` on Lua state for multi-threaded use

pub mod annotations;
mod ask;
pub mod auth_plugin;
mod context;
mod context_attach;
pub mod core_handler;
mod error;
mod error_ext;
mod executor;
#[cfg(feature = "fennel")]
mod fennel;
mod fs;
mod graph;
mod handlers;
mod hooks;
mod http;
mod interaction;
pub mod isolation;
mod json_query;
pub mod lifecycle;
mod lua_stdlib;
pub mod lua_util;
pub mod manifest;
mod mcp;
mod modes;
pub mod notify;
mod oil;
pub mod options;
mod paths;
pub mod plugin_status;
pub mod publications;
mod ratelimit;
mod registry;
pub mod schedule;
pub mod schema;
pub mod session;
mod session_api;
mod session_defaults;
mod sessions;
mod shell;
mod storage_api;
pub mod stubs;
mod timer;
mod tools_api;
mod types;
mod vault;
mod views;
mod ws;

#[cfg(test)]
pub(crate) mod test_support;

pub mod config;
pub mod hl;
pub mod hl_lua;
pub mod statusline_exprs;
pub mod statusline_items;
pub mod statusline_lua;
pub mod theme;
pub mod theme_wire;
pub mod ui_geometry;

pub use annotations::{
    DiscoveredCommand, DiscoveredHandler, DiscoveredParam, DiscoveredPlugin, DiscoveredService,
    DiscoveredTool, DiscoveredView,
};
pub use ask::{
    core_answer_to_lua, core_batch_to_lua, core_question_to_lua, core_response_to_lua,
    lua_answer_table_to_core, lua_answer_to_core, lua_batch_table_to_core, lua_batch_to_core,
    lua_question_table_to_core, lua_question_to_core, lua_response_table_to_core,
    lua_response_to_core, register_ask_module, EventPushCallback, LuaAskBatch, LuaAskBatchResponse,
    LuaAskContext, LuaAskError, LuaAskQuestion, LuaQuestionAnswer,
};
pub use auth_plugin::{fire_provider_auth_hooks, get_provider_auth_hooks};
pub use config::{
    get_app_config, get_layout, get_theme_config, get_ui_geometry, list_available_themes,
    merge_app_config, seed_app_config, ConfigLoader, ConfigState,
};
pub use context::{
    register_context_module, register_context_module_stub, register_context_validators,
    LuaValidatorRegistry,
};
pub use context_attach::{
    register_context_attach, AttachRejection, ContextAttachRegistry, DEFAULT_ATTACH_BUDGET_CHARS,
};
pub use core_handler::{LuaHandler, LuaHandlerMeta};
pub use error::{format_lua_error, LuaError};
pub use executor::LuaExecutor;
#[cfg(feature = "fennel")]
pub use fennel::FennelCompiler;
pub use fs::register_fs_module;
pub use graph::{register_graph_module, register_graph_module_with_store_scoped};
pub use hooks::{get_session_start_hooks, register_hooks_module};
pub use http::register_http_module;
pub use interaction::{lua_ask_to_core, lua_permission_to_core, register_interaction_module};
pub use json_query::{
    detect_format, encode_to_format, json_to_lua, lua_to_json, parse_auto, parse_with_format,
    register_oq_module, Format,
};
pub use lua_stdlib::register_lua_stdlib;
pub use oil::{register_oil_module, LuaNode};
pub use paths::{register_paths_module, PathsContext};
pub use ratelimit::register_ratelimit_module;
pub use registry::LuaToolRegistry;
pub use schedule::register_schedule_module;
pub use schema::{
    discovered_params_to_json_schema, generate_input_schema, type_to_string, FunctionSignature,
    LuauType, TypedParam,
};
pub use shell::{register_shell_module, ExecResult, ShellPolicy};
pub use statusline_exprs::{
    register_statusline_exprs, ExprRejection, StatuslineExprRegistry, MAX_KEYS_PER_SESSION,
};
pub use storage_api::{register_storage_module, register_storage_module_with_store};
pub use timer::register_timer_module;
pub use types::{LuaExecutionResult, LuaTool, ToolParam, ToolResult};
pub use vault::{
    register_vault_module, register_vault_module_with_store,
    register_vault_module_with_store_scoped,
};
pub use ws::register_ws_module;

/// The shipped defaults, compiled in as a last-resort baseline.
///
/// The file lives in `runtime/defaults/` alongside `runtime/themes/` and
/// `runtime/plugins/` — it is an ordinary runtime file with no privileged API,
/// and `cru setup` copies it out for editing like any other. Prefer
/// `crucible_daemon::runtime_defaults::load_defaults`, which resolves it from
/// the runtimepath first; this constant is what a bare binary with no runtime
/// directory falls back to, matching how themes and the statusline embed
/// theirs.
pub const BUILTIN_INIT_LUA: &str = include_str!("../../../runtime/defaults/init.lua");
// Handler system
pub use handlers::{
    execute_handler, execute_permission_hooks, execute_tool_before_execute_hooks,
    execute_tool_display_complete_hooks, execute_tool_display_start_hooks,
    interpret_handler_result, register_crucible_on_api, register_permission_hook_api,
    run_handler_chain, HandlerExecutionResult, LuaScriptHandler, LuaScriptHandlerRegistry,
    PermissionHook, PermissionHookResult, PermissionRequest, RuntimeHandler, ScriptHandlerResult,
    ToolBeforeExecuteEvent, ToolBeforeExecuteResult, ToolDisplayCompleteEvent,
    ToolDisplayCompleteHints, ToolDisplayStartEvent, ToolDisplayStartHints, HOOK_NAMES,
    SHIPPED_DEFAULT_PRIORITY, TOOL_BEFORE_EXECUTE_EVENT, TOOL_DISPLAY_COMPLETE_EVENT,
    TOOL_DISPLAY_START_EVENT,
};
pub use lifecycle::{
    load_plugin_spec, load_plugin_spec_from_source, CommandBuilder, HandlerBuilder, LifecycleError,
    LifecycleResult, PluginManager, PluginSpec, RegistrationHandle, ToolBuilder, ViewBuilder,
};
pub use manifest::{
    Capability, ExportDeclarations, LoadedPlugin, ManifestError, ManifestResult, PluginDependency,
    PluginManifest, PluginSource, PluginState,
};
pub use mcp::{
    register_mcp_module, register_mcp_module_stub, LuaMcpClient, McpToolInfo, McpToolResult,
};
pub use modes::{
    register_modes, ModeDefinition, ModePermissions, ModeRegistry, ModeStance, ToolSelector,
};
pub use session::{
    LuaSession, LuaSessionBuilder, LuaSessionConfig, LuaSessionHandle, SessionState,
};
pub use session_api::{
    register_session_module, ChannelSessionRpc, Session, SessionCommand, SessionConfigRpc,
    SessionManager,
};
pub use session_defaults::{
    register_session_defaults, SessionDefaultValues, SessionDefaults, SessionDefaultsRpc,
};
pub use sessions::{
    register_sessions_module, register_sessions_module_with_api, DaemonSessionApi, ResponsePart,
};
pub use tools_api::{register_tools_module, register_tools_module_with_api, DaemonToolsApi};

pub use isolation::{
    register_isolation_module, IsolationClaim, IsolationRegistry, SandboxEnv, SandboxExec,
};
pub use options::{register_options_module, OptionsRegistry};
pub use plugin_status::{register_status_module, Progress, StatusEntry, StatusRegistry};
pub use publications::{register_publish_module, PublicationRegistry};
