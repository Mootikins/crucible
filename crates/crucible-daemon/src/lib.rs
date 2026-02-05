//! # Crucible Server (cru-server)
//!
//! Headless backend that provides:
//! - **Session management**: Create, pause, resume, end chat/agent/workflow sessions
//! - **Kiln operations**: Open, close, query kilns (SQLite or SurrealDB)
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
//! # Start server (usually auto-started by CLI)
//! cru-server
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
pub mod file_watch_bridge;
pub mod kiln_manager;
pub mod lifecycle;
pub mod permission_bridge;
pub mod project_manager;
pub mod protocol;
pub mod rpc;
pub mod rpc_helpers;
pub mod server;
pub mod session_bridge;
pub mod session_manager;
pub mod session_storage;
pub mod subscription;
pub mod tools_bridge;

pub use acp_handle::{AcpAgentHandle, AcpHandleError};
pub use agent_factory::{create_agent_from_session_config, AgentFactoryError};
pub use agent_manager::{AgentError, AgentManager};
pub use background_manager::{
    BackgroundError, BackgroundJobManager, SubagentContext, SubagentFactory,
};
pub use daemon_plugins::{default_daemon_plugin_paths, DaemonPluginLoader};
pub use file_watch_bridge::{create_event_bridge, DaemonEventBridge};
pub use kiln_manager::KilnManager;
pub use lifecycle::{remove_socket, socket_path, wait_for_shutdown};
pub use permission_bridge::DaemonPermissionGate;
pub use project_manager::{ProjectError, ProjectManager};
pub use protocol::{Request, Response, RpcError, SessionEventMessage};
pub use server::Server;
pub use session_bridge::DaemonSessionBridge;
pub use session_manager::{SessionError, SessionManager};
pub use session_storage::{FileSessionStorage, SessionStorage};
pub use subscription::{ClientId, SubscriptionManager};
pub use tools_bridge::DaemonToolsBridge;
