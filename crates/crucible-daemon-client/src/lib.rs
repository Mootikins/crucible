//! Client library for connecting to Crucible daemon
//!
//! This crate provides two connection patterns:
//!
//! 1. **Legacy daemon** (re-exported from crucible-daemon): Uses PID file and separate daemon binary
//! 2. **Single-binary db-server** (lifecycle module): Fork `cru db-server` on demand
//!
//! For new code, prefer the single-binary pattern using `lifecycle::ensure_daemon()`.

mod client;
pub mod lifecycle;
mod storage;

pub use client::{DaemonClient, SessionEvent};
pub use storage::{DaemonNoteStore, DaemonStorageClient};

// Legacy re-exports for backwards compatibility
pub use crucible_daemon::{
    is_daemon_running as is_legacy_daemon_running, socket_path as legacy_socket_path,
};
