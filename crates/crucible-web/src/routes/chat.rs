use crate::events::ChatEvent;
use crate::services::daemon::AppState;
use crate::WebError;
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
        .map_err(|e| WebError::Daemon(e.to_string()))?;

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
        .map_err(|e| WebError::Daemon(e.to_string()))?;

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
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}
