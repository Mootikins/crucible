//! Chat module — currently only houses the agent-event bridge.
//!
//! The ring-buffer bridge sits between the streaming agent layer and
//! the TUI's `SessionEvent` consumers.

pub mod bridge;
