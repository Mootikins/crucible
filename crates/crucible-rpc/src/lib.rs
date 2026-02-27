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

mod agent;
mod client;
pub mod lifecycle;
mod storage;

pub use agent::DaemonAgentHandle;
pub use client::{
    DaemonCapabilities, DaemonClient, LuaExecuteHookRequest, LuaExecuteHookResponse,
    LuaInitSessionRequest, LuaInitSessionResponse, LuaRegisterHooksRequest,
    LuaRegisterHooksResponse, LuaShutdownSessionRequest, LuaShutdownSessionResponse, SessionEvent,
    VersionCheck,
};
pub use storage::{DaemonNoteStore, DaemonStorageClient};

pub use crucible_core::protocol::socket_path;
