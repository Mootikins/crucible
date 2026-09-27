//! The system stream (`GET /api/events/system`) and its plugin alias.
//!
//! The daemon sends `publication_changed` and `proposal_changed` on the
//! system session. The general route forwards both. The alias
//! `GET /api/plugins/events` forwards only the publications, because the
//! client that reads it knows no other event.

use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use crucible_core::protocol::SystemPayload;
use crucible_daemon::SessionEvent;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use super::shared::{build_mock_state, build_test_app, start_mock_daemon};

const PROPOSAL_ID: &str = "0b8f4a0e-7c1d-4c55-9a39-5d1f0a2e6b11";

/// Read frames off `body` until `done` holds or two seconds go by.
async fn read_until(body: &mut Body, done: impl Fn(&str) -> bool) -> String {
    let mut collected = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while !done(&String::from_utf8_lossy(&collected)) {
        match tokio::time::timeout_at(deadline, body.frame()).await {
            Ok(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    collected.extend_from_slice(data);
                }
            }
            _ => break,
        }
    }
    String::from_utf8_lossy(&collected).into_owned()
}

/// Open `uri`, send a proposal event and then a publication event on the
/// system session, and answer the text that the stream carried.
async fn stream_after_both_events(uri: &str) -> String {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state.clone());
    let mut body = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .into_body();
    // The route subscribes before it answers, so the version frame proves
    // that the next event has a receiver.
    read_until(&mut body, |text| text.contains("stream_version")).await;

    state
        .events
        .publish_for_tests(SessionEvent::new(
            "system",
            SystemPayload::PROPOSAL_CHANGED,
            json!({ "id": PROPOSAL_ID }),
        ))
        .await;
    state
        .events
        .publish_for_tests(SessionEvent::new(
            "system",
            SystemPayload::PUBLICATION_CHANGED,
            json!({ "plugin": "kanban", "key": "kanban:board" }),
        ))
        .await;

    // The publication is sent last, so its frame proves that the stream
    // already forwarded or dropped the proposal event.
    read_until(&mut body, |text| text.contains("kanban:board")).await
}

#[tokio::test]
async fn the_system_stream_carries_proposal_changed() {
    let text = stream_after_both_events("/api/events/system").await;

    assert!(
        text.contains(&format!("event: {}", SystemPayload::PROPOSAL_CHANGED))
            && text.contains(&format!("data: {{\"id\":\"{PROPOSAL_ID}\"}}")),
        "no proposal frame: {text}"
    );
    assert!(
        text.contains("event: publication_changed"),
        "no publication frame: {text}"
    );
}

#[tokio::test]
async fn the_plugin_alias_carries_only_publications() {
    let text = stream_after_both_events("/api/plugins/events").await;

    assert!(
        text.contains("event: publication_changed"),
        "no publication frame: {text}"
    );
    assert!(
        !text.contains(SystemPayload::PROPOSAL_CHANGED),
        "the alias forwarded a proposal: {text}"
    );
}

async fn assert_gap_reaches_projection(uri: &str, event: &str, data: serde_json::Value) {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let mut body = build_test_app(state.clone())
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .into_body();
    read_until(&mut body, |text| text.contains("stream_version")).await;
    // Do not poll the browser while overflowing the bounded broker ring.
    for _ in 0..300 {
        state
            .events
            .publish_for_tests(SessionEvent::new("system", event, data.clone()))
            .await;
    }
    let text = read_until(&mut body, |text| text.contains("gap-probe")).await;
    assert!(
        text.contains("event: stream_gap"),
        "{uri} silently discarded lag: {text}"
    );
    assert!(
        text.contains("gap-probe"),
        "{uri} stopped after its gap: {text}"
    );

    state
        .events
        .publish_for_tests(SessionEvent::typed(
            "*",
            SystemPayload::StreamGap { dropped: 7 },
        ))
        .await;
    let text = read_until(&mut body, |text| text.contains("\"dropped\":7")).await;
    assert!(
        text.contains("event: stream_gap"),
        "{uri} dropped an upstream gap: {text}"
    );
}

#[tokio::test]
async fn system_projection_reports_gaps() {
    assert_gap_reaches_projection(
        "/api/events/system",
        "publication_changed",
        json!({"plugin":"gap-probe", "key":"rows"}),
    )
    .await;
}

#[tokio::test]
async fn plugin_alias_reports_gaps() {
    assert_gap_reaches_projection(
        "/api/plugins/events",
        "publication_changed",
        json!({"plugin":"gap-probe", "key":"rows"}),
    )
    .await;
}

#[tokio::test]
async fn filesystem_projection_reports_gaps() {
    assert_gap_reaches_projection(
        "/api/fs/events",
        "file_changed",
        json!({"path":"/gap-probe.md", "kind":"modified"}),
    )
    .await;
}

#[tokio::test]
async fn surface_projection_reports_gaps() {
    assert_gap_reaches_projection(
        "/api/surfaces/events",
        "surface_changed",
        json!({"plugin":"gap-probe", "name":"rows", "version":1}),
    )
    .await;
}
