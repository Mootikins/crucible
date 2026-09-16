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

    /// The resource is here; this endpoint cannot represent it. Distinct from
    /// [`WebError::NotFound`] because collapsing the two sends the caller
    /// looking for a missing file that is sitting right there — see the text
    /// read in `routes/kiln.rs`.
    #[error("Unsupported media type: {0}")]
    UnsupportedMediaType(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Internal error: {0}")]
    Internal(String),

    /// The file moved on since the caller read it.
    ///
    /// Carries the hash on disk NOW, so a client can decide what to do — re-read,
    /// or write its version beside the note — without a second round trip that
    /// would race the same way.
    #[error("The file changed since it was read")]
    StaleBase { current_hash: String },
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        // A stale base answers with the hash it holds, not only a message:
        // the whole point is to tell the caller what it is racing against.
        // Shaped like the PATCH refusal so one client branch reads both.
        if let WebError::StaleBase { current_hash } = &self {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "ok": false,
                    "current_hash": current_hash,
                    "error": {
                        "code": StatusCode::CONFLICT.as_u16(),
                        "message": self.to_string(),
                    }
                })),
            )
                .into_response();
        }
        let (status, message) = match &self {
            WebError::Config(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.clone()),
            WebError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            WebError::Chat(e) => (StatusCode::BAD_REQUEST, e.clone()),
            WebError::Daemon(e) => (StatusCode::BAD_GATEWAY, e.clone()),
            WebError::Validation(e) => (StatusCode::UNPROCESSABLE_ENTITY, e.clone()),
            WebError::NotFound(e) => (StatusCode::NOT_FOUND, e.clone()),
            WebError::UnsupportedMediaType(e) => (StatusCode::UNSUPPORTED_MEDIA_TYPE, e.clone()),
            WebError::Forbidden(e) => (StatusCode::FORBIDDEN, e.clone()),
            WebError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.clone()),
            // Handled above, with its hash.
            WebError::StaleBase { .. } => (StatusCode::CONFLICT, self.to_string()),
        };

        error_response(status, &message)
    }
}

/// Build the one JSON error body every web reply uses: `{"error": {"code", "message"}}`.
pub fn error_response(status: StatusCode, message: &str) -> Response {
    let body = Json(json!({
        "error": {
            "code": status.as_u16(),
            "message": message,
        }
    }));
    (status, body).into_response()
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
            let raw = e.to_string();
            let (code, message) = rpc_error_parts(&raw);
            // JSON-RPC `-32602` is INVALID_PARAMS: the caller sent something the
            // daemon refused, which is a 4xx. Mapping it to `Daemon` told the
            // client "upstream is broken" (502) for its own bad input — e.g.
            // refusing `/` as a session kiln, a deliberate containment check,
            // reported as a gateway failure. One route mapped this correctly and
            // every other one did not, so it belongs here rather than per-route.
            if code == Some(-32602) || (code.is_none() && raw.contains("-32602")) {
                WebError::Validation(message)
            } else {
                WebError::Daemon(message)
            }
        })
    }
}

/// The code and the human message inside a daemon refusal.
///
/// `DaemonClient` reports a JSON-RPC error as `RPC error: {"code":…,"message":…}`
/// — the envelope, serialised, behind a prefix. Passed through, that is what
/// the browser toasted: a status and a JSON blob, where the daemon had written
/// one plain sentence. The sentence is the part a person can act on, so it is
/// what the response body carries; the code decides the status. Anything
/// that is not that shape (a socket error, a plain string) passes unchanged.
fn rpc_error_parts(raw: &str) -> (Option<i64>, String) {
    let Some(envelope) = raw.strip_prefix("RPC error: ") else {
        return (None, raw.to_string());
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(envelope) else {
        return (None, raw.to_string());
    };
    let code = value.get("code").and_then(serde_json::Value::as_i64);
    let message = value
        .get("message")
        .and_then(serde_json::Value::as_str)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| raw.to_string());
    (code, message)
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

    /// The body carries the daemon's sentence, not the serialised envelope
    /// it arrived in: a person reads "root is not a registered project…",
    /// never `RPC error: {"code":-32602,"message":"…"}`.
    #[tokio::test]
    async fn the_body_carries_the_daemons_message_not_the_rpc_envelope() {
        let refusal: std::result::Result<(), String> = Err(
            r#"RPC error: {"code":-32602,"message":"root is not a registered project or a session's own workspace folder"}"#
                .to_string(),
        );

        let response = refusal.daemon_err().unwrap_err().into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            body["error"]["message"],
            "root is not a registered project or a session's own workspace folder"
        );
    }

    /// A daemon-side failure keeps its sentence too, at 502.
    #[tokio::test]
    async fn a_daemon_failure_envelope_is_unwrapped_at_502() {
        let failure: std::result::Result<(), String> =
            Err(r#"RPC error: {"code":-32000,"message":"session.get exploded"}"#.to_string());

        let response = failure.daemon_err().unwrap_err().into_response();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["message"], "session.get exploded");
    }

    #[test]
    fn a_genuine_daemon_failure_is_still_a_gateway_error() {
        let broken: std::result::Result<(), String> =
            Err("connection refused (os error 111)".to_string());

        let status = broken.daemon_err().unwrap_err().into_response().status();

        assert_eq!(status, StatusCode::BAD_GATEWAY);
    }
}
