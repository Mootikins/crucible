//! Error types for crucible-web

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub type Result<T> = std::result::Result<T, WebError>;

#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Chat service error: {0}")]
    Chat(String),

    #[error("Daemon RPC error: {0}")]
    Daemon(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            WebError::Config(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.clone()),
            WebError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            WebError::Chat(e) => (StatusCode::BAD_REQUEST, e.clone()),
            WebError::Daemon(e) => (StatusCode::BAD_GATEWAY, e.clone()),
            WebError::NotFound(e) => (StatusCode::NOT_FOUND, e.clone()),
            WebError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.clone()),
        };

        let body = Json(json!({
            "error": {
                "code": status.as_u16(),
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}
