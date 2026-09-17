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
use crate::routes::helpers::{stream_version_frame, versioned};
use crate::routes::session::daemon_shape;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use crucible_core::protocol::SystemPayload;
use crucible_daemon::SessionEvent;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::StreamExt;
use utoipa::ToSchema;
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

/// What a client draws, mirroring [`crucible_lua::Shape`].
///
/// A closed set here, and not the open string the browser's hand-written type
/// declares, because the daemon cannot send anything else: `Shape::parse`
/// refuses an unknown name when the plugin declares the surface, so a spelling
/// outside this list never reaches a reply. `the_mirrored_vocabularies_stay_closed`
/// holds the two lists together — a variant added in `crucible-lua` fails that
/// test rather than reaching the browser as a shape no renderer knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceShapeRow {
    /// Rows in order, one line each.
    List,
}

/// A row's status, mirroring [`crucible_lua::Mark`].
///
/// Stated semantically, so each client picks its own glyph. Closed for the same
/// reason [`SurfaceShapeRow`] is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceMarkRow {
    /// Work is underway.
    Busy,
    /// Waiting on a person.
    Blocked,
    /// Finished, nothing wrong.
    Ok,
    /// Finished, something is wrong.
    Failed,
}

/// One line of a surface.
///
/// Named for a line rather than for a row, because the daemon calls both the
/// panel and its entries a row and this file needs to name them apart. The
/// wire key stays `rows`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SurfaceLineRow {
    /// Stable identity, chosen by the plugin. What an action names later, and
    /// what a client keys a selection on across a re-push.
    pub id: String,
    /// The line's own text.
    pub text: String,
    /// Secondary text, or `null`. The daemon always writes the key, so
    /// `required` rather than optional.
    #[schema(required = true)]
    pub detail: Option<String>,
    /// Status, or `null` for a line that has none. `null` is "no status",
    /// never "unknown". Always written, so `required`.
    #[schema(required = true)]
    pub mark: Option<SurfaceMarkRow>,
}

/// One declared surface, as `surface.list` reports it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SurfaceRow {
    /// The plugin that declared it, so a stale surface can be attributed.
    pub plugin: String,
    /// The plugin's own name for it. Stable across a reload.
    pub name: String,
    pub title: String,
    pub shape: SurfaceShapeRow,
    /// The session this surface is about, or `null` when it is about the
    /// plugin. Always written, so `required`.
    #[schema(required = true)]
    pub session: Option<String>,
    /// Bumped on every row change, so a client redraws on a change it sees
    /// rather than on a timer.
    pub version: u64,
    pub rows: Vec<SurfaceLineRow>,
}

/// What `GET /api/surfaces` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SurfaceListResponse {
    pub surfaces: Vec<SurfaceRow>,
}

/// `GET /api/surfaces` — every declared surface, rows included.
///
/// Rows come with the list because a surface is a panel, not a feed: fetching
/// each one separately would draw an empty sidebar first. The registry's row cap
/// keeps the response bounded.
#[utoipa::path(
    get,
    path = "/api/surfaces",
    responses(
        (status = 200, body = SurfaceListResponse),
        (status = 502, description = "The daemon could not list the surfaces, or answered a shape this route cannot read"),
    )
)]
async fn list_surfaces(
    State(state): State<AppState>,
) -> Result<Json<SurfaceListResponse>, WebError> {
    let surfaces = state.daemon.surfaces().await.daemon_err()?;
    Ok(Json(SurfaceListResponse {
        surfaces: daemon_shape(surfaces, "surface.list")?,
    }))
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
) -> Result<
    ([(axum::http::HeaderName, String); 1], Sse<impl Stream<Item = Result<Event, Infallible>>>),
    WebError,
> {
    // ORDERING IS LOAD-BEARING, same as `fs_event_stream`: open the LOCAL broker
    // channel BEFORE telling the daemon to forward. The daemon forwards "system"
    // events only after `subscribe_sticky` lands, and `EventBroker::dispatch`
    // drops events for a session id with no local subscriber — so subscribing the
    // daemon first leaves a first-connection loss window.
    let rx = state.events.subscribe("system").await;
    // Sticky: survives reconnect, shared by all browser connections. A surface is
    // daemon-wide, so "system" is the right address.
    state.daemon.subscribe_sticky("system").await.daemon_err()?;

    let stream = futures::stream::iter([Ok(stream_version_frame())]).chain(
        tokio_stream::wrappers::BroadcastStream::new(rx)
            .filter_map(|result| result.ok())
            .filter_map(|event| {
                SurfaceChangedEvent::from_daemon_event(&event).map(|se| {
                    let data = serde_json::to_string(&se).unwrap_or_default();
                    Ok(Event::default()
                        .event(SurfaceChangedEvent::EVENT_NAME)
                        .data(data))
                })
            }),
    );

    Ok(versioned(Sse::new(stream).keep_alive(KeepAlive::default())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{shape, survives};
    use crucible_lua::{Mark, Shape, Surface, SurfaceRow as LuaSurfaceRow};

    // =====================================================================
    // The route answers the shape it declares
    // =====================================================================

    #[tokio::test]
    async fn list_surfaces_answers_the_declared_shape() {
        let listing: SurfaceListResponse = shape("GET", "/api/surfaces", None).await;

        let panel = &listing.surfaces[0];
        assert_eq!(panel.name, "sessions");
        assert_eq!(panel.shape, SurfaceShapeRow::List);
        // About the plugin rather than about one session, and the key is
        // written either way.
        assert_eq!(panel.session, None);
        assert_eq!(panel.rows[0].mark, Some(SurfaceMarkRow::Busy));
        // A line with no status. `null` is "no status", never "unknown".
        assert_eq!(panel.rows[1].mark, None);
        assert_eq!(panel.rows[1].detail, None);
    }

    // =====================================================================
    // The reply writes back the object the daemon sent
    // =====================================================================

    /// Every field of a surface survives [`SurfaceRow`].
    ///
    /// Built by the daemon's own projection rather than from a JSON literal:
    /// `surface_json` is what `surface.list` answers with, so a field added to
    /// `crucible_lua::Surface` and written there fails here instead of vanishing
    /// on the way to the browser. That loss is the risk this route took on when
    /// it stopped forwarding the object verbatim.
    #[test]
    fn a_surface_writes_back_the_object_surface_list_sent() {
        let surface = Surface {
            plugin: "mock-plugin".to_string(),
            name: "sessions".to_string(),
            title: "Sessions".to_string(),
            shape: Shape::List,
            session: Some("session-1".to_string()),
            version: 7,
            // One line per mark, plus one with none, so no variant reaches the
            // row untested.
            rows: [
                Some(Mark::Busy),
                Some(Mark::Blocked),
                Some(Mark::Ok),
                Some(Mark::Failed),
                None,
            ]
            .into_iter()
            .enumerate()
            .map(|(index, mark)| LuaSurfaceRow {
                id: format!("row-{index}"),
                text: format!("line {index}"),
                detail: mark.map(|_| "and more".to_string()),
                mark,
            })
            .collect(),
        };

        survives::<SurfaceRow>(&crucible_daemon::server::plugins::surface_json(&surface));
    }

    /// A surface about the plugin rather than about a session.
    ///
    /// `session` is `null` here and a string above, and both spellings are
    /// written: an absent key would be a different answer, which is why the
    /// field is `required` in the document.
    #[test]
    fn a_plugin_wide_surface_writes_back_its_null_session() {
        let surface = Surface {
            plugin: "mock-plugin".to_string(),
            name: "about".to_string(),
            title: "About".to_string(),
            shape: Shape::List,
            session: None,
            version: 0,
            rows: Vec::new(),
        };

        let wire = crucible_daemon::server::plugins::surface_json(&surface);
        assert_eq!(wire["session"], serde_json::Value::Null);
        survives::<SurfaceRow>(&wire);
    }

    // =====================================================================
    // The mirrored vocabularies stay closed
    // =====================================================================

    /// The wire spelling of every shape a plugin can declare.
    ///
    /// An exhaustive match, so a variant added to `crucible_lua`'s enum fails
    /// to compile here rather than reaching [`SurfaceShapeRow`] as a string it
    /// cannot read. `Shape::as_str` carries the same rule on the other side,
    /// and this is what holds the two together.
    fn shape_spelling(shape: Shape) -> &'static str {
        match shape {
            Shape::List => "list",
        }
    }

    /// The wire spelling of every mark. Exhaustive for the same reason.
    fn mark_spelling(mark: Mark) -> &'static str {
        match mark {
            Mark::Busy => "busy",
            Mark::Blocked => "blocked",
            Mark::Ok => "ok",
            Mark::Failed => "failed",
        }
    }

    /// Every shape a plugin can declare. One today; the exhaustive
    /// [`shape_spelling`] is what makes a second one announce itself.
    const EVERY_SHAPE: &[Shape] = &[Shape::List];

    /// Every mark a row can carry.
    const EVERY_MARK: &[Mark] = &[Mark::Busy, Mark::Blocked, Mark::Ok, Mark::Failed];

    #[test]
    fn the_mirrored_vocabularies_stay_closed() {
        for &declared in EVERY_SHAPE {
            let spelling = shape_spelling(declared);
            assert_eq!(declared.as_str(), spelling, "the daemon's spelling moved");
            serde_json::from_value::<SurfaceShapeRow>(serde_json::json!(spelling))
                .unwrap_or_else(|e| panic!("`SurfaceShapeRow` cannot read `{spelling}`: {e}"));
        }

        for &declared in EVERY_MARK {
            let spelling = mark_spelling(declared);
            assert_eq!(declared.as_str(), spelling, "the daemon's spelling moved");
            serde_json::from_value::<SurfaceMarkRow>(serde_json::json!(spelling))
                .unwrap_or_else(|e| panic!("`SurfaceMarkRow` cannot read `{spelling}`: {e}"));
        }
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
