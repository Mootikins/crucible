//! Shared RPC request/response types used across client submodules.
//!
//! This module houses the common types (wire-format structs, generic
//! parameter shapes, and small helpers) that are referenced from more
//! than one of the split submodules (`agent`, `lua`, `session`,
//! `storage`, `subscription`).

/// A session event that the daemon sent to this client.
///
/// This is the wire type itself. A client reads the wire type once; it
/// does not keep a second copy with other field names.
pub type SessionEvent = crucible_core::protocol::SessionEventMessage;
