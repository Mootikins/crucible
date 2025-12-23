//! Crucible Session Daemon
//!
//! Library for running the Crucible daemon server that manages
//! SurrealDB connections to multiple kilns.

pub mod kiln_manager;
pub mod lifecycle;
pub mod protocol;
pub mod server;

pub use kiln_manager::KilnManager;
pub use lifecycle::{is_daemon_running, pid_path, socket_path, write_pid_file};
pub use protocol::{Request, Response, RpcError};
pub use server::Server;
