//! Daemon event types.
//!
//! Re-exports [`InternalSessionEvent`] from crucible-core for convenience.
//! Internal events are pipeline signals that never cross the RPC wire.

pub use crucible_core::events::InternalSessionEvent;
