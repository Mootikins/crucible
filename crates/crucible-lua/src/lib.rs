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
pub mod capability_gate;
mod commands;
pub mod core_handler;
mod error;
mod executor;
#[cfg(feature = "fennel")]
mod fennel;
mod fs;
mod graph;
mod handlers;
mod hooks;
mod http;
mod interaction;
mod json_query;
pub mod lifecycle;
mod lua_stdlib;
mod lua_util;
pub mod manifest;
mod mcp;
pub mod notify;
mod oil;
mod panel;
mod paths;
mod popup;
mod ratelimit;
mod registry;
pub mod schema;
pub mod session;
mod session_api;
mod sessions;
mod shell;
pub mod statusline;
mod timer;
mod tools_api;
mod types;
mod vault;
mod views;
mod ws;

pub mod config;

pub use annotations::{
    DiscoveredCommand, DiscoveredHandler, DiscoveredPlugin, DiscoveredService, DiscoveredTool,
    DiscoveredView,
};
pub use ask::{
    core_answer_to_lua, core_batch_to_lua, core_question_to_lua, core_response_to_lua,
    lua_answer_table_to_core, lua_answer_to_core, lua_batch_table_to_core, lua_batch_to_core,
    lua_question_table_to_core, lua_question_to_core, lua_response_table_to_core,
    lua_response_to_core, register_ask_module, register_ask_module_with_agent,
    register_ask_module_with_context, EventPushCallback, LuaAgentAskContext, LuaAskBatch,
    LuaAskBatchResponse, LuaAskContext, LuaAskError, LuaAskQuestion, LuaQuestionAnswer,
};
pub use capability_gate::{check_module_access, module_capability_map, ModuleCapabilityMapping};
pub use commands::{command_to_descriptor, LuaCommandHandler};
pub use config::{get_statusline_config, ConfigLoader, ConfigState};
pub use core_handler::{LuaHandler, LuaHandlerMeta};
pub use error::LuaError;
pub use executor::LuaExecutor;
#[cfg(feature = "fennel")]
pub use fennel::FennelCompiler;
pub use fs::register_fs_module;
pub use graph::{
    register_graph_module, register_graph_module_full, register_graph_module_with_all,
    register_graph_module_with_executor, register_graph_module_with_store,
    register_graph_view_functions, register_note_store_functions,
};
pub use hooks::{get_session_start_hooks, register_hooks_module};
pub use http::register_http_module;
pub use interaction::{lua_ask_to_core, lua_permission_to_core, register_interaction_module};
pub use json_query::{
    detect_format, encode_to_format, json_to_lua, lua_to_json, parse_auto, parse_with_format,
    register_oq_module, Format,
};
pub use lua_stdlib::register_lua_stdlib;
pub use oil::{register_oil_module, LuaNode};
pub use panel::{
    core_result_to_lua, lua_item_to_core, lua_panel_to_core, lua_result_to_core, register_ui_module,
};
pub use paths::{register_paths_module, PathsContext};
pub use popup::{lua_entry_to_core, lua_request_to_core, register_popup_module};
pub use ratelimit::register_ratelimit_module;
pub use registry::LuaToolRegistry;
pub use schema::{generate_input_schema, type_to_string, FunctionSignature, LuauType, TypedParam};
pub use shell::{register_shell_module, ExecResult, ShellPolicy};
pub use statusline::{
    parse_statusline_config, register_statusline_module, ColorSpec, ModeStyleSpec,
    StatuslineComponent, StatuslineConfig, StyleSpec,
};
pub use timer::register_timer_module;
pub use types::{LuaExecutionResult, LuaTool, ToolParam, ToolResult};
pub use vault::{
    register_vault_module, register_vault_module_full, register_vault_module_with_graph,
    register_vault_module_with_store,
};
pub use ws::register_ws_module;
// Handler system
pub use handlers::{
    execute_handler, execute_permission_hooks, interpret_handler_result, register_crucible_on_api,
    register_permission_hook_api, run_handler_chain, HandlerExecutionResult, LuaScriptHandler,
    LuaScriptHandlerRegistry, PermissionHook, PermissionHookResult, PermissionRequest,
    RuntimeHandler, ScriptHandlerResult,
};
pub use lifecycle::{
    load_plugin_spec, load_plugin_spec_from_source, CommandBuilder, HandlerBuilder, LifecycleError,
    LifecycleResult, PluginManager, PluginSpec, RegistrationHandle, ToolBuilder, ViewBuilder,
};
pub use manifest::{
    Capability, ConfigProperty, ConfigSchema, ConfigType, ExportDeclarations, LoadedPlugin,
    ManifestError, ManifestResult, PluginDependency, PluginManifest, PluginState,
};
pub use mcp::{
    register_mcp_module, register_mcp_module_stub, LuaMcpClient, McpToolInfo, McpToolResult,
};
pub use session::{
    LuaSession, LuaSessionBuilder, LuaSessionConfig, LuaSessionHandle, SessionState,
};
pub use session_api::{
    register_session_module, ChannelSessionRpc, Session, SessionCommand, SessionConfigRpc,
    SessionManager,
};
pub use sessions::{
    register_sessions_module, register_sessions_module_with_api, DaemonSessionApi, ResponsePart,
};
pub use tools_api::{register_tools_module, register_tools_module_with_api, DaemonToolsApi};
