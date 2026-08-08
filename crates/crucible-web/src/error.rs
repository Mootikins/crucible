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

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

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
            WebError::Validation(e) => (StatusCode::UNPROCESSABLE_ENTITY, e.clone()),
            WebError::NotFound(e) => (StatusCode::NOT_FOUND, e.clone()),
            WebError::Forbidden(e) => (StatusCode::FORBIDDEN, e.clone()),
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

/// Extension trait to convert `Result<T, E: Display>` into `WebResult<T>`.
///
/// Replaces the verbose `.map_err(|e| WebError::Daemon(e.to_string()))` pattern.
pub trait WebResultExt<T> {
    fn daemon_err(self) -> Result<T>;
}

impl<T, E: std::fmt::Display> WebResultExt<T> for std::result::Result<T, E> {
    fn daemon_err(self) -> Result<T> {
        self.map_err(|e| {
            let message = e.to_string();
            // JSON-RPC `-32602` is INVALID_PARAMS: the caller sent something the
            // daemon refused, which is a 4xx. Mapping it to `Daemon` told the
            // client "upstream is broken" (502) for its own bad input — e.g.
            // refusing `/` as a session kiln, a deliberate containment check,
            // reported as a gateway failure. One route mapped this correctly and
            // every other one did not, so it belongs here rather than per-route.
            if message.contains("-32602") {
                WebError::Validation(message)
            } else {
                WebError::Daemon(message)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invalid_params_rpc_error_is_a_client_error_not_a_gateway_failure() {
        // The daemon's containment checks refuse bad input with `-32602`.
        // Reporting that as 502 tells the caller the upstream is broken when
        // the upstream worked exactly as designed.
        let refusal: std::result::Result<(), String> = Err(
            r#"RPC error: {"code":-32602,"message":"Refusing '/' as a session kiln: it is the filesystem root"}"#
                .to_string(),
        );

        let status = refusal.daemon_err().unwrap_err().into_response().status();

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn a_genuine_daemon_failure_is_still_a_gateway_error() {
        let broken: std::result::Result<(), String> =
            Err("connection refused (os error 111)".to_string());

        let status = broken.daemon_err().unwrap_err().into_response().status();

        assert_eq!(status, StatusCode::BAD_GATEWAY);
    }
}
