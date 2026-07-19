//! File-tree explorer routes: `GET /api/fs/list` (project dir listing proxy),
//! `POST /api/fs/move` (drag-and-drop move/rename), and `GET /api/fs/events`
//! (live filesystem-change SSE).

use crate::fs_events::FsEvent;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use tokio_stream::StreamExt;

pub fn fs_routes() -> Router<AppState> {
    Router::new()
        .route("/api/fs/list", get(list_dir))
        .route("/api/fs/move", post(move_path))
        .route("/api/fs/events", get(fs_event_stream))
}

#[derive(Debug, Deserialize)]
struct FsListQuery {
    root: String,
    #[serde(default)]
    rel_path: String,
    #[serde(default)]
    show_ignored: bool,
}

/// Proxy a one-level directory listing from the daemon. All security
/// (registry allowlist, path containment, symlink/dotfile handling) is enforced
/// daemon-side; this handler is a thin passthrough of the entry array.
async fn list_dir(
    State(state): State<AppState>,
    Query(query): Query<FsListQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let entries = state
        .daemon
        .fs_list_dir(&query.root, &query.rel_path, query.show_ignored)
        .await
        .daemon_err()?;

    Ok(Json(serde_json::Value::Array(entries)))
}

#[derive(Debug, Deserialize)]
struct FsMoveBody {
    root: String,
    /// `"project"` or `"kiln"` — selects the daemon-side allowlist.
    kind: String,
    from_rel: String,
    to_rel: String,
}

/// Move/rename a file or directory within one root (file-tree DnD backend).
/// All security (allowlist, containment, overwrite refusal) is daemon-side;
/// this handler is a thin passthrough.
async fn move_path(
    State(state): State<AppState>,
    Json(body): Json<FsMoveBody>,
) -> Result<Json<serde_json::Value>, WebError> {
    state
        .daemon
        .fs_move(&body.root, &body.kind, &body.from_rel, &body.to_rel)
        .await
        .daemon_err()?;
    Ok(Json(serde_json::json!({ "moved": true })))
}

/// Live filesystem-change stream for the file-tree explorer.
async fn fs_event_stream(
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, WebError> {
    // ORDERING IS LOAD-BEARING: open the LOCAL broker channel BEFORE telling the
    // daemon to forward. The daemon only forwards "system" events after
    // `subscribe_sticky` lands; `EventBroker::dispatch` drops events for session
    // ids with no local subscriber (verified by
    // `dispatch_ignores_unsubscribed_sessions`). If we subscribed the daemon
    // first, an event forwarded in the window before `events.subscribe` created
    // the "system" channel would be dropped (a first-connection loss window).
    // Creating the broker channel first means every forwarded event has a buffer
    // to land in (broadcast buffers up to capacity regardless of SSE polling).
    let rx = state.events.subscribe("system").await;
    // Sticky: survives reconnect, shared by all browser connections.
    state.daemon.subscribe_sticky("system").await.daemon_err()?;

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .filter_map(|result| result.ok())
        .filter_map(|event| {
            FsEvent::from_daemon_event(&event).map(|fe| {
                let name = fe.event_name();
                let data = serde_json::to_string(&fe).unwrap_or_default();
                Ok(Event::default().event(name).data(data))
            })
        });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
