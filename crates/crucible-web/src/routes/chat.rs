use crate::events::ChatEvent;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use tokio_stream::StreamExt;

pub fn chat_routes() -> Router<AppState> {
    Router::new()
        .route("/api/chat/send", post(send_message))
        .route("/api/chat/events/{session_id}", get(event_stream))
        .route("/api/interaction/respond", post(interaction_respond))
        .route("/api/interactions/pending", get(pending_interactions))
}

#[derive(Debug, Deserialize)]
struct SendMessageRequest {
    session_id: String,
    content: String,
}

async fn send_message(
    State(state): State<AppState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    if req.content.trim().is_empty() {
        return Err(WebError::Chat("Message cannot be empty".to_string()));
    }

    let message_id = state
        .daemon
        .session_send_message(&req.session_id, &req.content)
        .await
        .daemon_err()?;

    Ok(Json(serde_json::json!({ "message_id": message_id })))
}

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
                crucible_daemon::SessionEvent {
                    session_id: session_id.clone(),
                    event_type,
                    data,
                }
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

/// Aggregate pending interactions across all sessions, with each request
/// normalized to the same flat shape the SSE path delivers — the Inbox
/// renders both sources through one component.
async fn pending_interactions(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, WebError> {
    let raw = state
        .daemon
        .session_pending_interactions()
        .await
        .daemon_err()?;

    let pending: Vec<serde_json::Value> = raw["pending"]
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
                    serde_json::json!({
                        "session_id": item["session_id"],
                        "request_id": item["request_id"],
                        "request": crate::events::normalize_interaction(&wire),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(serde_json::json!({ "pending": pending })))
}

#[derive(Debug, Deserialize)]
struct InteractionResponseRequest {
    session_id: String,
    request_id: String,
    response: serde_json::Value,
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

async fn interaction_respond(
    State(state): State<AppState>,
    Json(req): Json<InteractionResponseRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let response: crucible_core::interaction::InteractionResponse =
        serde_json::from_value(tag_interaction_response(req.response))
            .map_err(|e| WebError::Chat(format!("Invalid interaction response: {e}")))?;

    state
        .daemon
        .session_interaction_respond(&req.session_id, &req.request_id, response)
        .await
        .daemon_err()?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::tag_interaction_response;
    use crucible_core::interaction::InteractionResponse;

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
