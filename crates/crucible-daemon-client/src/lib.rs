//! Client library for connecting to Crucible daemon

mod client;
mod storage;

pub use client::DaemonClient;
pub use crucible_daemon::{is_daemon_running, socket_path};
pub use storage::DaemonStorageClient;
