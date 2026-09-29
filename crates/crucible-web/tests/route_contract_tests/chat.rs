//! Chat Route Contract Tests
//!
//! The SSE stream carries the daemon's own `{event, data}` pair — one event
//! vocabulary, not a second one re-encoded for the browser (see
//! `crates/crucible-web/src/routes/chat.rs::to_sse`). There is no `ChatEvent`
//! type left to test here: the wire tests below exercise the stream itself.

use crucible_core::protocol::rpc::RpcMethod;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{build_state, build_test_app, start_mock_daemon};

// =========================================================================
// Chat Route Contract Tests (with mock daemon)
// =========================================================================

#[tokio::test]
async fn chat_send_empty_message_returns_400() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/send")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"session_id": "s1", "content": "  "}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_send_valid_message_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/send")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"session_id": "test-session-001", "content": "Hello"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json.get("message_id").is_some(),
        "Response must contain message_id"
    );
}

#[tokio::test]
async fn chat_send_missing_fields_returns_422() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    // Missing content field
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/send")
                .header("content-type", "application/json")
                .body(Body::from(json!({"session_id": "s1"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn chat_send_invalid_json_returns_error() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/send")
                .header("content-type", "application/json")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum returns 400 for JSON parse errors
    assert!(
        response.status().is_client_error(),
        "Invalid JSON should return client error, got: {}",
        response.status()
    );
}

// =========================================================================
// The chat event stream: replay from a seq cursor (Task G5)
// =========================================================================

use crucible_daemon::{DaemonClient, SessionEvent};
use http_body_util::BodyExt;
use std::time::Duration;

/// Drains body frames off an SSE response until `predicate` accepts the text
/// collected so far, or a short deadline passes. An SSE body never ends, so
/// `to_bytes` would wait forever. Answers the state (for the broker the route
/// fans out from) and the text.
async fn read_stream(
    client: DaemonClient,
    uri: &str,
    headers: &[(&str, &str)],
    predicate: impl Fn(&str) -> bool,
) -> (
    crucible_web::services::daemon::AppState,
    String,
    axum::body::Body,
) {
    let state = build_state(client);
    let app = build_test_app(state.clone());
    let mut request = Request::builder().method("GET").uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let body = app
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .into_body();

    let (text, body) = drain(body, &predicate).await;
    (state, text, body)
}

/// Reads more frames off a body already open, under the same deadline.
async fn drain(
    mut body: axum::body::Body,
    predicate: &impl Fn(&str) -> bool,
) -> (String, axum::body::Body) {
    let mut collected = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while !predicate(&String::from_utf8_lossy(&collected)) {
        match tokio::time::timeout_at(deadline, body.frame()).await {
            Ok(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    collected.extend_from_slice(data);
                }
            }
            _ => break,
        }
    }
    (String::from_utf8_lossy(&collected).into_owned(), body)
}

/// Every `id:` the stream stamped, in order — the seqs the client would read
/// back off `MessageEvent.lastEventId`.
fn frame_ids(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.starts_with("id:"))
        .map(|line| line.trim_start_matches("id:").trim().to_string())
        .collect()
}

/// A stamped envelope for the broker, as the daemon's forwarder delivers one.
fn stamped(seq: u64, event: &str, data: Value) -> SessionEvent {
    let mut event = SessionEvent::new("test-session-001", event, data);
    event.seq = Some(seq);
    event
}

/// The stream replays the persisted tail past the cursor and stamps each
/// event's seq as the SSE `id:` — the field a reconnecting client names.
#[tokio::test]
async fn chat_events_replays_past_the_cursor_and_stamps_seq_ids() {
    let (mock, client) = start_mock_daemon().await;

    // Cursor 1: the mock's log holds seqs 1-4, so the tail is 2, 3, 4.
    let (_state, text, _body) = read_stream(
        client,
        "/api/chat/events/test-session-001?after=1",
        &[],
        |text| text.matches("id:").count() >= 3,
    )
    .await;
    assert_eq!(frame_ids(&text), vec!["2", "3", "4"], "frames: {text}");
    assert!(
        text.contains("Second turn") && text.contains("Second answer"),
        "the replay carries the payload, not just the seq: {text}"
    );

    // The cursor reached the daemon as the wire `after`, not a rewrite.
    let params = mock
        .received_params(RpcMethod::SessionEventsAfter)
        .expect("the route replayed");
    assert_eq!(params["after"], json!(1));
    assert_eq!(params["session_id"], json!("test-session-001"));
}

/// A live event with a seq at or below the replayed max never reaches the
/// client a second time; one above it does, stamped. Both travel one open
/// connection, which is the ordering the route promises: the replayed tail
/// first, live strictly above it.
#[tokio::test]
async fn chat_events_skips_live_events_the_replay_already_covered() {
    let (_mock, client) = start_mock_daemon().await;
    let (state, replayed, body) = read_stream(
        client,
        "/api/chat/events/test-session-001?after=3",
        &[],
        |text| text.matches("id:").count() >= 1,
    )
    .await;
    // Cursor 3: the tail is seq 4 alone.
    assert_eq!(frame_ids(&replayed), vec!["4"], "replay: {replayed}");

    // Below the tail (long covered) and at the tail (the tail itself).
    state
        .events
        .publish_for_tests(stamped(
            2,
            "message_complete",
            json!({
                "message_id": "msg-001", "full_response": "First answer",
            }),
        ))
        .await;
    state
        .events
        .publish_for_tests(stamped(
            4,
            "user_message",
            json!({
                "message_id": "msg-002", "content": "Second turn",
            }),
        ))
        .await;
    // Above the tail: forwarded, stamped.
    state
        .events
        .publish_for_tests(stamped(
            5,
            "message_complete",
            json!({
                "message_id": "msg-003", "full_response": "Third answer",
            }),
        ))
        .await;

    // The SAME connection: the live frames land in the receiver the route
    // opened before it replayed, which is the whole ordering claim.
    let (live, _body) = drain(body, &|text: &str| text.contains("Third answer")).await;
    assert_eq!(frame_ids(&live), vec!["5"], "live frames: {live}");
    assert!(
        !live.contains("First answer") && !live.contains("Second turn"),
        "the covered events did not reach the client a second time: {live}"
    );
}

/// The browser's own retry of one source sends `Last-Event-ID` (it cannot set
/// `?after=` on that retry); the route honours both spellings of the cursor.
#[tokio::test]
async fn chat_events_accepts_the_last_event_id_header_as_the_cursor() {
    let (mock, client) = start_mock_daemon().await;

    let (_state, text, _body) = read_stream(
        client,
        "/api/chat/events/test-session-001",
        &[("Last-Event-ID", "3")],
        |text| text.matches("id:").count() >= 1,
    )
    .await;
    assert_eq!(frame_ids(&text), vec!["4"], "frames: {text}");

    let params = mock
        .received_params(RpcMethod::SessionEventsAfter)
        .expect("the route replayed");
    assert_eq!(params["after"], json!(3));
}

/// A request with no cursor replays nothing: a fresh viewer hydrates from the
/// history route, and replaying the whole log at it would duplicate every
/// turn it is about to fold.
#[tokio::test]
async fn chat_events_without_a_cursor_replays_nothing() {
    let (mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    // The handler runs to the point of building the stream; the body needs
    // no draining, because the claim is about a call that was never made.
    let _response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/chat/events/test-session-001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        mock.received_params(RpcMethod::SessionEventsAfter)
            .is_none(),
        "a cursor-less request must not replay"
    );
}

/// The `data:` payload of a live event is the daemon's own `{event, data}`
/// pair, not a re-tagged `{"type": ...}` object — `ChatEvent` is gone, and
/// the SSE `event:` field carries the name instead of a JSON tag.
#[tokio::test]
async fn chat_events_data_payload_is_the_daemon_event_data_pair() {
    let (_mock, client) = start_mock_daemon().await;
    let (state, _replayed, body) = read_stream(
        client,
        "/api/chat/events/test-session-001?after=4",
        &[],
        |_| true,
    )
    .await;

    state
        .events
        .publish_for_tests(stamped(5, "text_delta", json!({ "content": "hi" })))
        .await;

    let (live, _body) = drain(body, &|text: &str| text.contains("event: text_delta")).await;
    let frame = live
        .split("\n\n")
        .find(|frame| frame.contains("event: text_delta"))
        .expect("a text_delta frame");
    let data = frame
        .lines()
        .find_map(|l| l.strip_prefix("data: "))
        .expect("a data: line");
    let parsed: Value = serde_json::from_str(data).unwrap();
    assert_eq!(
        parsed,
        json!({ "event": "text_delta", "data": { "content": "hi" } }),
        "the data: payload is {{event, data}}, not a {{type, ...}} tag: {live}"
    );
}
