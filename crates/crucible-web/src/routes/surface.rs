//! Plugin surfaces: the panels a plugin declares for every client to draw.
//!
//! Two endpoints and no interpretation. `GET /api/surfaces` passes the daemon's
//! answer through verbatim, exactly as publications do, and the SSE stream says
//! only that a surface moved. Nothing here knows what a plugin's rows mean: a row
//! is `{id, text, detail, mark}` and the component draws it from that, so a
//! plugin shipped tomorrow gets a panel with no change on this side.
//!
//! Its own channel rather than a variant on [`FsEvent`](crate::fs_events::FsEvent),
//! which is a *filesystem* change by its own definition. A focused type per
//! channel is what keeps either one honest.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Json, Router,
};
use crucible_daemon::SessionEvent;
use futures::stream::Stream;
use serde::Serialize;
use std::convert::Infallible;
use tokio_stream::StreamExt;

pub fn surface_routes() -> Router<AppState> {
    Router::new()
        .route("/api/surfaces", get(list_surfaces))
        .route("/api/surfaces/events", get(surface_event_stream))
}

/// A surface changed, delivered to the browser.
///
/// Carries the identity and the new version, never the rows — the same contract
/// the daemon event has, and for the same reason: a surface is unbounded where an
/// event is not, and two clients want it at different moments. The browser
/// refetches through `GET /api/surfaces`.
#[derive(Debug, Clone, Serialize)]
pub struct SurfaceChangedEvent {
    pub plugin: String,
    pub name: String,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

impl SurfaceChangedEvent {
    /// The SSE `event:` name the browser listens for.
    pub const EVENT_NAME: &'static str = "surface_changed";

    /// Project a daemon event into this shape, or `None` for anything else.
    ///
    /// `name` is required: an event that cannot say which surface moved would
    /// make every client refetch every surface, which is worse than missing it.
    pub fn from_daemon_event(ev: &SessionEvent) -> Option<Self> {
        if ev.event != Self::EVENT_NAME {
            return None;
        }
        let d = &ev.data;
        Some(Self {
            plugin: d["plugin"].as_str().unwrap_or_default().to_string(),
            name: d["name"].as_str()?.to_string(),
            version: d["version"].as_u64().unwrap_or(0),
            session: d["session"].as_str().map(str::to_string),
        })
    }
}

/// `GET /api/surfaces` — every declared surface, rows included.
///
/// Rows come with the list because a surface is a panel, not a feed: fetching
/// each one separately would draw an empty sidebar first. The registry's row cap
/// keeps the response bounded.
async fn list_surfaces(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let surfaces = state.daemon.surfaces().await.daemon_err()?;
    Ok(Json(serde_json::json!({ "surfaces": surfaces })))
}

/// Live stream of surface changes.
async fn surface_event_stream(
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, WebError> {
    // ORDERING IS LOAD-BEARING, same as `fs_event_stream`: open the LOCAL broker
    // channel BEFORE telling the daemon to forward. The daemon forwards "system"
    // events only after `subscribe_sticky` lands, and `EventBroker::dispatch`
    // drops events for a session id with no local subscriber — so subscribing the
    // daemon first leaves a first-connection loss window.
    let rx = state.events.subscribe("system").await;
    // Sticky: survives reconnect, shared by all browser connections. A surface is
    // daemon-wide, so "system" is the right address.
    state.daemon.subscribe_sticky("system").await.daemon_err()?;

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .filter_map(|result| result.ok())
        .filter_map(|event| {
            SurfaceChangedEvent::from_daemon_event(&event).map(|se| {
                let data = serde_json::to_string(&se).unwrap_or_default();
                Ok(Event::default()
                    .event(SurfaceChangedEvent::EVENT_NAME)
                    .data(data))
            })
        });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
