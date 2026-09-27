//! The daemon's system session, served to the browser.
//!
//! `GET /api/events/system` forwards each system event that a browser acts
//! on: `publication_changed` and `proposal_changed`. The older route
//! `GET /api/plugins/events` stays as an alias that forwards the
//! publications only. All projections also forward stream gap control frames.
//!
//! `/api/fs/events` and `/api/surfaces/events` also read the system session.
//! They keep their own routes, because their clients read other shapes.

use crate::routes::helpers::{stream_version_frame, versioned};
use crate::routes::PublicationChangedEvent;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, KeepAliveStream, Sse};
use crucible_core::proposal::ProposalId;
use crucible_core::protocol::{SessionEventPayload, SystemPayload};
use crucible_daemon::SessionEvent;
use futures::stream::BoxStream;
use serde::Serialize;
use std::convert::Infallible;
use tokio_stream::StreamExt;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn events_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(system_event_stream))
}

/// A proposal changed. The browser reads it again through
/// `GET /api/proposals/{id}`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProposalChangedEvent {
    pub id: ProposalId,
}

impl ProposalChangedEvent {
    /// The SSE `event:` name that the browser listens for. The value comes
    /// from the daemon payload, so the two names cannot differ.
    pub const EVENT_NAME: &'static str = SystemPayload::PROPOSAL_CHANGED;

    /// Project a daemon event into this shape, or `None` for another event
    /// or for an event with no valid id.
    pub fn from_daemon_event(ev: &SessionEvent) -> Option<Self> {
        if ev.event != Self::EVENT_NAME {
            return None;
        }
        Some(Self {
            id: ev.data["id"].as_str()?.parse().ok()?,
        })
    }
}

/// One frame of the system stream. The SSE `event:` name tells the two
/// shapes apart.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(untagged)]
pub enum SystemEvent {
    Publication(PublicationChangedEvent),
    Proposal(ProposalChangedEvent),
}

impl SystemEvent {
    /// Project a daemon event into a frame of the general route.
    pub fn from_daemon_event(ev: &SessionEvent) -> Option<Self> {
        PublicationChangedEvent::from_daemon_event(ev)
            .map(Self::Publication)
            .or_else(|| ProposalChangedEvent::from_daemon_event(ev).map(Self::Proposal))
    }

    /// Project a daemon event into a frame of the plugin alias. The alias
    /// forwards the publications only.
    pub fn publication_only(ev: &SessionEvent) -> Option<Self> {
        PublicationChangedEvent::from_daemon_event(ev).map(Self::Publication)
    }

    fn event_name(&self) -> &'static str {
        match self {
            Self::Publication(_) => PublicationChangedEvent::EVENT_NAME,
            Self::Proposal(_) => ProposalChangedEvent::EVENT_NAME,
        }
    }

    pub(crate) fn into_frame(self) -> Event {
        let data = match &self {
            Self::Publication(e) => serde_json::to_string(e),
            Self::Proposal(e) => serde_json::to_string(e),
        }
        .unwrap_or_default();
        Event::default().event(self.event_name()).data(data)
    }
}

/// The headers and the body of one system stream.
pub(crate) type SystemStream = (
    [(axum::http::HeaderName, String); 1],
    Sse<KeepAliveStream<BoxStream<'static, Result<Event, Infallible>>>>,
);

/// Open a stream of the system session that forwards each event that
/// `project` accepts.
///
/// The route subscribes the LOCAL broker channel before it tells the daemon to
/// forward. `EventBroker::dispatch` drops an event for a session id with no
/// local subscriber, so the other order loses the first event.
pub(crate) async fn system_stream(
    state: &AppState,
    project: fn(&SessionEvent) -> Option<Event>,
) -> Result<SystemStream, WebError> {
    let live = state.daemon.subscribe_events("system").await.daemon_err()?;

    let stream =
        futures::stream::iter([Ok(stream_version_frame())]).chain(live.filter_map(move |event| {
            if matches!(
                event.payload(),
                Ok(SessionEventPayload::System(SystemPayload::StreamGap { .. }))
            ) {
                Some(Ok(Event::default()
                    .event(event.event)
                    .data(event.data.to_string())))
            } else {
                project(&event).map(Ok)
            }
        }));
    let stream: BoxStream<'static, Result<Event, Infallible>> = Box::pin(stream);

    Ok(versioned(Sse::new(stream).keep_alive(KeepAlive::default())))
}

/// `GET /api/events/system` — a push when a publication or a proposal
/// changes.
///
/// A proposal belongs to no user session, so a session stream cannot carry
/// it. The Inbox and the note bar listen here.
#[utoipa::path(
    get,
    path = "/api/events/system",
    responses((
        status = 200,
        content_type = "text/event-stream",
        body = SystemEvent,
        headers((
            "X-Crucible-Stream-Version" = u64,
            description = "The stream protocol this build speaks (also the first \
                           `stream_version` frame, for clients whose transport \
                           cannot read headers)"
        ))
    ))
)]
async fn system_event_stream(State(state): State<AppState>) -> Result<SystemStream, WebError> {
    system_stream(&state, |event| {
        SystemEvent::from_daemon_event(event).map(SystemEvent::into_frame)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The browser reads `id` off the frame. The name and the field are the
    /// whole contract.
    #[test]
    fn a_proposal_frame_keeps_the_name_and_the_id() {
        let id = ProposalId::generate();
        let ev = SessionEvent::new(
            "system",
            SystemPayload::PROPOSAL_CHANGED,
            serde_json::json!({ "id": id.to_string() }),
        );
        let projected = SystemEvent::from_daemon_event(&ev).expect("projects");
        assert_eq!(projected.event_name(), ProposalChangedEvent::EVENT_NAME);
        assert_eq!(
            serde_json::to_value(&projected).unwrap(),
            serde_json::json!({ "id": id.to_string() })
        );
        assert!(SystemEvent::publication_only(&ev).is_none());
    }

    /// A frame with no valid id names no proposal, so the stream drops it.
    #[test]
    fn a_proposal_frame_without_an_id_is_dropped() {
        for data in [serde_json::json!({}), serde_json::json!({ "id": "x" })] {
            let ev = SessionEvent::new("system", SystemPayload::PROPOSAL_CHANGED, data);
            assert!(SystemEvent::from_daemon_event(&ev).is_none());
        }
    }
}
