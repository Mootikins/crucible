//! Wire types of the `notifications` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

use crate::types::Notification;
/// Request for `notification.list`.
///
/// `workspace` and `kilns` say who is asking, so the daemon answers with
/// what that client may see. `all` is the operator's view of the whole ring.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NotificationListRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kilns: Vec<String>,
    #[serde(default)]
    pub all: bool,
}

/// The body of `session.add_notification`, inside `Scoped`.
///
/// `notification` is the whole [`Notification`], so a malformed one answers
/// `INVALID_PARAMS` with the reason from its own deserializer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NewNotification {
    pub notification: Notification,
}

/// Request for `notification.dismiss`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NotificationDismissRequest {
    pub id: String,
}

#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NotificationListResponse {
    pub notifications: Vec<Notification>,
}

#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NotificationDismissResponse {
    pub dismissed: bool,
}
