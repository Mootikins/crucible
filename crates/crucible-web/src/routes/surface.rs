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
use axum::{extract::State, response::sse::Event, Json};
use crucible_core::protocol::requests::SurfaceListReply;
use crucible_core::protocol::SystemPayload;
use crucible_daemon::SessionEvent;
use serde::Serialize;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn surface_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_surfaces))
        .routes(routes!(surface_event_stream))
}

/// A surface changed, delivered to the browser.
///
/// Carries the identity and the new version, never the rows — the same contract
/// the daemon event has, and for the same reason: a surface is unbounded where an
/// event is not, and two clients want it at different moments. The browser
/// refetches through `GET /api/surfaces`.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct SurfaceChangedEvent {
    pub plugin: String,
    pub name: String,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The surface is gone, so the browser drops it instead of refetching.
    ///
    /// Passed through rather than re-derived. The daemon knew this when it
    /// dropped the entry, and a browser that had to ask `GET /api/surfaces`
    /// to find out would pay a round trip for a fact the event already held.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub withdrawn: bool,
}

impl SurfaceChangedEvent {
    /// The SSE `event:` name the browser listens for.
    ///
    /// Read from the daemon's own payload, never written again here. See
    /// `PublicationChangedEvent::EVENT_NAME` for what a second literal cost.
    pub const EVENT_NAME: &'static str = SystemPayload::SURFACE_CHANGED;

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
            // Absent means present: the daemon omits the field for an ordinary
            // change, so only an explicit `true` withdraws a panel. Defaulting
            // the other way would erase every panel on an event this build did
            // not recognise.
            withdrawn: d["withdrawn"].as_bool().unwrap_or(false),
        })
    }
}

/// `GET /api/surfaces` — every declared surface, rows included.
///
/// Rows come with the list because a surface is a panel, not a feed: fetching
/// each one separately would draw an empty sidebar first. The registry's row cap
/// keeps the response bounded. `Surface`, `Shape`, `Mark` and `SurfaceRow` are
/// `crucible_core::types` types: the daemon answers them directly, and this
/// route forwards them unchanged, so no row here can drop a field the daemon
/// added.
#[utoipa::path(
    get,
    path = "/api/surfaces",
    responses(
        (status = 200, body = SurfaceListReply),
        (status = 502, description = "The daemon could not list the surfaces"),
    )
)]
async fn list_surfaces(State(state): State<AppState>) -> Result<Json<SurfaceListReply>, WebError> {
    let surfaces = state.daemon.surfaces().await.daemon_err()?;
    Ok(Json(SurfaceListReply { surfaces }))
}

/// Live stream of surface changes.
///
/// The body schema describes one SSE `data:` payload, not the whole stream.
#[utoipa::path(
    get,
    path = "/api/surfaces/events",
    responses((
        status = 200,
        content_type = "text/event-stream",
        body = SurfaceChangedEvent,
        headers((
            "X-Crucible-Stream-Version" = u64,
            description = "The stream protocol this build speaks (also the first \
                           `stream_version` frame, for clients whose transport \
                           cannot read headers)"
        ))
    ))
)]
async fn surface_event_stream(
    State(state): State<AppState>,
) -> Result<crate::routes::events::SystemStream, WebError> {
    crate::routes::events::system_stream(&state, |event| {
        SurfaceChangedEvent::from_daemon_event(event).map(|frame| {
            Event::default()
                .event(SurfaceChangedEvent::EVENT_NAME)
                .data(serde_json::to_string(&frame).unwrap_or_default())
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::shape;
    use crucible_core::types::{Mark, Shape};

    // =====================================================================
    // The route answers the shape it declares
    // =====================================================================

    #[tokio::test]
    async fn list_surfaces_answers_the_declared_shape() {
        let listing: SurfaceListReply = shape("GET", "/api/surfaces", None).await;

        let panel = &listing.surfaces[0];
        assert_eq!(panel.name, "sessions");
        assert_eq!(panel.shape, Shape::List);
        // About the plugin rather than about one session, and the key is
        // written either way.
        assert_eq!(panel.session, None);
        assert_eq!(panel.rows[0].mark, Some(Mark::Busy));
        // A line with no status. `null` is "no status", never "unknown".
        assert_eq!(panel.rows[1].mark, None);
        assert_eq!(panel.rows[1].detail, None);
    }

    // =====================================================================
    // The push frame
    // =====================================================================

    fn event(data: serde_json::Value) -> SessionEvent {
        SessionEvent::new("system", SurfaceChangedEvent::EVENT_NAME, data)
    }

    /// The browser reads `withdrawn` off the frame to drop a panel without a
    /// refetch (`web/src/components/SurfacesPanel.tsx`). The contract crosses a
    /// language boundary, so it gets a test on this side of it.
    #[test]
    fn a_withdrawal_reaches_the_browser_frame() {
        let projected = SurfaceChangedEvent::from_daemon_event(&event(serde_json::json!({
            "plugin": "p", "name": "sessions", "version": 3, "withdrawn": true,
        })))
        .expect("projects");

        assert!(projected.withdrawn);
        assert_eq!(
            serde_json::to_value(&projected).unwrap(),
            serde_json::json!({
                "plugin": "p", "name": "sessions", "version": 3, "withdrawn": true,
            }),
            "the browser reads this name for the fact"
        );
    }

    /// **Absent means present.** The daemon omits the field for an ordinary
    /// change, so the frame must stay byte-identical to what it was before the
    /// field existed — and must never tell the browser to erase the panel.
    #[test]
    fn an_ordinary_change_carries_no_withdrawal() {
        for data in [
            serde_json::json!({ "plugin": "p", "name": "sessions", "version": 2 }),
            serde_json::json!({
                "plugin": "p", "name": "sessions", "version": 2, "withdrawn": false,
            }),
        ] {
            let projected =
                SurfaceChangedEvent::from_daemon_event(&event(data.clone())).expect("projects");
            assert!(!projected.withdrawn, "{data}");
            assert_eq!(
                serde_json::to_value(&projected).unwrap(),
                serde_json::json!({ "plugin": "p", "name": "sessions", "version": 2 }),
                "an ordinary change serialises as it always did: {data}"
            );
        }
    }
}
