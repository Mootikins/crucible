//! Daemon notification RPC methods: the ring `cru.log.notify` fills.

use anyhow::Result;
use crucible_core::protocol::requests::*;
use crucible_core::protocol::RpcMethod;
use crucible_core::types::Notification;
use std::path::Path;

use super::DaemonClient;

impl DaemonClient {
    /// The notifications a client in `workspace` may see, newest first.
    /// `all` lists the whole ring instead.
    pub async fn notification_list(
        &self,
        workspace: Option<&Path>,
        all: bool,
    ) -> Result<Vec<Notification>> {
        let resp: NotificationListResponse = self
            .typed_call(
                RpcMethod::NotificationList,
                NotificationListRequest {
                    workspace: workspace.map(|w| w.to_string_lossy().into_owned()),
                    kilns: Vec::new(),
                    all,
                },
            )
            .await?;
        Ok(resp.notifications)
    }

    /// Drop one notification. `false` when the ring did not hold it.
    pub async fn notification_dismiss(&self, id: &str) -> Result<bool> {
        let resp: NotificationDismissResponse = self
            .typed_call(
                RpcMethod::NotificationDismiss,
                NotificationDismissRequest { id: id.to_string() },
            )
            .await?;
        Ok(resp.dismissed)
    }
}
