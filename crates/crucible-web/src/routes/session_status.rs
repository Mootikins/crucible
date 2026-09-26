//! `/api/session/{id}/status` — the status list of a session: the items
//! that plugins published and the engine's plugin-turn items. Also
//! `/api/session/{id}/notifications`, the other per-session read a client
//! makes on attach, and the close of one of those notifications. Split from `session.rs`; the status shape is a surface of its own,
//! apart from the session router.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What `GET /api/session/{id}/status` answers.
///
/// Each item is the daemon's [`StatusDisplayItem`], the same type that the
/// `status_items_changed` event carries, so the browser and the TUI read one
/// shape. `id`, `plugin` and `text` stay plain strings: the moment a client
/// enumerates them, a new plugin needs a client change to be visible at all,
/// which is the thing this channel exists to avoid.
///
/// [`StatusDisplayItem`]: crucible_core::types::StatusDisplayItem
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct SessionStatusResponse {
    /// Every item the session has, ordered by priority. A session with no
    /// item answers an empty list.
    status: Vec<crucible_core::types::StatusDisplayItem>,
}

/// Proxy `session.status` verbatim.
///
/// The daemon answers `{"status": [StatusDisplayItem, …]}`, ordered by
/// priority. Nothing here reads an id: items are generic precisely so the
/// chrome owner renders any plugin's state, and a match on a known id would
/// be this crate learning what one particular plugin does. Every future
/// plugin gets the channel for free, so long as this stays a passthrough.
///
/// A session that published nothing — the overwhelmingly common case, and any
/// session the daemon has never seen — comes back as an empty array, not an
/// error.
#[utoipa::path(
    get,
    path = "/api/session/{id}/status",
    params(("id" = String, Path, description = "The session whose slots to read")),
    responses(
        (status = 200, body = SessionStatusResponse),
        (status = 502, description = "The daemon could not read the slots"),
    )
)]
pub(super) async fn session_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionStatusResponse>, WebError> {
    let status = state.daemon.session_status(&id).await.daemon_err()?;
    Ok(Json(super::session::daemon_shape(
        status,
        "session.status",
    )?))
}

/// What `GET /api/session/{id}/notifications` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct SessionNotificationsResponse {
    /// The daemon's notifications of the session, newest first, each with
    /// `id`, `kind` and `message`, as `notification_added` carries them.
    #[schema(value_type = Vec<Object>)]
    notifications: Vec<crucible_core::types::Notification>,
}

/// The notifications of a session. A browser reads them once when it
/// attaches, and then follows `notification_added` and
/// `notification_dismissed` on the event stream.
#[utoipa::path(
    get,
    path = "/api/session/{id}/notifications",
    params(("id" = String, Path, description = "The session whose notifications to read")),
    responses(
        (status = 200, body = SessionNotificationsResponse),
        (status = 502, description = "The daemon could not read the notifications"),
    )
)]
pub(super) async fn session_notifications(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionNotificationsResponse>, WebError> {
    let notifications = state
        .daemon
        .session_list_notifications(&id)
        .await
        .daemon_err()?;
    Ok(Json(SessionNotificationsResponse { notifications }))
}

/// What `POST /api/session/{id}/notifications/{notification_id}/dismiss`
/// answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct DismissSessionNotificationResponse {
    /// True when the daemon dropped the notification of the session, or hid
    /// a shared notification for this session. False when the notification
    /// does not reach the session.
    success: bool,
}

/// The user closed a notification in this session. The daemon drops a
/// notification of the session. A shared notification stays for the other
/// sessions, and this session does not see it again.
#[utoipa::path(
    post,
    path = "/api/session/{id}/notifications/{notification_id}/dismiss",
    params(
        ("id" = String, Path, description = "The session that closes the notification"),
        ("notification_id" = String, Path, description = "The notification to close"),
    ),
    responses(
        (status = 200, body = DismissSessionNotificationResponse),
        (status = 502, description = "The daemon could not close the notification"),
    )
)]
pub(super) async fn dismiss_session_notification(
    State(state): State<AppState>,
    Path((id, notification_id)): Path<(String, String)>,
) -> Result<Json<DismissSessionNotificationResponse>, WebError> {
    let success = state
        .daemon
        .session_dismiss_notification(&id, &notification_id)
        .await
        .daemon_err()?;
    Ok(Json(DismissSessionNotificationResponse { success }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::request_json;

    /// The route answers the struct it declares, for a session with slots and
    /// for one with none.
    #[tokio::test]
    async fn session_status_answers_the_declared_shape() {
        let (status, json) =
            request_json("GET", "/api/session/test-session-001/status", None).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{json}");

        let slots: SessionStatusResponse =
            serde_json::from_value(json.clone()).expect("the reply reads back as its own struct");
        // The engine's plugin-turn item comes first, as the daemon sends it.
        assert_eq!(slots.status[0].id, "plugin_turns:goal");
        assert_eq!(json["status"][0]["kind"], "plugin_turns");
        assert_eq!(json["status"][0]["action"], "plugin_approval");
        assert_eq!(json["status"][0]["pinned"], true);
        assert_eq!(slots.status[1].id, "oci");
        assert_eq!(slots.status[2].plugin, "weather");
        assert_eq!(json["status"][1]["color_group"], "hue-4");
        assert_eq!(json["status"][1]["priority"], 30);
        assert_eq!(json["status"][1]["kind"], "published");
    }

    /// The daemon's `progress` — a fraction, `"indeterminate"`, or `null` for
    /// a state slot — has to reach the browser. A reply type without the
    /// field drops it silently: the daemon sends it, and the reply does not
    /// carry it.
    #[tokio::test]
    async fn a_slot_s_progress_reaches_the_reply() {
        let (status, json) =
            request_json("GET", "/api/session/test-session-001/status", None).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{json}");

        let slots: SessionStatusResponse =
            serde_json::from_value(json.clone()).expect("the reply reads back as its own struct");
        assert_eq!(
            slots.status[1].id, "oci",
            "a state slot still carries its id, with no progress"
        );
        assert_eq!(
            json["status"][1]["progress"],
            serde_json::Value::Null,
            "a state slot's progress is null, not absent"
        );
        assert_eq!(slots.status[2].id, "weather");
        assert_eq!(
            json["status"][2]["progress"],
            serde_json::json!(0.6),
            "a slot mid-work carries its fraction"
        );
    }

    /// A session that published nothing answers an empty list, not an error
    /// and not an absent key.
    #[tokio::test]
    async fn a_quiet_session_answers_an_empty_slot_list() {
        let (status, json) = request_json("GET", "/api/session/quiet-session/status", None).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{json}");

        let slots: SessionStatusResponse =
            serde_json::from_value(json).expect("the reply reads back as its own struct");
        assert!(slots.status.is_empty());
        assert_eq!(
            serde_json::to_value(slots).expect("the reply writes JSON"),
            serde_json::json!({"status": []})
        );
    }
}
