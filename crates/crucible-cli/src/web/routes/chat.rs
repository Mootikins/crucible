use crate::web::events::ChatEvent;
use crate::web::services::daemon::AppState;
use crate::web::{error::WebResultExt, WebError};
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

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .filter_map(|result| result.ok())
        .map(|event| {
            let chat_event = ChatEvent::from_daemon_event(&event);
            let event_name = chat_event.event_name();
            let data = serde_json::to_string(&chat_event).unwrap_or_default();
            Ok(Event::default().event(event_name).data(data))
        });

    Ok(Sse::new(stream))
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
                        "request": crate::web::events::normalize_interaction(&wire),
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

async fn interaction_respond(
    State(state): State<AppState>,
    Json(req): Json<InteractionResponseRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let response: crucible_core::interaction::InteractionResponse =
        serde_json::from_value(req.response)
            .map_err(|e| WebError::Chat(format!("Invalid interaction response: {e}")))?;

    state
        .daemon
        .session_interaction_respond(&req.session_id, &req.request_id, response)
        .await
        .daemon_err()?;

    Ok(Json(serde_json::json!({ "ok": true })))
}
