//! Chat API endpoints with SSE streaming

use crate::services::ChatService;
use crate::{ChatEvent, WebError};
use axum::{
    extract::State,
    response::sse::{Event, Sse},
    routing::post,
    Json, Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

/// Shared state for chat routes
pub type ChatState = Arc<ChatService>;

/// Request body for chat messages
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
}

pub fn chat_routes(state: ChatState) -> Router {
    Router::new()
        .route("/api/chat", post(chat_handler))
        .route(
            "/api/interaction/respond",
            post(interaction_respond_handler),
        )
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct InteractionResponseRequest {
    request_id: String,
    response: serde_json::Value,
}

async fn interaction_respond_handler(
    State(chat_service): State<ChatState>,
    Json(request): Json<InteractionResponseRequest>,
) -> Result<(), WebError> {
    chat_service
        .submit_interaction_response(request.request_id, request.response)
        .await;
    Ok(())
}

/// Handle chat message and return SSE stream
async fn chat_handler(
    State(chat_service): State<ChatState>,
    Json(request): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, WebError> {
    // Validate message
    if request.message.trim().is_empty() {
        return Err(WebError::Chat("Message cannot be empty".to_string()));
    }

    // Subscribe to events before sending message
    let rx = chat_service.subscribe();

    // Send message in background task
    let service = chat_service.clone();
    let message = request.message.clone();
    tokio::spawn(async move {
        if let Err(e) = service.send_message(message).await {
            tracing::error!("Chat error: {}", e);
            // Error will be sent via event channel
            let _ = service.subscribe(); // Get a sender indirectly
        }
    });

    // Convert broadcast receiver to SSE stream
    let stream = BroadcastStream::new(rx)
        .filter_map(|result| match result {
            Ok(event) => Some(event),
            Err(e) => {
                tracing::warn!("Broadcast receive error: {}", e);
                None
            }
        })
        .map(|event: ChatEvent| {
            Ok(Event::default()
                .event(event_type(&event))
                .data(serde_json::to_string(&event).unwrap_or_default()))
        })
        // Stop stream after MessageComplete or Error
        .take_while(|result| {
            if let Ok(event) = result {
                // Check event type from the SSE event
                let event_str = format!("{:?}", event);
                !event_str.contains("message_complete") && !event_str.contains("error")
            } else {
                true
            }
        });

    Ok(Sse::new(stream))
}

fn event_type(event: &ChatEvent) -> &'static str {
    match event {
        ChatEvent::Token { .. } => "token",
        ChatEvent::ToolCall { .. } => "tool_call",
        ChatEvent::ToolResult { .. } => "tool_result",
        ChatEvent::Thinking { .. } => "thinking",
        ChatEvent::MessageComplete { .. } => "message_complete",
        ChatEvent::Error { .. } => "error",
        ChatEvent::InteractionRequested { .. } => "interaction_requested",
    }
}
