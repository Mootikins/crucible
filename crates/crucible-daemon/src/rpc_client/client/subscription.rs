//! Event subscription and streaming RPC methods
//!
//! Methods for subscribing to session events and managing event streams.

use anyhow::Result;

use super::DaemonClient;

/// Shared request for `session.subscribe` and `session.unsubscribe`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSubscribeRequest {
    pub session_ids: Vec<String>,
}

impl DaemonClient {
    pub async fn session_subscribe(&self, session_ids: &[&str]) -> Result<serde_json::Value> {
        self.typed_call(
            "session.subscribe",
            SessionSubscribeRequest {
                session_ids: session_ids.iter().map(|s| s.to_string()).collect(),
            },
        )
        .await
    }

    pub async fn session_unsubscribe(&self, session_ids: &[&str]) -> Result<serde_json::Value> {
        self.typed_call(
            "session.unsubscribe",
            SessionSubscribeRequest {
                session_ids: session_ids.iter().map(|s| s.to_string()).collect(),
            },
        )
        .await
    }
}
