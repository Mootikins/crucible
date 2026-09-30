//! One SSE route for the browser.
//!
//! `GET /api/events?topics=<a>,<b>,...` replaces the four streams this file
//! and `chat.rs`, `fs.rs` and `surface.rs` used to open on their own: the
//! chat stream of a session, the filesystem watcher, the surface changes and
//! the daemon's system session (publications and proposals). A topic is
//! either a session id or the literal `system`, and a client names as many
//! as it wants on one connection. Each frame's JSON body gains a `topic`
//! field so the browser can route it; nothing else about a frame's payload
//! changed — [`crucible_core::protocol::session_events::SessionEventPayload`]
//! for a session topic, [`FsEvent`], [`SurfaceChangedEvent`],
//! [`PublicationChangedEvent`] and [`ProposalChangedEvent`] for the
//! `system` topic, exactly as `chat.rs`, `fs.rs`, `events.rs` and
//! `surface.rs` built them before this route existed.
//!
//! Each session topic keeps the resume behavior the old chat stream had: a
//! `topic:seq` pair in `?after=` (or in `Last-Event-ID`, for a client whose
//! transport cannot set a query string on retry) replays the persisted tail
//! past that seq, and the live forwarding starts strictly above the
//! replayed max so a live event the replay already covered is not sent
//! twice. The `system` topic keeps no log and answers no cursor, as its four
//! constituent streams never did.

use crate::fs_events::FsEvent;
use crate::routes::helpers::{stream_version_frame, versioned};
use crate::routes::plugin::PublicationChangedEvent;
use crate::routes::surface::SurfaceChangedEvent;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use crucible_core::proposal::ProposalId;
use crucible_core::protocol::session_events::SessionEventPayload;
use crucible_core::protocol::SystemPayload;
use crucible_daemon::SessionEvent;
use futures::stream::{iter, select_all, BoxStream, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn events_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(events_stream))
}

/// The topic every publication, proposal, filesystem and surface event
/// travels on, both on the daemon's own event bus and here.
pub(crate) const SYSTEM_TOPIC: &str = "system";

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

/// One frame of the `system` topic. The SSE `event:` name tells the two
/// shapes apart.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(untagged)]
pub enum SystemEvent {
    Publication(PublicationChangedEvent),
    Proposal(ProposalChangedEvent),
}

impl SystemEvent {
    /// Project a daemon event into a frame of the `system` topic.
    pub fn from_daemon_event(ev: &SessionEvent) -> Option<Self> {
        PublicationChangedEvent::from_daemon_event(ev)
            .map(Self::Publication)
            .or_else(|| ProposalChangedEvent::from_daemon_event(ev).map(Self::Proposal))
    }

    fn event_name(&self) -> &'static str {
        match self {
            Self::Publication(_) => PublicationChangedEvent::EVENT_NAME,
            Self::Proposal(_) => ProposalChangedEvent::EVENT_NAME,
        }
    }
}

/// The second SSE frame for a live session event that changed the
/// transcript, with the same `id:` as the event's own frame. The browser
/// applies `ops` to the snapshot the history route gave, and drops an op
/// when `seq` is not above the `as_of_seq` of that snapshot.
///
/// One variant, internally tagged on `type`, rather than a plain struct with
/// a hand-set `&'static str` field: a bare `&str` gives the schema `type:
/// string`, not the literal `"transcript"` a browser needs to discriminate
/// on. The tag is the whole reason for the enum.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TranscriptFrame {
    Transcript {
        seq: Option<u64>,
        ops: Vec<crucible_core::transcript::TranscriptOp>,
    },
}

/// One SSE `data:` payload of `GET /api/events`.
///
/// Documentation only — the route never constructs this enum, and builds
/// each frame from its own producer instead. It exists so `openapi.json` can
/// name one schema for the route: a typed session event
/// [`SessionEventPayload`] itself serializes to, a [`TranscriptFrame`], or
/// one of the four `system`-topic shapes.
#[derive(Debug, ToSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum EventsFrame {
    Session(SessionEventPayload),
    Transcript(TranscriptFrame),
    Fs(FsEvent),
    Surface(SurfaceChangedEvent),
    System(SystemEvent),
}

/// The topics and the resume cursor of `GET /api/events`.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct EventsQuery {
    /// One or more topics, comma-separated: a session id, or `system` for
    /// publications, proposals, surface changes and filesystem changes.
    topics: String,
    /// Replay the persisted events of each named topic past its seq, as
    /// `topic:seq` pairs, comma-separated. A topic with no pair starts from
    /// the live tail. The `system` topic keeps no log, so a pair for it is
    /// ignored.
    after: Option<String>,
}

/// Splits `topics` on commas, dropping blanks so a trailing comma or
/// doubled separator names no empty topic.
fn parse_topics(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Splits `after` into its `topic:seq` pairs. A pair that does not parse
/// (a bad number, a topic with no `:seq`) is dropped rather than refused:
/// the request still opens the stream for the topics it did understand.
fn parse_after(raw: Option<&str>) -> HashMap<String, u64> {
    raw.map(|raw| {
        raw.split(',')
            .filter_map(|pair| {
                let (topic, seq) = pair.split_once(':')?;
                Some((topic.trim().to_string(), seq.trim().parse().ok()?))
            })
            .collect()
    })
    .unwrap_or_default()
}

/// The browser's own retry of one source sends `Last-Event-ID` (it cannot
/// set `?after=` on that retry). The route honours both spellings of the
/// cursor for the one topic the header can name: the `topic:seq` pair this
/// route itself stamped as the frame's `id:`.
fn last_event_id_after(headers: &HeaderMap) -> Option<(String, u64)> {
    let raw = headers.get("last-event-id")?.to_str().ok()?;
    let (topic, seq) = raw.split_once(':')?;
    Some((topic.to_string(), seq.parse().ok()?))
}

/// Adds the topic to a frame's JSON body. Every payload here is an object,
/// so the field always lands; a body that is not an object (never the case
/// today) is left as it was rather than panicking.
fn with_topic(topic: &str, mut value: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("topic".into(), serde_json::json!(topic));
    }
    value
}

/// One session-topic event as SSE frames, its `id:` naming the topic and,
/// when stamped, the seq (`topic:seq`) — the cursor's own vocabulary, read
/// back off `MessageEvent.lastEventId` and stated again as `?after=` on
/// reconnect.
///
/// The `data:` payload is the daemon's own `{event, data}` pair, byte-
/// identical (but for the added `topic`) to what the RPC socket and
/// `session.jsonl` carry. The SSE `event:` name is `event.event`, so a
/// listener needs no `type` field inside the JSON to dispatch on.
///
/// A live event that changed the transcript gives a second frame,
/// `transcript`, with the same `id:`.
fn session_event_frames(topic: &str, event: &SessionEvent) -> Vec<Result<Event, Infallible>> {
    let with_id = |frame: Event| match event.seq {
        Some(seq) => frame.id(format!("{topic}:{seq}")),
        None => frame,
    };

    let body = with_topic(
        topic,
        serde_json::json!({ "event": event.event, "data": event.data }),
    );
    let main = with_id(
        Event::default()
            .event(event.event.clone())
            .data(serde_json::to_string(&body).unwrap_or_default()),
    );

    let transcript = (!event.transcript.is_empty()).then(|| {
        let frame = TranscriptFrame::Transcript {
            seq: event.seq,
            ops: event.transcript.clone(),
        };
        let body = with_topic(topic, serde_json::to_value(&frame).unwrap_or_default());
        with_id(
            Event::default()
                .event("transcript")
                .data(serde_json::to_string(&body).unwrap_or_default()),
        )
    });

    std::iter::once(Ok(main))
        .chain(transcript.map(Ok))
        .collect()
}

/// One `system`-topic event as an SSE frame, or `None` for an event none of
/// the four projections recognise.
///
/// `stream_gap` bypasses the projections (none of them name it) and is
/// forwarded as the daemon wrote it, the same special case the old
/// `system_stream` made.
fn system_event_frame(event: &SessionEvent) -> Option<Event> {
    if event.event == SystemPayload::STREAM_GAP {
        let body = with_topic(SYSTEM_TOPIC, event.data.clone());
        return Some(
            Event::default()
                .event(event.event.clone())
                .data(serde_json::to_string(&body).unwrap_or_default()),
        );
    }
    if let Some(frame) = FsEvent::from_daemon_event(event) {
        let name = frame.event_name();
        let body = with_topic(
            SYSTEM_TOPIC,
            serde_json::to_value(&frame).unwrap_or_default(),
        );
        return Some(
            Event::default()
                .event(name)
                .data(serde_json::to_string(&body).unwrap_or_default()),
        );
    }
    if let Some(frame) = SurfaceChangedEvent::from_daemon_event(event) {
        let body = with_topic(
            SYSTEM_TOPIC,
            serde_json::to_value(&frame).unwrap_or_default(),
        );
        return Some(
            Event::default()
                .event(SurfaceChangedEvent::EVENT_NAME)
                .data(serde_json::to_string(&body).unwrap_or_default()),
        );
    }
    if let Some(frame) = SystemEvent::from_daemon_event(event) {
        let name = frame.event_name();
        let body = with_topic(
            SYSTEM_TOPIC,
            serde_json::to_value(&frame).unwrap_or_default(),
        );
        return Some(
            Event::default()
                .event(name)
                .data(serde_json::to_string(&body).unwrap_or_default()),
        );
    }
    None
}

/// The frames of one topic, given its live receiver and (for a session
/// topic) any replay the request asked for.
///
/// ORDERING IS LOAD-BEARING, twice over, as the old chat stream's own
/// comment put it: the broker receiver of THIS topic was subscribed before
/// the replay read (the caller guarantees it — see `events_stream`), and
/// both come before this function ever runs. An event emitted while the
/// replay read runs must land in the receiver's buffer.
fn topic_frames(
    topic: String,
    live: crate::services::daemon::EventStream,
    after: Option<u64>,
    replayed: Vec<SessionEvent>,
) -> BoxStream<'static, Result<Event, Infallible>> {
    if topic == SYSTEM_TOPIC {
        let stream = live
            .filter_map(move |event| futures::future::ready(system_event_frame(&event).map(Ok)));
        return Box::pin(stream);
    }

    // Live forwarding starts strictly above the replayed tail. An event at or
    // below its max seq is either in the tail, or a non-persisted frame (a
    // text delta) whose content the tail's turn-end event supersedes — a
    // plain comparison, no identity tracking.
    let max_replayed = replayed
        .iter()
        .filter_map(|event| event.seq)
        .max()
        .unwrap_or(after.unwrap_or(0));

    // A browser that falls behind loses events here for the same reason it
    // used to lose them in the daemon's forwarder. `Lagged(n)` becomes the
    // same `stream_gap` the daemon emits, so the client cannot tell which
    // layer's ring overflowed — it only needs to know its transcript has a
    // hole and how big. A gap ends the filter: after a daemon reconnect the
    // stream carries the events of a new daemon process, numbered from its
    // persisted log, so a seq at or below the replayed tail can be a new
    // event.
    let mut floor = max_replayed;
    let live_topic = topic.clone();
    let live = live
        .filter(move |event| {
            if event.event == SystemPayload::STREAM_GAP {
                floor = 0;
            }
            futures::future::ready(event.seq.is_none_or(|seq| seq > floor))
        })
        .flat_map(move |event| iter(session_event_frames(&live_topic, &event)));

    let replay_topic = topic;
    let stream = iter(replayed)
        .flat_map(move |event| iter(session_event_frames(&replay_topic, &event)))
        .chain(live);
    Box::pin(stream)
}

/// The browser's event stream, one connection carrying as many topics as it
/// names.
///
/// The body schema describes one SSE `data:` payload, not the whole stream:
/// OpenAPI has no way to say "many of these, one per line".
#[utoipa::path(
    get,
    path = "/api/events",
    params(EventsQuery),
    responses((
        status = 200,
        content_type = "text/event-stream",
        body = EventsFrame,
        headers((
            "X-Crucible-Stream-Version" = u64,
            description = "The stream protocol this build speaks (also the first \
                           `stream_version` frame, for clients whose transport \
                           cannot read headers)"
        ))
    ))
)]
async fn events_stream(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
    headers: HeaderMap,
) -> Result<
    (
        [(axum::http::HeaderName, String); 1],
        Sse<impl Stream<Item = Result<Event, Infallible>>>,
    ),
    WebError,
> {
    let topics = parse_topics(&query.topics);
    if topics.is_empty() {
        return Err(WebError::Chat(
            "`topics` must name at least one topic".to_string(),
        ));
    }

    let mut after = parse_after(query.after.as_deref());
    if let Some((topic, seq)) = last_event_id_after(&headers) {
        after.entry(topic).or_insert(seq);
    }

    // Every topic subscribes before any topic replays: an event emitted
    // during a slow replay read of one topic must not be lost because a
    // later topic had not subscribed yet.
    let mut live_by_topic = Vec::with_capacity(topics.len());
    for topic in &topics {
        let live = state.daemon.subscribe_events(topic).await.daemon_err()?;
        live_by_topic.push((topic.clone(), live));
    }

    let mut per_topic = Vec::with_capacity(live_by_topic.len());
    for (topic, live) in live_by_topic {
        let after_seq = after.get(&topic).copied();
        let replayed = if topic != SYSTEM_TOPIC {
            match after_seq {
                Some(after_seq) => state
                    .daemon
                    .session_events_after(&topic, after_seq)
                    .await
                    .daemon_err()?,
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
        per_topic.push(topic_frames(topic, live, after_seq, replayed));
    }

    let merged = select_all(per_topic);
    let stream = iter([Ok(stream_version_frame())]).chain(merged);

    // Keep-alive comments stop idle proxies/load balancers from dropping the
    // stream, which the client would otherwise treat as a reconnect.
    Ok(versioned(Sse::new(stream).keep_alive(KeepAlive::default())))
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
    }

    /// A frame with no valid id names no proposal, so the stream drops it.
    #[test]
    fn a_proposal_frame_without_an_id_is_dropped() {
        for data in [serde_json::json!({}), serde_json::json!({ "id": "x" })] {
            let ev = SessionEvent::new("system", SystemPayload::PROPOSAL_CHANGED, data);
            assert!(SystemEvent::from_daemon_event(&ev).is_none());
        }
    }

    /// A live event with ops gives its own frame, then a `transcript` frame
    /// with the same id. A stored event, which has no ops, gives one frame.
    /// Both carry the topic in their body.
    #[tokio::test]
    async fn a_live_event_sends_its_transcript_ops_as_a_second_frame() {
        use axum::response::IntoResponse;
        use crucible_core::transcript::{TextField, TranscriptOp};
        use http_body_util::BodyExt;

        let mut live =
            SessionEvent::new("chat", "text_delta", serde_json::json!({"content": "hi"}));
        live.seq = Some(7);
        live.transcript = vec![TranscriptOp::Append {
            id: "t1-seg-0".into(),
            field: TextField::Text,
            at: 0,
            text: "hi".into(),
        }];
        let mut stored = live.clone();
        stored.seq = Some(8);
        stored.transcript.clear();

        let frames =
            iter(vec![live, stored]).flat_map(|event| iter(session_event_frames("s1", &event)));
        let body = Sse::new(frames).into_response().into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let text = String::from_utf8(bytes.to_vec()).unwrap();

        let frames: Vec<&str> = text.split("\n\n").filter(|f| !f.is_empty()).collect();
        assert_eq!(frames.len(), 3, "{text}");
        assert!(frames[0].contains("event: text_delta") && frames[0].contains("id: s1:7"));
        let frame0_data = frames[0]
            .lines()
            .find_map(|l| l.strip_prefix("data: "))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(frame0_data).unwrap(),
            serde_json::json!({"topic": "s1", "event": "text_delta", "data": {"content": "hi"}})
        );
        assert!(frames[1].contains("event: transcript") && frames[1].contains("id: s1:7"));
        let data = frames[1]
            .lines()
            .find_map(|l| l.strip_prefix("data: "))
            .unwrap();
        let data: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(
            data,
            serde_json::json!({"topic": "s1", "type": "transcript", "seq": 7, "ops": [
                {"op": "append", "id": "t1-seg-0", "field": "text", "at": 0, "text": "hi"}
            ]})
        );
        assert!(frames[2].contains("event: text_delta") && frames[2].contains("id: s1:8"));
    }

    /// A `system`-topic frame carries its topic too.
    #[tokio::test]
    async fn a_system_topic_frame_carries_its_topic() {
        use axum::response::IntoResponse;
        use http_body_util::BodyExt;

        let ev = SessionEvent::new(
            "system",
            SystemPayload::PUBLICATION_CHANGED,
            serde_json::json!({ "plugin": "kanban", "key": "kanban:board" }),
        );
        let frame = system_event_frame(&ev).expect("projects");
        let body = Sse::new(iter([Ok::<_, Infallible>(frame)]))
            .into_response()
            .into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        let data = text
            .lines()
            .find_map(|l| l.strip_prefix("data: "))
            .expect("a data: line");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(data).unwrap(),
            serde_json::json!({ "plugin": "kanban", "key": "kanban:board", "topic": "system" })
        );
    }

    #[test]
    fn topics_split_on_commas_and_drop_blanks() {
        assert_eq!(parse_topics("system,s1,, s2 ,"), vec!["system", "s1", "s2"]);
        assert_eq!(parse_topics(""), Vec::<String>::new());
    }

    #[test]
    fn after_pairs_parse_by_topic() {
        let after = parse_after(Some("s1:5,s2:9"));
        assert_eq!(after.get("s1"), Some(&5));
        assert_eq!(after.get("s2"), Some(&9));
        assert_eq!(parse_after(None), HashMap::new());
        // A pair this build cannot read is dropped, not refused.
        assert_eq!(parse_after(Some("s1:x,s2:9")).get("s1"), None);
    }
}
