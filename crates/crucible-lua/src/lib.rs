//! Luau scripting integration for Crucible
//!
//! This crate provides Luau scripting:
//! - **LLM-friendly**: Simple syntax, massive training data
//! - **Spec tables**: Plugins declare exports by returning a table from `init.lua`
//! - **Threading**: `send` feature enables Send+Sync
//! - **Luau types**: native type annotations and strict mode
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
//! - `luau` (default): Use the Luau runtime
//! - `send`: Enable `Send+Sync` on Lua state for multi-threaded use

pub mod auth_plugin;
pub mod authorship;
pub mod check;
mod context;
mod context_attach;
pub mod discovered;
mod embed;
mod error;
mod error_ext;
mod executor;
mod fs;
pub mod handler_budget;
mod handlers;
mod hooks;
pub mod host_api;
pub mod host_registry;
mod http;
pub mod isolation;
mod json_query;
pub mod lifecycle;
pub mod lua_util;
pub mod luau_compat;
pub mod manifest;
mod mcp;
mod modes;
pub mod modules;
pub mod namespace;
pub mod notify;
mod oil;
pub mod options;
mod paths;
pub mod plugin_context;
pub mod plugin_status;
mod prelude;
pub mod publications;
mod ratelimit;
pub mod schedule;
pub mod schema;
pub mod session_api;
mod session_defaults;
mod sessions;
mod shell;
pub mod signature;
pub mod source_files;
mod storage_api;
pub mod stubs;
mod timer;
mod tools_api;
mod types;
pub mod ui;
mod vault;
mod vec_api;
mod ws;

#[cfg(test)]
pub(crate) mod test_support;

pub mod config;
pub mod config_syntax;
pub mod hl;
pub mod hl_lua;
pub mod statusline_exprs;
pub mod statusline_items;
pub mod statusline_lua;
pub use statusline_lua::register_statusline_items;
pub mod theme;
pub mod theme_wire;
pub mod ui_geometry;

pub use auth_plugin::{fire_provider_auth_hooks, get_provider_auth_hooks};
pub use authorship::AuthorRoots;
pub use config::{
    add_plugin_author_root, app_config_origin, app_config_origins, begin_boot_store,
    end_boot_phase, evaluate_config_source, get_app_config, get_app_config_provenance, get_layout,
    get_theme_config, get_ui_geometry, in_boot_phase, install_state, install_store,
    list_available_themes, merge_app_config, merge_app_config_tagged, resolve_theme_file,
    seed_app_config, set_author_roots, set_runtimepath_extender, snapshot_state, snapshot_store,
    split_pinned_app_config, theme_roots, ConfigLoader, ConfigState,
};
pub use config_syntax::{config_syntax_error, mark_config_syntax, ConfigSyntaxError};
pub use context::{
    register_context_module, register_context_module_stub, register_context_validators,
    LuaValidatorRegistry,
};
pub use context_attach::{
    register_context_attach, AttachRejection, ContextAttachRegistry, DEFAULT_ATTACH_BUDGET_CHARS,
};
pub use discovered::{
    DiscoveredCommand, DiscoveredHandler, DiscoveredParam, DiscoveredService, DiscoveredTool,
};
pub use embed::{register_embed_module, register_embed_resolver, EmbedResolver};
pub use error::{format_lua_error, LuaError};
pub use executor::{register_log_function, LuaExecutor};
pub use fs::register_fs_module;
pub use handler_budget::{
    enter as enter_handler_budget, install_deadline_hook, BudgetGuard, LIFECYCLE_BUDGET,
    PERMISSION_BUDGET, TURN_STAGE_BUDGET,
};
pub use hooks::{
    clear_plugin_hooks, get_session_end_hooks, get_session_start_hooks,
    get_session_start_required_flags, register_hooks_module,
};
pub use http::register_http_module;
pub use json_query::{
    detect_format, encode_to_format, json_to_lua, lua_to_json, parse_auto, parse_with_format,
    register_oq_module, Format,
};
pub use notify::{
    register_notify_module, upgrade_with_notify_sink, NotificationSink, NotifyRequest,
};
pub use oil::{register_oil_module, LuaNode};
pub use paths::{register_paths_module, PathsContext};
pub use plugin_context::{
    current_may_intercept, current_plugin_context, current_plugin_name, enter_plugin,
    set_plugin_context, PluginContext,
};
pub use prelude::{register_prelude, register_test_harness};
pub use ratelimit::register_ratelimit_module;
pub use schedule::register_schedule_module;
pub use schema::{discovered_params_to_json_schema, generate_input_schema};
pub use shell::{register_shell_module, ExecResult, PluginShellPolicy};
pub use statusline_exprs::{
    register_statusline_exprs, ExprRejection, StatuslineExprRegistry, MAX_KEYS_PER_SESSION,
};
pub use storage_api::{register_storage_module, register_storage_module_with_store};
pub use timer::register_timer_module;
pub use types::{LuaExecutionResult, LuaTool, ToolParam, ToolResult};
pub use vault::{
    register_kiln_path_resolver, register_kiln_repository_resolver, register_vault_module,
    register_vault_module_with_store, register_vault_module_with_store_scoped, KilnPathResolver,
    KilnRepositoryResolver,
};
pub use vec_api::register_vec_module;
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
pub const BUILTIN_INIT_LUA: &str = include_str!("../../../runtime/defaults/init.luau");
// Handler system
pub use check::{
    check_file, check_file_using, check_plugin, check_plugin_using, check_plugin_with,
    find_checker, CheckReport, Checker, CheckerChoice, Finding, TypecheckStatus,
};
pub use handlers::{
    execute_permission_hooks, execute_tool_before_execute_hooks,
    execute_tool_display_complete_hooks, execute_tool_display_start_hooks,
    interpret_handler_result, register_cru_on_api, register_permission_hook_api, EventOutcome,
    LuaScriptHandlerRegistry, PermissionHook, PermissionHookResult, PermissionRequest,
    RuntimeHandler, ScriptHandlerResult, ToolBeforeExecuteEvent, ToolBeforeExecuteResult,
    ToolDisplayCompleteEvent, ToolDisplayCompleteHints, ToolDisplayStartEvent,
    ToolDisplayStartHints,
};
pub use handlers::{
    hook_names, EventName, HookName, StageId, SHIPPED_DEFAULT_PRIORITY, TOOL_BEFORE_EXECUTE_EVENT,
    TOOL_DISPLAY_COMPLETE_EVENT, TOOL_DISPLAY_START_EVENT,
};
pub use host_api::render_declarations;
pub use host_registry::{HostSignatures, LuauArgs, LuauValue, Ns};
pub use lifecycle::{load_plugin_spec, LifecycleError, LifecycleResult, PluginManager, PluginSpec};
pub use luau_compat::register_stdlib_compat;
pub use manifest::{
    LoadedPlugin, ManifestError, ManifestResult, PluginManifest, PluginSource, PluginState,
};
pub use mcp::register_mcp_module_stub;
pub use modes::{
    humanize_mode_id, register_modes, ModeDefinition, ModePermissions, ModeRegistry, ModeStance,
    ToolSelector,
};
pub use modules::{ModuleLoadHook, ModuleRegistry, ModuleRequest, PrivateRootGuard, RootKind};
pub use session_api::{
    register_session_module, CurrentSession, Session, SessionConfigRpc, SessionVariables,
    UnsupportedSessionRpc,
};
pub use session_defaults::{
    register_session_defaults, SessionDefaultValues, SessionDefaults, SessionDefaultsRpc,
};
pub use sessions::{
    register_sessions_module, register_sessions_module_with_api,
    register_sessions_module_with_api_and_current, DaemonSessionApi, ResponsePart,
};
pub use signature::{LuaType, Param as SignatureParam, Signature, TypeError};
pub use tools_api::{register_tools_module, register_tools_module_with_api, DaemonToolsApi};
pub use ui::{register_ui_module, register_ui_module_with_api, INTERACTION_KINDS};

pub use isolation::{
    register_isolation_module, IsolationClaim, IsolationRegistry, SandboxEnv, SandboxExec,
};
pub use options::{register_options_module, OptionsRegistry};
pub use plugin_status::{register_status_module, Progress, StatusEntry, StatusRegistry};
pub use publications::{register_publish_module, PublicationRegistry};
