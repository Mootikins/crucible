//! WebError Response Contract Tests

use axum::http::StatusCode;
use crucible_web::WebError;
use serde_json::Value;

#[test]
fn web_error_config_returns_500() {
    let err = WebError::Config("bad config".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn web_error_chat_returns_400() {
    let err = WebError::Chat("invalid message".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn web_error_daemon_returns_502() {
    let err = WebError::Daemon("daemon unreachable".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[test]
fn web_error_validation_returns_422() {
    let err = WebError::Validation("invalid input".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn web_error_not_found_returns_404() {
    let err = WebError::NotFound("missing resource".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn web_error_internal_returns_500() {
    let err = WebError::Internal("unexpected failure".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn web_error_body_contains_error_code_and_message() {
    let err = WebError::Chat("test error message".to_string());
    let response = axum::response::IntoResponse::into_response(err);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Contract: error responses have { "error": { "code": N, "message": "..." } }
    assert!(
        json.get("error").is_some(),
        "Response must have 'error' key"
    );
    assert_eq!(json["error"]["code"], 400);
    assert_eq!(json["error"]["message"], "test error message");
}

#[tokio::test]
async fn web_error_io_returns_500_with_message() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let err = WebError::Io(io_err);
    let response = axum::response::IntoResponse::into_response(err);

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], 500);
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("file missing"));
}
