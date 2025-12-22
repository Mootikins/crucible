//! Crucible Session Daemon
//!
//! Library for running the Crucible daemon server that manages
//! SurrealDB connections to multiple kilns.

pub mod protocol;
pub mod server;
pub mod lifecycle;
pub mod kiln_manager;

pub use protocol::{Request, Response, RpcError};
pub use server::Server;
pub use lifecycle::{pid_path, socket_path, is_daemon_running, write_pid_file};
pub use kiln_manager::KilnManager;
