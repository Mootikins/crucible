//! Daemon notification RPC methods: the ring `cru.log.notify` fills.

use anyhow::Result;
use crucible_core::types::Notification;
use std::path::Path;

use super::DaemonClient;

/// Request for `notification.list`.
///
/// `workspace` and `kilns` say who is asking, so the daemon answers with
/// what that client may see. `all` is the operator's view of the whole ring.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NotificationListRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kilns: Vec<String>,
    #[serde(default)]
    pub all: bool,
}

/// Request for `notification.dismiss`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotificationDismissRequest {
    pub id: String,
}

#[derive(Debug, serde::Deserialize)]
struct NotificationListResponse {
    notifications: Vec<Notification>,
}

#[derive(Debug, serde::Deserialize)]
struct NotificationDismissResponse {
    dismissed: bool,
}

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
                "notification.list",
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
                "notification.dismiss",
                NotificationDismissRequest { id: id.to_string() },
            )
            .await?;
        Ok(resp.dismissed)
    }
}
