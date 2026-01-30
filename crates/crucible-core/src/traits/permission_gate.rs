//! Permission gate trait for pluggable permission decisions.

use async_trait::async_trait;

use crate::interaction::{PermRequest, PermResponse};

/// A pluggable permission decision gate.
///
/// Implementations route permission requests through various systems:
/// - Daemon's 3-layer system (is_safe → PatternStore → Lua hooks → user prompt)
/// - Auto-allow for testing or non-interactive contexts
#[async_trait]
pub trait PermissionGate: Send + Sync {
    /// Request permission for an agent action.
    async fn request_permission(&self, request: PermRequest) -> PermResponse;
}
