//! Event subscription and streaming RPC methods
//!
//! Methods for subscribing to session events and managing event streams.

use anyhow::Result;
use crucible_core::protocol::requests::*;
use crucible_core::protocol::RpcMethod;

use super::DaemonClient;

impl DaemonClient {
    /// Turns a borrowed `&[&str]` id list into the row's owned
    /// `Vec<String>`. Kept because ~30 call sites across the daemon, the
    /// CLI and the web service pass a borrowed slice (`&["*"]`,
    /// `&[session_id]`); each would otherwise collect it by hand.
    pub async fn session_subscribe(&self, session_ids: &[&str]) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::SessionSubscribe,
            SessionSubscribeRequest {
                session_ids: session_ids.iter().map(|s| s.to_string()).collect(),
            },
        )
        .await
    }

    /// The same borrowed-slice-to-owned-`Vec` transform as
    /// [`Self::session_subscribe`], for the matching unsubscribe row.
    pub async fn session_unsubscribe(&self, session_ids: &[&str]) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::SessionUnsubscribe,
            SessionSubscribeRequest {
                session_ids: session_ids.iter().map(|s| s.to_string()).collect(),
            },
        )
        .await
    }
}
