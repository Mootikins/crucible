//! # Crucible Daemon (cru daemon serve)
//!
//! Headless backend that provides:
//! - **Session management**: Create, pause, resume, end chat/agent/workflow sessions
//! - **Kiln operations**: Open, close, query kilns (SQLite)
//! - **File processing**: Parse and index markdown files
//! - **Event persistence**: Auto-save session events to JSONL/markdown
//!
//! ## Architecture
//!
//! The server listens on a Unix socket and accepts JSON-RPC 2.0 requests.
//! Multiple CLI instances can connect simultaneously.
//!
//! ## Usage
//!
//! ```bash
//! # Start daemon (usually auto-started by CLI)
//! cru daemon serve
//!
//! # Server listens at: $XDG_RUNTIME_DIR/crucible.sock
//! # Or: <tmpdir>/crucible-<uid>/crucible.sock
//! ```
//!

// `spawn_delegation`'s future nests deeply enough that auto-trait resolution
// can blow the default limit (deep `PhantomData` chains in dependency types).
// Not a real cycle — rustc just needs more headroom.
#![recursion_limit = "256"]

pub mod acp;
pub mod acp_handle;
pub mod acp_launch;
pub mod agent_cards;
pub mod agent_factory;
pub mod agent_manager;
pub mod background_manager;
pub mod daemon_plugins;
pub mod delegation;
pub mod embedding;
mod empty_providers;
pub mod enrichment;
pub mod event_emitter;
pub mod file_watch_bridge;
pub mod internal_events;
pub mod kiln_manager;
pub mod lifecycle;
pub mod llm;
pub mod mcp;
pub mod mcp_host;
pub mod mcp_server;
pub mod multi_kiln_search;
pub mod observe;
pub mod permission_bridge;
pub mod pipeline;
pub mod plugin_ops;
pub mod plugin_tools;
pub mod project_manager;
pub mod protocol;
pub mod provider;
pub mod recording;
pub mod replay;
pub mod review;
pub mod rpc;
pub mod rpc_client;
pub mod rpc_helpers;
pub(crate) mod rules_files;
pub mod runtime_defaults;
pub mod scm;
pub mod server;
pub mod session_bridge;
pub mod session_lifecycle;
pub mod session_manager;
pub mod session_migration;
pub mod session_storage;
pub mod skills;
pub mod storage;
pub mod subscription;
#[cfg(test)]
pub(crate) mod test_fixtures;
pub mod test_support;
pub mod tool_dispatch;
pub mod tools;
pub mod tools_bridge;
pub mod trust_resolution;
pub mod watch;
pub mod webhook;
pub mod workflow_handlers;
pub mod workflow_registry;
pub mod workspace;
pub mod workspace_snapshot;
pub mod workspace_targets;

pub use acp_handle::{AcpAgentHandle, AcpHandleError};
pub use agent_factory::{create_agent_from_session_config, AgentFactoryError};
pub use agent_manager::{AgentError, AgentManager, AgentManagerParams};
pub use background_manager::{BackgroundError, BackgroundJobManager};
pub use daemon_plugins::{
    bootstrap_plugin_entry, bootstrap_plugins, daemon_plugin_paths, default_daemon_plugin_paths,
    BootstrapOutcome, DaemonPluginLoader,
};
pub use delegation::{DelegationRequest, DelegationService, DelegationSpawned, DelegationSpawner};
pub use file_watch_bridge::{create_event_bridge, DaemonEventBridge};
pub use kiln_manager::KilnManager;
pub use lifecycle::{remove_socket, socket_path, wait_for_shutdown};
pub use mcp_host::InProcessMcpHost;
pub use mcp_server::McpServerManager;
pub use observe::{events, id, indexer, markdown, serde_md, session};
pub use observe::{
    extract_session_content, list_sessions, load_events, parse_session_log, render_to_markdown,
    wire_to_log_event, LogEvent, PermissionDecision, RenderOptions, SessionContent, SessionId,
    SessionIdError, SessionLogLine, SessionType, TokenUsage,
};
pub use permission_bridge::DaemonPermissionGate;
pub use project_manager::{ProjectError, ProjectManager};
pub use protocol::{Request, Response, RpcError, SessionEventMessage};
pub use recording::{RecordedEvent, RecordingFooter, RecordingHeader};
pub use rpc_client::DaemonAgentHandle;
pub use rpc_client::{ChatResultExt, DaemonNoteStore, DaemonStorageClient};
pub use rpc_client::{
    DaemonCapabilities, DaemonClient, LuaDiscoverPluginsRequest, LuaDiscoverPluginsResponse,
    LuaGenerateStubsRequest, LuaGenerateStubsResponse, LuaInitSessionRequest,
    LuaInitSessionResponse, LuaPluginHealthRequest, LuaPluginHealthResponse,
    LuaRunPluginTestsRequest, LuaRunPluginTestsResponse, LuaShutdownSessionRequest,
    LuaShutdownSessionResponse, PluginTestFailure, PluginTestLoadFailure, SessionEvent,
    VersionCheck,
};
pub use scm::ScmCloneResponse;
pub use server::{BindWithPluginConfigParams, Server};
pub use session_bridge::DaemonSessionBridge;
pub use session_manager::{SessionError, SessionManager};
pub use session_storage::{FileSessionStorage, SessionStorage};
pub use skills::{
    format_skills_for_context, FolderDiscovery, ResolvedSkill, SearchPath, Skill, SkillError,
    SkillParser, SkillResult, SkillScope, SkillSource,
};
pub use subscription::{ClientId, SubscriptionManager};
pub use tools::grep_engine::{GrepHit, GrepSearchResponse};
pub use tools_bridge::DaemonToolsBridge;
pub use watch::*;
