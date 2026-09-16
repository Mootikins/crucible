use crate::events::ChatEvent;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::StreamExt;
use utoipa::ToSchema;
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
}

/// The identifier the daemon minted for the turn this request started.
///
/// One key, because the browser correlates the SSE events that follow with it
/// and needs nothing else to do so.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SendMessageResponse {
    message_id: String,
}

/// Start a turn in a session.
#[utoipa::path(
    post,
    path = "/api/chat/send",
    request_body = SendMessageRequest,
    responses(
        (status = 200, body = SendMessageResponse),
        (status = 400, description = "The message is empty"),
        (status = 502, description = "The daemon could not accept the message"),
    )
)]
async fn send_message(
    State(state): State<AppState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, WebError> {
    if req.content.trim().is_empty() {
        return Err(WebError::Chat("Message cannot be empty".to_string()));
    }

    let message_id = state
        .daemon
        .session_send_message(&req.session_id, &req.content)
        .await
        .daemon_err()?;

    Ok(Json(SendMessageResponse { message_id }))
}

/// The session's live event stream.
///
/// The body schema describes one SSE `data:` payload, not the whole stream:
/// OpenAPI has no way to say "many of these, one per line".
#[utoipa::path(
    get,
    path = "/api/chat/events/{session_id}",
    params(("session_id" = String, Path, description = "The session to stream")),
    responses((status = 200, content_type = "text/event-stream", body = ChatEvent))
)]
async fn event_stream(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, WebError> {
    state
        .daemon
        .session_subscribe(&[session_id.as_str()])
        .await
        .daemon_err()?;

    let rx = state.events.subscribe(&session_id).await;

    // A browser that falls behind loses events here for the same reason it used
    // to lose them in the daemon's forwarder, and `.ok()` discarded the very
    // error that says so. `Lagged(n)` becomes the same `stream_gap` the daemon
    // emits, so the client cannot tell which layer's ring overflowed — it only
    // needs to know its transcript has a hole and how big.
    //
    // Not fatal: the receiver stays usable after lagging, having been advanced to
    // the oldest surviving event, so the stream continues rather than ending the
    // SSE connection and provoking a reconnect that would lose more.
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .map(move |result| match result {
            Ok(event) => event,
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!(session_id = %session_id, dropped = n, "SSE subscriber lagged");
                // Name and payload come from the typed vocabulary rather than a
                // literal, so this cannot drift from what the daemon emits for
                // the same condition.
                let (event_type, data) =
                    crucible_core::protocol::session_events::SessionEventPayload::from(
                        crucible_core::protocol::session_events::SystemPayload::StreamGap {
                            dropped: n,
                        },
                    )
                    .to_wire();
                crucible_daemon::SessionEvent::new(session_id.clone(), event_type, data)
            }
        })
        .map(|event| {
            let chat_event = ChatEvent::from_daemon_event(&event);
            let event_name = chat_event.event_name();
            let data = serde_json::to_string(&chat_event).unwrap_or_default();
            Ok(Event::default().event(event_name).data(data))
        });

    // Keep-alive comments stop idle proxies/load balancers from dropping the
    // stream, which the client would otherwise treat as a reconnect.
    Ok(Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default()))
}

/// One interaction a session is waiting on an answer to.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct PendingInteraction {
    /// The session that asked.
    session_id: String,
    /// The identifier an answer must carry back.
    request_id: String,
    /// The request, in the flat shape the SSE path delivers.
    ///
    /// Deliberately open: `normalize_interaction` writes one object per
    /// interaction kind — a permission request carries `tokens` and maybe
    /// `diffs`, an ask carries the question's own fields — and the kinds are
    /// the daemon's to add to. `kind` tells the browser which one it has.
    #[schema(value_type = Object)]
    request: serde_json::Value,
}

/// The interactions every session is waiting on, in one list.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct PendingInteractionsResponse {
    pending: Vec<PendingInteraction>,
}

/// Aggregate pending interactions across all sessions, with each request
/// normalized to the same flat shape the SSE path delivers — the Inbox
/// renders both sources through one component.
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
                .map(|item| {
                    // Wrap into the SSE payload shape so normalize_interaction
                    // applies the identical mapping.
                    let wire = serde_json::json!({
                        "request_id": item["request_id"],
                        "request": item["request"],
                    });
                    PendingInteraction {
                        session_id: item["session_id"].as_str().unwrap_or_default().to_string(),
                        request_id: item["request_id"].as_str().unwrap_or_default().to_string(),
                        request: crate::events::normalize_interaction(&wire),
                    }
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
    #[schema(value_type = Object)]
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

    #[tokio::test]
    async fn send_message_answers_the_declared_shape() {
        let (status, json) = request_json(
            "POST",
            "/api/chat/send",
            Some(serde_json::json!({ "session_id": "s-1", "content": "hello" })),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let parsed: SendMessageResponse =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert_eq!(parsed.message_id, "msg-001");
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

    /// The daemon names the session and the request; this route replaces the
    /// request body with the flat shape the Inbox renders, and keeps the two
    /// identifiers beside it.
    #[tokio::test]
    async fn pending_interactions_answers_the_declared_shape() {
        let (status, json) = request_json("GET", "/api/interactions/pending", None).await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let parsed: PendingInteractionsResponse =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert_eq!(parsed.pending.len(), 1);
        let entry = &parsed.pending[0];
        assert_eq!(entry.session_id, "session-001");
        assert_eq!(entry.request_id, "req-001");
        assert_eq!(
            entry.request["kind"],
            serde_json::json!("permission"),
            "the request arrives normalised: {json}"
        );
        assert_eq!(
            entry.request["id"],
            serde_json::json!("req-001"),
            "the normalised request carries the id an answer quotes: {json}"
        );
        assert_eq!(entry.request["tokens"], serde_json::json!(["ls"]), "{json}");
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
}
