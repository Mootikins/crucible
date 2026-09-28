//! Wire types of the `subscription` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

/// Shared request for `session.subscribe` and `session.unsubscribe`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSubscribeRequest {
    pub session_ids: Vec<String>,
}
