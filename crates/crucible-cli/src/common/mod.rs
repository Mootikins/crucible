//! Common utilities and shared components for Crucible CLI

pub mod tool_manager;
pub mod daemon_manager;

pub use tool_manager::{CrucibleToolManager, ToolManagerConfig};
pub use daemon_manager::{DaemonManager, DaemonResult};