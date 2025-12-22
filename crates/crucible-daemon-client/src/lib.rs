//! Client library for connecting to Crucible daemon

mod client;

pub use client::DaemonClient;
pub use crucible_daemon::{is_daemon_running, socket_path};
