//! Crucible Session Daemon
//!
//! Library for running the Crucible daemon server that manages
//! SurrealDB connections to multiple kilns and session lifecycle.

pub mod kiln_manager;
pub mod rpc_helpers;
pub mod lifecycle;
pub mod protocol;
pub mod server;
pub mod session_manager;
pub mod session_storage;
pub mod subscription;

pub use kiln_manager::KilnManager;
pub use lifecycle::{is_daemon_running, pid_path, socket_path, write_pid_file};
pub use protocol::{Notification, Request, Response, RpcError, SessionEventMessage};
pub use server::Server;
pub use session_manager::{SessionError, SessionManager};
pub use session_storage::{FileSessionStorage, SessionStorage};
pub use subscription::{ClientId, SubscriptionManager};
