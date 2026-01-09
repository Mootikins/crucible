//! Client library for connecting to Crucible daemon
//!
//! This crate provides two connection patterns:
//!
//! 1. **Daemon binary** (re-exported from crucible-daemon): Separate daemon binary
//! 2. **Single-binary db-server** (lifecycle module): Fork `cru db-server` on demand
//!
//! Daemon detection is socket-based:
//! - If socket exists and connectable -> daemon running
//! - If socket exists but not connectable -> stale socket, safe to replace
//! - If socket doesn't exist -> daemon not running
//!
//! For new code, prefer the single-binary pattern using `lifecycle::ensure_daemon()`.

mod client;
pub mod lifecycle;
mod storage;

pub use client::{DaemonClient, SessionEvent};
pub use storage::{DaemonNoteStore, DaemonStorageClient};

// Re-exports from crucible-daemon for convenience
pub use crucible_daemon::socket_path as legacy_socket_path;
