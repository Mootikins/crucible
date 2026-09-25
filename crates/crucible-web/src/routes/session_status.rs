//! `/api/session/{id}/status` — the status slots plugins published for a
//! session. Split from `session.rs`; the status shape is a surface of its own,
//! apart from the session router.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One keyed status slot a plugin published for a session.
///
/// `key`, `plugin` and `level` stay plain strings rather than unions on
/// purpose. The moment a client enumerates them, a new plugin needs a client
/// change to be visible at all, which is the thing this channel exists to
/// avoid.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct SessionStatusSlot {
    /// Stable item id. `key` remains as an alias for older clients.
    id: String,
    /// What the slot is about. The plugin chooses it.
    key: String,
    /// Which plugin published the slot.
    plugin: String,
    /// The line to draw.
    text: String,
    /// How loud the line is, such as `info` or `warn`.
    level: String,
    /// Progress of the slot's work: a fraction (`0.0..=1.0`), the literal
    /// string `"indeterminate"`, or `null` when the slot describes a state
    /// rather than work (`crates/crucible-daemon/src/server/plugins.rs`).
    ///
    /// `null` must stay distinguishable from `0.0`: a bar pinned at zero
    /// reads as stalled, which a state slot such as "sandboxed: alpine" is
    /// not. Opaque `serde_json::Value` rather than a typed union, the same
    /// choice `PluginOptionValueResponse::value` makes for a plugin's opaque
    /// option value — the daemon always writes this key, so it is required
    /// rather than optional.
    progress: serde_json::Value,
    /// Named status group; the browser maps it through its own CSS theme.
    color_group: String,
    /// Smaller priorities appear first; pinned items remain visible in overflow.
    priority: u8,
    /// An engine method this item opens, if any.
    action: Option<String>,
    pinned: bool,
    /// Who made the item: a plugin (`published`), or the engine from the
    /// plugin approval knob and the running plugin turn (`plugin_turns`).
    kind: crucible_core::types::StatusItemKind,
}

/// What `GET /api/session/{id}/status` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct SessionStatusResponse {
    /// Every slot the session has, sorted by key. A session that published
    /// nothing answers an empty list.
    status: Vec<SessionStatusSlot>,
}

/// Proxy `session.status` verbatim.
///
/// The daemon answers `{"status": [{key, plugin, text, level, color_group}, …]}`, sorted by
/// key. Nothing here reads a key: slots are keyed precisely so the chrome
/// owner renders any plugin's state generically, and a match on a known key
/// would be this crate learning what one particular plugin does. Every future
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
        assert_eq!(slots.status[1].key, "oci");
        assert_eq!(slots.status[1].id, "oci");
        assert_eq!(slots.status[2].plugin, "weather");
        assert_eq!(json["status"][1]["color_group"], "hue-4");
        assert_eq!(json["status"][1]["priority"], 30);
        assert_eq!(json["status"][1]["kind"], "published");
    }

    /// The daemon's `progress` — a fraction, `"indeterminate"`, or `null` for
    /// a state slot — has to reach the browser. Before this, `SessionStatusSlot`
    /// had no `progress` field, so serde silently dropped it: the daemon sent
    /// it, and the reply never carried it.
    #[tokio::test]
    async fn a_slot_s_progress_reaches_the_reply() {
        let (status, json) =
            request_json("GET", "/api/session/test-session-001/status", None).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{json}");

        let slots: SessionStatusResponse =
            serde_json::from_value(json.clone()).expect("the reply reads back as its own struct");
        assert_eq!(
            slots.status[1].key, "oci",
            "a state slot still carries the key, with no progress"
        );
        assert_eq!(
            json["status"][1]["progress"],
            serde_json::Value::Null,
            "a state slot's progress is null, not absent"
        );
        assert_eq!(slots.status[2].key, "weather");
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
