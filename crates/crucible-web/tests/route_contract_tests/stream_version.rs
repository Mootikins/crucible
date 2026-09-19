//! Task G6: every versioned stream names its protocol on the wire.
//!
//! Four streams are versioned — the chat events, the filesystem events, the
//! surface changes and the plugin publications. Each answers with the
//! `X-Crucible-Stream-Version` header AND opens its body with a
//! `stream_version` frame, because the browser's `EventSource` cannot read
//! response headers and the client's fail-closed gate reads the frame.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use super::shared::{build_mock_state, build_test_app, start_mock_daemon};

/// Opens one stream endpoint and answers its headers plus the first body
/// bytes, under a short deadline (an SSE body never ends).
async fn open(uri: &str) -> (Option<String>, String) {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{uri} did not answer 200"
    );

    let header = response
        .headers()
        .get("x-crucible-stream-version")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let mut body = response.into_body();
    let mut collected = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while !collected.windows(7).any(|w| w == b"version") {
        match tokio::time::timeout_at(deadline, body.frame()).await {
            Ok(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    collected.extend_from_slice(data);
                }
            }
            _ => break,
        }
    }
    (header, String::from_utf8_lossy(&collected).into_owned())
}

/// The one assertion every versioned stream must satisfy: the header names
/// version 1, and the body opens with the mirroring frame the browser can
/// actually read.
#[tokio::test]
async fn every_versioned_stream_names_its_protocol_twice() {
    for uri in [
        "/api/chat/events/test-session-001",
        "/api/fs/events",
        "/api/surfaces/events",
        "/api/plugins/events",
    ] {
        let (header, body) = open(uri).await;

        assert_eq!(header.as_deref(), Some("1"), "{uri}: the version header");
        assert!(
            body.contains("event: stream_version") && body.contains("\"version\":1"),
            "{uri}: the body must open with the stream_version frame, got: {body}"
        );
    }
}

/// The frame is data, not a comment: an `EventSource` that installs no
/// listener for it ignores it, which is what lets a pre-G6 browser keep
/// reading a post-G6 stream.
#[tokio::test]
async fn the_handshake_frame_is_an_ignorable_named_event() {
    let (_header, body) = open("/api/fs/events").await;

    let first_event = body
        .lines()
        .find(|line| line.starts_with("event:"))
        .expect("the body opens with an event");
    assert_eq!(first_event, "event: stream_version");
    // And it carries nothing a payload decoder would mistake for content.
    assert!(body.contains("data: {\"version\":1}"), "frame: {body}");
}

/// The payload decoder is untouched by the gate: a chat replay frame still
/// follows the handshake, so a client that ignores the frame sees exactly the
/// pre-G6 stream behind it.
#[tokio::test]
async fn the_chat_stream_replays_behind_the_handshake() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let mut body = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/chat/events/test-session-001?after=3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .into_body();

    let mut collected = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while !String::from_utf8_lossy(&collected).contains("Second answer") {
        match tokio::time::timeout_at(deadline, body.frame()).await {
            Ok(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    collected.extend_from_slice(data);
                }
            }
            _ => break,
        }
    }
    let text = String::from_utf8_lossy(&collected).into_owned();
    let handshake_at = text.find("event: stream_version").expect("handshake first");
    let replay_at = text.find("Second answer").expect("a replayed payload");
    assert!(handshake_at < replay_at, "frames: {text}");
}
