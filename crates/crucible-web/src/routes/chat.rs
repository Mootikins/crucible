use crate::routes::helpers::{stream_version_frame, versioned};
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::sse::{Event, Sse},
    Json,
};
use crucible_core::protocol::session_events::SessionEventPayload;
use crucible_core::protocol::SystemPayload;
use futures::stream::{iter, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn chat_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(send_message))
        .routes(routes!(event_stream))
        .routes(routes!(interaction_respond))
        .routes(routes!(pending_interactions))
}

#[derive(Debug, Deserialize, ToSchema)]
struct SendMessageRequest {
    session_id: String,
    content: String,
    /// Stored review comments that the message attaches. The daemon builds
    /// the context of each one, and refuses an unknown or resolved comment.
    #[serde(default)]
    comments: Vec<crucible_core::diff::CommentRef>,
}

/// Start a turn in a session, or run the daemon command that the message
/// names.
///
/// A turn answers its `message_id`: the browser correlates the SSE events
/// that follow with it. A command that ran without a turn, such as a plugin
/// command, answers its result instead.
#[utoipa::path(
    post,
    path = "/api/chat/send",
    request_body = SendMessageRequest,
    responses(
        (status = 200, body = crucible_core::types::SendOutcome),
        (status = 400, description = "The message is empty and attaches no comment"),
        (status = 502, description = "The daemon could not accept the message"),
    )
)]
async fn send_message(
    State(state): State<AppState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<crucible_core::types::SendOutcome>, WebError> {
    if req.content.trim().is_empty() && req.comments.is_empty() {
        return Err(WebError::Chat("Message cannot be empty".to_string()));
    }

    let outcome = state
        .daemon
        .session_send_message(&req.session_id, &req.content, &req.comments)
        .await
        .daemon_err()?;

    Ok(Json(outcome))
}

/// The resume cursor of `GET /api/chat/events/{session_id}`.
///
/// `after` is the last event seq the client APPLIED for this session. A
/// request that sends none is a fresh viewer: it hydrates from the history
/// route, and replaying the whole log at it would duplicate every turn.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct EventStreamQuery {
    /// Replay the persisted events past this seq.
    after: Option<u64>,
}

/// One SSE `data:` payload of `GET /api/chat/events/{session_id}`.
///
/// Documentation only — `to_sse` builds each frame from its own producer and
/// never constructs this enum. It exists so `openapi.json` can name one
/// schema for the route: either a typed session event, in the same
/// `{event, data}` shape [`SessionEventPayload`] itself serializes to, or a
/// [`TranscriptFrame`], the second frame a live event sends when it changed
/// the transcript.
#[derive(Debug, ToSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum ChatSseFrame {
    Event(SessionEventPayload),
    Transcript(TranscriptFrame),
}

/// The second SSE frame for a live event that changed the transcript, with
/// the same `id:` as the event's own frame. The browser applies `ops` to the
/// snapshot the history route gave, and drops an op when `seq` is not above
/// the `as_of_seq` of that snapshot.
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

/// The session's live event stream, resuming from a seq cursor.
///
/// The body schema describes one SSE `data:` payload, not the whole stream:
/// OpenAPI has no way to say "many of these, one per line". Each event's seq
/// rides the SSE `id:` field, so a client that reconnects can name the last
/// event it applied — through `?after=` on a fresh `EventSource` (the browser
/// API cannot set headers) or the automatic `Last-Event-ID` a browser sends
/// when it retries the same source.
#[utoipa::path(
    get,
    path = "/api/chat/events/{session_id}",
    params(
        ("session_id" = String, Path, description = "The session to stream"),
        EventStreamQuery,
    ),
    responses((
        status = 200,
        content_type = "text/event-stream",
        body = ChatSseFrame,
        headers((
            "X-Crucible-Stream-Version" = u64,
            description = "The stream protocol this build speaks (also the first \
                           `stream_version` frame, for clients whose transport \
                           cannot read headers)"
        ))
    ))
)]
async fn event_stream(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<EventStreamQuery>,
    headers: HeaderMap,
) -> Result<
    (
        [(axum::http::HeaderName, String); 1],
        Sse<impl Stream<Item = Result<Event, Infallible>>>,
    ),
    WebError,
> {
    // The cursor, either way a client can state it. A non-numeric
    // `Last-Event-ID` is ignored rather than refused: the ids are this
    // route's own numbers, and a client forwarding one it never received
    // from us gets a fresh tail, not a 400 over a header it cannot fix.
    let after = query.after.or_else(|| {
        headers
            .get("last-event-id")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
    });

    // ORDERING IS LOAD-BEARING, twice over. The broker receiver comes before
    // the daemon subscription (the window `fs_event_stream` documents:
    // `EventBroker::dispatch` drops events for sessions with no local
    // channel), and BOTH come before the replay read. An event emitted while
    // the read runs must land in this receiver's buffer — if it can land in
    // neither the snapshot nor the live tail, it is lost exactly when the
    // client reconnected because it was already losing events.
    let live = state
        .daemon
        .subscribe_events(&session_id)
        .await
        .daemon_err()?;

    let replayed = match after {
        Some(after) => state
            .daemon
            .session_events_after(&session_id, after)
            .await
            .daemon_err()?,
        None => Vec::new(),
    };
    // Live forwarding starts strictly above the replayed tail. An event at or
    // below its max seq is either in the tail, or a non-persisted frame (a
    // text delta) whose content the tail's turn-end event supersedes — a
    // plain comparison, no identity tracking.
    let max_replayed = replayed
        .iter()
        .filter_map(|event| event.seq)
        .max()
        .unwrap_or(after.unwrap_or(0));

    // A browser that falls behind loses events here for the same reason it used
    // to lose them in the daemon's forwarder, and `.ok()` discarded the very
    // error that says so. `Lagged(n)` becomes the same `stream_gap` the daemon
    // emits, so the client cannot tell which layer's ring overflowed — it only
    // needs to know its transcript has a hole and how big.
    //
    // Not fatal: the receiver stays usable after lagging, having been advanced to
    // the oldest surviving event, so the stream continues rather than ending the
    // SSE connection and provoking a reconnect that would lose more.
    //
    // A gap ends the filter. After a daemon reconnect the stream carries the
    // events of a new daemon process, which numbers them from its persisted
    // log, so a seq at or below the replayed tail can be a new event. The
    // client refetches on the gap, and each later event goes through.
    let mut floor = max_replayed;
    let live = live
        .filter(move |event| {
            if event.event == SystemPayload::STREAM_GAP {
                floor = 0;
            }
            futures::future::ready(event.seq.is_none_or(|seq| seq > floor))
        })
        .flat_map(|event| iter(to_sse(&event)));
    let stream = iter([Ok(stream_version_frame())])
        .chain(iter(replayed).flat_map(|event| iter(to_sse(&event))))
        .chain(live);

    // Keep-alive comments stop idle proxies/load balancers from dropping the
    // stream, which the client would otherwise treat as a reconnect.
    Ok(versioned(
        Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default()),
    ))
}

/// One daemon event as SSE frames, its seq (when stamped) as the `id:`.
///
/// The `data:` payload is the daemon's own `{event, data}` pair, byte-
/// identical to what the RPC socket and `session.jsonl` carry — one event
/// vocabulary, not a second one re-encoded for the browser. The SSE
/// `event:` name is `event.event`, so a listener needs no `type` field
/// inside the JSON to dispatch on.
///
/// The seq is the cursor's vocabulary: the client reads it back off
/// `MessageEvent.lastEventId` and states it on reconnect, and nothing else on
/// the frame needs it.
///
/// A live event that changed the transcript gives a second frame,
/// `transcript`, with the same `id:`. The first frame stays until the client
/// stops reading it.
fn to_sse(event: &crucible_daemon::SessionEvent) -> Vec<Result<Event, Infallible>> {
    let with_id = |frame: Event| match event.seq {
        Some(seq) => frame.id(seq.to_string()),
        None => frame,
    };

    let body = serde_json::json!({ "event": event.event, "data": event.data });
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
        with_id(
            Event::default()
                .event("transcript")
                .data(serde_json::to_string(&frame).unwrap_or_default()),
        )
    });

    std::iter::once(Ok(main))
        .chain(transcript.map(Ok))
        .collect()
}

/// One interaction a session is waiting on an answer to.
///
/// `request` is the same typed, kind-tagged shape the SSE
/// `interaction_requested` frame carries — see
/// [`crucible_core::interaction::InteractionRequest`]. A permission
/// request's `pattern` is filled in here too, the one place this route
/// decides it, so the browser never re-derives it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct PendingInteraction {
    /// The session that asked.
    session_id: String,
    /// The identifier an answer must carry back.
    request_id: String,
    request: crucible_core::interaction::InteractionRequest,
}

/// The interactions every session is waiting on, in one list.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct PendingInteractionsResponse {
    pending: Vec<PendingInteraction>,
}

/// Aggregate pending interactions across all sessions — the Inbox renders
/// both sources through one component.
#[utoipa::path(
    get,
    path = "/api/interactions/pending",
    responses(
        (status = 200, body = PendingInteractionsResponse),
        (status = 502, description = "The daemon could not list the pending interactions"),
    )
)]
async fn pending_interactions(
    State(state): State<AppState>,
) -> Result<Json<PendingInteractionsResponse>, WebError> {
    let raw = state
        .daemon
        .session_pending_interactions()
        .await
        .daemon_err()?;

    let pending: Vec<PendingInteraction> = raw["pending"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let request: crucible_core::interaction::InteractionRequest =
                        serde_json::from_value(item["request"].clone()).ok()?;
                    let request = match request {
                        crucible_core::interaction::InteractionRequest::Permission(perm) => {
                            crucible_core::interaction::InteractionRequest::Permission(
                                perm.with_suggested_pattern(),
                            )
                        }
                        other => other,
                    };
                    Some(PendingInteraction {
                        session_id: item["session_id"].as_str().unwrap_or_default().to_string(),
                        request_id: item["request_id"].as_str().unwrap_or_default().to_string(),
                        request,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(PendingInteractionsResponse { pending }))
}

#[derive(Debug, Deserialize, ToSchema)]
struct InteractionResponseRequest {
    session_id: String,
    request_id: String,
    /// The answer, in the kind-tagged shape `InteractionResponse` takes.
    ///
    /// Open, because the vocabulary belongs to the daemon's interaction types
    /// and an unreadable answer must come back as this route's 400 naming the
    /// deserialiser's own complaint, not as axum's plain-text rejection.
    #[schema(value_type = serde_json::Value)]
    response: serde_json::Value,
}

/// The answer this route gives when the daemon took the response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct InteractionRespondResponse {
    /// Always `true`. A refusal is an error status, not a `false`.
    ok: bool,
}

/// Infer the `kind` tag for a bare response object.
///
/// `InteractionResponse` is a `kind`-tagged enum. The frontend now always
/// states its kind (`lib/types.ts`), so this is the compatibility path for
/// clients that do not — and it must stay one, because inference cannot cover
/// the full set: a panel result and an ask response both carry `selected`, and
/// an edit response's `{modified}` and a bare `cancelled` have no
/// discriminating field at all. Anything it cannot name is passed through
/// unchanged so the deserializer reports the real error rather than this
/// function guessing wrong.
fn tag_interaction_response(mut value: serde_json::Value) -> serde_json::Value {
    if value.get("kind").is_some() {
        return value;
    }
    let kind = if value.get("allowed").is_some() {
        "permission"
    } else if value.get("selected").is_some() {
        "ask"
    } else if value.get("selected_index").is_some() || value.get("other").is_some() {
        "popup"
    } else {
        return value;
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert("kind".into(), serde_json::json!(kind));
    }
    value
}

/// Answer one pending interaction.
#[utoipa::path(
    post,
    path = "/api/interaction/respond",
    request_body = InteractionResponseRequest,
    responses(
        (status = 200, body = InteractionRespondResponse),
        (status = 400, description = "The response does not name an interaction kind the daemon knows"),
        (status = 502, description = "The daemon could not take the response"),
    )
)]
async fn interaction_respond(
    State(state): State<AppState>,
    Json(req): Json<InteractionResponseRequest>,
) -> Result<Json<InteractionRespondResponse>, WebError> {
    let response: crucible_core::interaction::InteractionResponse =
        serde_json::from_value(tag_interaction_response(req.response))
            .map_err(|e| WebError::Chat(format!("Invalid interaction response: {e}")))?;

    state
        .daemon
        .session_interaction_respond(&req.session_id, &req.request_id, response)
        .await
        .daemon_err()?;

    Ok(Json(InteractionRespondResponse { ok: true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::request_json;
    use crucible_core::interaction::InteractionResponse;
    use crucible_core::protocol::rpc::RpcMethod;

    /// The SSE body that `to_sse` gives for `events`, as text.
    async fn sse_text(events: Vec<crucible_daemon::SessionEvent>) -> String {
        use axum::response::IntoResponse;
        use http_body_util::BodyExt;
        let frames = iter(events).flat_map(|event| iter(to_sse(&event)));
        let body = Sse::new(frames).into_response().into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// A live event with ops gives its own frame, then a `transcript` frame
    /// with the same id. A stored event, which has no ops, gives one frame.
    #[tokio::test]
    async fn a_live_event_sends_its_transcript_ops_as_a_second_frame() {
        use crucible_core::transcript::{TextField, TranscriptOp};
        let mut live = crucible_daemon::SessionEvent::new(
            "chat",
            "text_delta",
            serde_json::json!({"content": "hi"}),
        );
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

        let text = sse_text(vec![live, stored]).await;
        let frames: Vec<&str> = text.split("\n\n").filter(|f| !f.is_empty()).collect();
        assert_eq!(frames.len(), 3, "{text}");
        // The SSE `event:` name is the daemon's own event name — `text_delta`,
        // not a re-tagged `token` — and the `data:` payload is the
        // `{event, data}` pair, not a re-encoded `ChatEvent`.
        assert!(frames[0].contains("event: text_delta") && frames[0].contains("id: 7"));
        let frame0_data = frames[0]
            .lines()
            .find_map(|l| l.strip_prefix("data: "))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(frame0_data).unwrap(),
            serde_json::json!({"event": "text_delta", "data": {"content": "hi"}})
        );
        assert!(frames[1].contains("event: transcript"), "{text}");
        assert!(frames[1].contains("id: 7"), "{text}");
        let data = frames[1]
            .lines()
            .find_map(|l| l.strip_prefix("data: "))
            .unwrap();
        let data: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(
            data,
            serde_json::json!({"type": "transcript", "seq": 7, "ops": [
                {"op": "append", "id": "t1-seg-0", "field": "text", "at": 0, "text": "hi"}
            ]})
        );
        assert!(frames[2].contains("event: text_delta") && frames[2].contains("id: 8"));
    }

    #[tokio::test]
    async fn send_message_answers_the_declared_shape() {
        let (status, json) = request_json(
            "POST",
            "/api/chat/send",
            Some(serde_json::json!({ "session_id": "s-1", "content": "hello" })),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let parsed: crucible_core::types::SendOutcome =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert_eq!(
            parsed,
            crucible_core::types::SendOutcome::Turn {
                message_id: "msg-001".into()
            }
        );
    }

    /// The route forwards the comment references as the client sent them.
    /// The daemon, not the route, builds their context.
    #[tokio::test]
    async fn send_message_forwards_the_attached_comments() {
        use crate::test_support::{build_state, build_test_app, start_mock_daemon};
        use tower::ServiceExt;

        let comments = serde_json::json!([{
            "id": "c1",
            "source": { "kind": "session_record", "session": "s-1" },
        }]);
        let (mock, client) = start_mock_daemon().await;
        let app = build_test_app(build_state(client));
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/chat/send")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::json!({
                            "session_id": "s-1",
                            "content": "",
                            "comments": comments,
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::OK,
            "a message with only a comment is not empty"
        );
        let params = mock.received_params(RpcMethod::SessionSendMessage).unwrap();
        assert_eq!(params["comments"], comments);

        let (status, _) = request_json(
            "POST",
            "/api/chat/send",
            Some(serde_json::json!({ "session_id": "s-1", "content": " " })),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn interaction_respond_answers_the_declared_shape() {
        let (status, json) = request_json(
            "POST",
            "/api/interaction/respond",
            Some(serde_json::json!({
                "session_id": "s-1",
                "request_id": "r-1",
                "response": { "kind": "permission", "allowed": true, "scope": "once" },
            })),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let parsed: InteractionRespondResponse =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert!(parsed.ok);
    }

    /// The daemon names the session and the request; this route decodes the
    /// request into the typed, kind-tagged `InteractionRequest` and fills in
    /// the permission's suggested `pattern`.
    #[tokio::test]
    async fn pending_interactions_answers_the_declared_shape() {
        use crucible_core::interaction::{InteractionRequest, PermAction};

        let (status, json) = request_json("GET", "/api/interactions/pending", None).await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let parsed: PendingInteractionsResponse =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert_eq!(parsed.pending.len(), 1);
        let entry = &parsed.pending[0];
        assert_eq!(entry.session_id, "session-001");
        assert_eq!(entry.request_id, "req-001");
        match &entry.request {
            InteractionRequest::Permission(perm) => {
                assert_eq!(
                    perm.action,
                    PermAction::Bash {
                        tokens: vec!["ls".into()]
                    }
                );
                assert_eq!(
                    perm.pattern.as_deref(),
                    Some("ls"),
                    "the route fills the suggested pattern: {json}"
                );
            }
            other => panic!("expected a permission request, got {other:?}: {json}"),
        }
    }

    /// The exact objects the frontend POSTs (PermResponse/AskResponse/
    /// PopupResponse in web/src/lib/types.ts) must deserialize into the
    /// kind-tagged InteractionResponse after tagging.
    /// The four kinds inference cannot reach arrive tagged, and must survive
    /// the tagging pass untouched. `panel` is the one that would be actively
    /// mis-tagged as `ask` without its own `kind`, because both carry
    /// `selected` — which is why the frontend states it.
    #[test]
    fn explicitly_tagged_responses_pass_through() {
        let cases = [
            serde_json::json!({ "kind": "panel", "selected": [1], "cancelled": false }),
            serde_json::json!({ "kind": "edit", "modified": "new text" }),
            serde_json::json!({
                "kind": "ask_batch",
                "id": "8a2f5c1e-0000-4000-8000-000000000000",
                "answers": [],
                "cancelled": false
            }),
            serde_json::json!({ "kind": "cancelled" }),
        ];
        for case in cases {
            let kind = case["kind"].as_str().unwrap().to_string();
            let tagged = tag_interaction_response(case);
            assert_eq!(tagged["kind"], kind, "tagging changed an explicit kind");
            serde_json::from_value::<InteractionResponse>(tagged)
                .unwrap_or_else(|e| panic!("{kind} did not deserialize: {e}"));
        }
    }

    /// A panel result reaching the inference path would be read as an ask
    /// response. Pinned so nobody "helpfully" drops `kind` from the frontend.
    #[test]
    fn an_untagged_panel_result_is_indistinguishable_from_an_ask() {
        let bare = serde_json::json!({ "selected": [1], "cancelled": true });
        let tagged = tag_interaction_response(bare);
        assert_eq!(
            tagged["kind"], "ask",
            "inference guesses ask for a bare selected-carrying object; \
             panel responses must therefore carry their own kind"
        );
    }

    #[test]
    fn bare_frontend_responses_deserialize_after_tagging() {
        let perm = serde_json::json!({ "allowed": true, "scope": "once" });
        let tagged = tag_interaction_response(perm);
        let parsed: InteractionResponse = serde_json::from_value(tagged).expect("permission");
        assert!(matches!(parsed, InteractionResponse::Permission(p) if p.allowed));

        let ask = serde_json::json!({ "selected": [1] });
        let parsed: InteractionResponse =
            serde_json::from_value(tag_interaction_response(ask)).expect("ask");
        assert!(matches!(parsed, InteractionResponse::Ask(a) if a.selected == vec![1]));

        let popup = serde_json::json!({ "selected_index": 0 });
        let parsed: InteractionResponse =
            serde_json::from_value(tag_interaction_response(popup)).expect("popup");
        assert!(matches!(parsed, InteractionResponse::Popup(_)));
    }

    #[test]
    fn already_tagged_responses_pass_through() {
        let tagged = serde_json::json!({ "kind": "permission", "allowed": false, "scope": "once" });
        let out = tag_interaction_response(tagged.clone());
        assert_eq!(out, tagged);
    }

    // ── Cross-language drift guards ───────────────────────────────────
    //
    // These two moved here from the deleted `crucible-web/src/events.rs`
    // when `ChatEvent` was deleted (see step 11 of the Simplification
    // Plan). Neither depends on `ChatEvent`: one guards the stop-reason
    // wording, the other the side-channel SSE names, and both still hold
    // under the SSE route sending the daemon's own `{event, data}` pair.

    /// Every `.ts` and `.tsx` file the frontend ships.
    ///
    /// A walk rather than `include_str!`, because the gate below must refuse a
    /// wording wherever a future edit puts it, not only in the one file that
    /// held the old copy.
    fn frontend_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/src");
        walkdir::WalkDir::new(&root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                matches!(
                    e.path().extension().and_then(|x| x.to_str()),
                    Some("ts" | "tsx")
                )
            })
            .filter_map(|e| {
                let name = e.path().strip_prefix(&root).ok()?.display().to_string();
                Some((name, std::fs::read_to_string(e.path()).ok()?))
            })
            .collect()
    }

    /// **The H3 gate.** `StopReason::user_notice` is the only wording. The page
    /// held a second one (`stopReasonNotice` in `lib/stop-reason.ts`) and the
    /// two had already drifted — a capital letter and a trailing full stop — so
    /// the comparison ignores case and a trailing stop. Any frontend file that
    /// spells a notice again fails here.
    ///
    /// The notices come from the running enum through [`StopReason::ALL`], not
    /// from a list in this file, and the walk must find sources: an empty parse
    /// is a gate that passes because it looked at nothing.
    #[test]
    fn the_frontend_words_no_stop_reason_notice() {
        use crucible_core::turn::StopReason;

        let sources = frontend_sources();
        assert!(
            !sources.is_empty(),
            "walked no frontend sources — the path moved, fix this test"
        );

        let notices: Vec<&str> = StopReason::ALL
            .iter()
            .filter_map(StopReason::user_notice)
            .collect();
        assert!(
            !notices.is_empty(),
            "no reason words a notice — `user_notice` changed, fix this test"
        );

        let normalize = |s: &str| s.to_lowercase().replace('.', "");
        let mut offenders = Vec::new();
        for (name, body) in &sources {
            let flat = normalize(body);
            for notice in &notices {
                if flat.contains(&normalize(notice)) {
                    offenders.push(format!("web/src/{name}: {notice:?}"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "the daemon words the stop-reason note; the page must draw \
             `stop_notice` instead of spelling it again:\n  {}",
            offenders.join("\n  ")
        );
    }

    // ── The side-channel SSE names ───────────────────────────────────

    /// The publication and surface streams do not travel the chat channel, so
    /// a gate that compares the chat vocabulary alone cannot see them. That is
    /// why a fresh literal for one of these two names passed review before
    /// this test existed.
    ///
    /// Each name comes from the compiled const, which `event_payload!`
    /// expands from the same literal as the daemon's serde rename. So the
    /// chain is: rename and core const → route const → browser listener.
    #[test]
    fn every_side_channel_event_name_has_a_frontend_listener() {
        let sources = frontend_sources();
        assert!(
            !sources.is_empty(),
            "walked no frontend sources — the path moved, fix this test"
        );

        let declared = [
            crate::routes::PublicationChangedEvent::EVENT_NAME,
            crate::routes::SurfaceChangedEvent::EVENT_NAME,
            crate::routes::ProposalChangedEvent::EVENT_NAME,
        ];

        // `SIDE_CHANNEL_EVENTS` in `lib/api.ts` types each listener table as
        // `Record<name, listener>`, so tsc proves a listener for each name in
        // the table. This test proves the table holds exactly the names the
        // routes send.
        let api = sources
            .iter()
            .find(|(name, _)| name == "lib/api.ts")
            .map(|(_, body)| body.as_str())
            .expect("web/src/lib/api.ts is missing — the path moved, fix this test");
        let start = api
            .find("const SIDE_CHANNEL_EVENTS = {")
            .expect("lib/api.ts declares no SIDE_CHANNEL_EVENTS table");
        let table = &api[start..];
        let table = &table[..table
            .find("} as const;")
            .expect("SIDE_CHANNEL_EVENTS does not end with `} as const;`")];
        let listed: std::collections::BTreeSet<&str> =
            table.split('\'').skip(1).step_by(2).collect();
        let declared: std::collections::BTreeSet<&str> = declared.into_iter().collect();
        assert_eq!(
            listed, declared,
            "SIDE_CHANNEL_EVENTS in web/src/lib/api.ts must name exactly the \
             side-channel events that the daemon sends"
        );
    }
}
