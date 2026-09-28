//! Client library for connecting to Crucible daemon (cru daemon serve)
//!
//! Connection patterns:
//! - `DaemonClient::connect()` - connect to running daemon
//! - `DaemonClient::connect_or_start()` - connect or spawn daemon if not running
//!
//! Daemon detection is socket-based:
//! - Socket exists and connectable -> daemon running
//! - Socket exists but not connectable -> stale socket, safe to replace
//! - Socket doesn't exist -> daemon not running

mod client;
mod error_ext;
pub mod lifecycle;
mod storage;

pub use client::{decode_status_items, DaemonClient, SessionEvent};
pub use error_ext::{rpc_error_message, ChatResultExt};
// `DaemonClient::fts_search` returns this type, so callers of the client
// must name it without a path into the storage module.
pub use crate::storage::sqlite::FtsResult;
#[cfg(test)]
pub(crate) use storage::parse_note_from_record;
pub use storage::{DaemonNoteStore, DaemonStorageClient};

pub use crucible_core::protocol::socket_path;
