//! Event subscription and streaming RPC methods
//!
//! Methods for subscribing to session events and managing event streams.

use anyhow::Result;
use crucible_core::protocol::requests::*;
use crucible_core::protocol::RpcMethod;

use super::DaemonClient;

impl DaemonClient {
    pub async fn session_subscribe(&self, session_ids: &[&str]) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionSubscribe,
            SessionSubscribeRequest {
                session_ids: session_ids.iter().map(|s| s.to_string()).collect(),
            },
        )
        .await
    }

    pub async fn session_unsubscribe(&self, session_ids: &[&str]) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionUnsubscribe,
            SessionSubscribeRequest {
                session_ids: session_ids.iter().map(|s| s.to_string()).collect(),
            },
        )
        .await
    }
}
