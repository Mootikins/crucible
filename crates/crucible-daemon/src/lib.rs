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
//! # Or: /tmp/crucible.sock
//! ```
//!
pub mod acp_handle;
pub mod agent_factory;
pub mod agent_manager;
pub mod background_manager;
pub mod daemon_plugins;
pub mod embedding;
mod empty_providers;
pub mod enrichment;
pub mod event_emitter;
pub mod file_watch_bridge;
pub mod internal_events;
pub mod kiln_manager;
pub mod lifecycle;
pub mod mcp_host;
pub mod mcp_server;
pub mod multi_kiln_search;
pub mod observe;
pub mod permission_bridge;
pub mod pipeline;
pub mod project_manager;
pub mod protocol;
pub mod provider;
pub mod recording;
pub mod replay;
pub mod rpc;
pub mod rpc_client;
pub mod rpc_helpers;
pub mod server;
pub mod session_bridge;
pub mod session_manager;
pub mod session_storage;
pub mod skills;
pub mod subscription;
pub mod tool_dispatch;
pub mod tools;
pub mod tools_bridge;
pub mod trust_resolution;
pub mod watch;

pub use acp_handle::{AcpAgentHandle, AcpHandleError};
pub use agent_factory::{create_agent_from_session_config, AgentFactoryError};
pub use agent_manager::{AgentError, AgentManager, AgentManagerParams};
pub use background_manager::{
    BackgroundError, BackgroundJobManager, SubagentContext, SubagentFactory,
};
pub use daemon_plugins::{default_daemon_plugin_paths, DaemonPluginLoader};
pub use file_watch_bridge::{create_event_bridge, DaemonEventBridge};
pub use kiln_manager::KilnManager;
pub use lifecycle::{remove_socket, socket_path, wait_for_shutdown};
pub use mcp_host::InProcessMcpHost;
pub use mcp_server::McpServerManager;
#[cfg(feature = "storage-sqlite")]
pub use observe::SessionIndex;
pub use observe::{events, id, indexer, markdown, serde_md, session, storage, truncate};
pub use observe::{
    extract_session_content, list_sessions, load_events, render_to_markdown, truncate_for_log,
    LogEvent, PermissionDecision, RenderOptions, SessionContent, SessionId, SessionIdError,
    SessionMetadata, SessionType, SessionWriter, TokenUsage, TruncateResult,
    DEFAULT_TRUNCATE_THRESHOLD,
};
pub use permission_bridge::DaemonPermissionGate;
pub use project_manager::{ProjectError, ProjectManager};
pub use protocol::{Request, Response, RpcError, SessionEventMessage};
pub use recording::{RecordedEvent, RecordingFooter, RecordingHeader};
pub use rpc_client::DaemonAgentHandle;
pub use rpc_client::{ChatResultExt, DaemonNoteStore, DaemonStorageClient};
pub use rpc_client::{
    DaemonCapabilities, DaemonClient, LuaDiscoverPluginsRequest, LuaDiscoverPluginsResponse,
    LuaExecuteHookRequest, LuaExecuteHookResponse, LuaGenerateStubsRequest,
    LuaGenerateStubsResponse, LuaInitSessionRequest, LuaInitSessionResponse,
    LuaPluginHealthRequest, LuaPluginHealthResponse, LuaRegisterHooksRequest,
    LuaRegisterHooksResponse, LuaRunPluginTestsRequest, LuaRunPluginTestsResponse,
    LuaShutdownSessionRequest, LuaShutdownSessionResponse, SessionEvent, VersionCheck,
};
pub use server::{BindWithPluginConfigParams, Server};
pub use session_bridge::DaemonSessionBridge;
pub use session_manager::{SessionError, SessionManager};
pub use session_storage::{FileSessionStorage, SessionStorage};
pub use skills::{
    format_skills_for_context, FolderDiscovery, ResolvedSkill, SearchPath, Skill, SkillError,
    SkillParser, SkillResult, SkillScope, SkillSource,
};
pub use subscription::{ClientId, SubscriptionManager};
pub use tools_bridge::DaemonToolsBridge;
pub use watch::*;
