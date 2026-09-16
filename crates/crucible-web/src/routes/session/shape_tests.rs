//! Every session route answers the struct it declares.
//!
//! One test per handler, because a test that reads `status == 200` proves
//! nothing about the reply: the routes answered `serde_json::Value` until this
//! group named its shapes, and a renamed field would have passed every one of
//! them. Each test decodes the body into the handler's own reply struct, which
//! fails on a missing or retyped field, and then reads one field back.
//!
//! Split from `tests.rs` for the reason `search_scope_tests.rs` was: that file
//! is at its size budget.
//!
//! The two round-trip tests at the end are the stronger claim. They take the
//! daemon object the mock answers, push it through the reply struct, and
//! demand the same JSON back — so naming the reply cannot have dropped a
//! field or added one.

use super::*;
use crate::routes::helpers::ModelsResponse;
use crate::test_support::request_json;
use axum::http::StatusCode;
use serde_json::json;

/// Drive one request and decode the body into the reply struct `T`.
async fn shape<T: serde::de::DeserializeOwned>(
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> T {
    let (status, json) = request_json(method, uri, body).await;
    assert_eq!(status, StatusCode::OK, "{method} {uri}: {json}");
    serde_json::from_value(json.clone()).unwrap_or_else(|e| {
        panic!("{method} {uri} answered a body the struct cannot read: {e}\n{json}")
    })
}

const SESSION: &str = "/api/session/test-session-001";

#[tokio::test]
async fn create_session_answers_the_declared_shape() {
    let row: SessionRow = shape("POST", "/api/session", Some(json!({}))).await;
    assert_eq!(row.session_id, "test-session-001");
}

#[tokio::test]
async fn list_sessions_answers_the_declared_shape() {
    let list: SessionListResponse = shape("GET", "/api/session/list", None).await;
    assert_eq!(list.total, 0);
}

#[tokio::test]
async fn search_sessions_answers_the_declared_shape() {
    let found: SessionSearchResponse = shape("GET", "/api/sessions/search?q=x", None).await;
    assert_eq!(found.matches[0].session_id, "s1");
}

#[tokio::test]
async fn get_session_answers_the_declared_shape() {
    let row: SessionRow = shape("GET", SESSION, None).await;
    // `type` on the wire, and the model is nested rather than flat — the
    // divergence `SessionRow` exists to absorb.
    assert_eq!(row.session_type, "chat");
    assert_eq!(row.agent_model, None);
}

#[tokio::test]
async fn delete_session_answers_the_declared_shape() {
    let deleted: DeleteResponse = shape("DELETE", SESSION, None).await;
    assert!(deleted.deleted);
}

#[tokio::test]
async fn get_session_history_answers_the_declared_shape() {
    let history: SessionHistoryResponse = shape("GET", &format!("{SESSION}/history"), None).await;
    assert_eq!(history.history[0].event, "user_message");
}

#[tokio::test]
async fn pause_session_answers_the_declared_shape() {
    let paused: SessionLifecycleResponse = shape("POST", &format!("{SESSION}/pause"), None).await;
    assert_eq!(paused.state, "paused");
}

#[tokio::test]
async fn resume_session_answers_the_declared_shape() {
    let resumed: ResumeSessionResponse = shape("POST", &format!("{SESSION}/resume"), None).await;
    // The warm path, because the mock's `session.resume` succeeds.
    match resumed {
        ResumeSessionResponse::Live(live) => assert_eq!(live.state, "active"),
        ResumeSessionResponse::Restored(_) => panic!("the warm path answered the stored history"),
    }
}

#[tokio::test]
async fn end_session_answers_the_declared_shape() {
    let ended: SessionLifecycleResponse = shape("POST", &format!("{SESSION}/end"), None).await;
    assert_eq!(ended.state, "ended");
}

#[tokio::test]
async fn archive_session_answers_the_declared_shape() {
    let archived: ArchiveResponse = shape("POST", &format!("{SESSION}/archive"), None).await;
    assert!(archived.archived);
}

#[tokio::test]
async fn unarchive_session_answers_the_declared_shape() {
    let archived: ArchiveResponse = shape("POST", &format!("{SESSION}/unarchive"), None).await;
    assert!(!archived.archived);
}

#[tokio::test]
async fn cancel_session_answers_the_declared_shape() {
    let cancelled: CancelledResponse = shape("POST", &format!("{SESSION}/cancel"), None).await;
    assert!(cancelled.cancelled);
}

#[tokio::test]
async fn list_models_answers_the_declared_shape() {
    let models: ModelsResponse = shape("GET", &format!("{SESSION}/models"), None).await;
    assert_eq!(models.models, vec!["llama3.2", "mistral"]);
}

#[tokio::test]
async fn switch_model_answers_the_declared_shape() {
    let ok: OkResponse = shape(
        "POST",
        &format!("{SESSION}/model"),
        Some(json!({"model_id": "mistral"})),
    )
    .await;
    assert!(ok.ok);
}

#[tokio::test]
async fn list_modes_answers_the_declared_shape() {
    let modes: SessionModesResponse = shape("GET", &format!("{SESSION}/modes"), None).await;
    assert_eq!(modes.current_mode_id, "ask");
    // The daemon's descriptor carries a review policy the browser's own type
    // never declared; the document now does.
    assert_eq!(modes.modes[0].review_policy, ReviewPolicyRow::PreWrite);
}

#[tokio::test]
async fn list_knobs_answers_the_declared_shape() {
    let knobs: SessionKnobsResponse = shape("GET", &format!("{SESSION}/knobs"), None).await;
    assert_eq!(knobs.knobs[0].id, "model");
}

#[tokio::test]
async fn connect_kiln_answers_the_declared_shape() {
    let scope: SessionScopeResponse = shape(
        "POST",
        &format!("{SESSION}/kilns/connect"),
        Some(json!({"kiln": "extra-kiln"})),
    )
    .await;
    assert_eq!(scope.kilns, vec!["test-kiln", "extra-kiln"]);
}

#[tokio::test]
async fn disconnect_kiln_answers_the_declared_shape() {
    let scope: SessionScopeResponse = shape(
        "POST",
        &format!("{SESSION}/kilns/disconnect"),
        Some(json!({"kiln": "extra-kiln"})),
    )
    .await;
    assert_eq!(scope.kilns, vec!["test-kiln"]);
}

#[tokio::test]
async fn set_workspace_answers_the_declared_shape() {
    let scope: SessionScopeResponse = shape(
        "PUT",
        &format!("{SESSION}/workspace"),
        Some(json!({"workspace": "/repos/crucible"})),
    )
    .await;
    assert_eq!(scope.workspace.as_deref(), Some("/repos/crucible"));
}

#[tokio::test]
async fn set_mode_answers_the_declared_shape() {
    let ok: OkResponse = shape(
        "POST",
        &format!("{SESSION}/mode"),
        Some(json!({"mode": "plan"})),
    )
    .await;
    assert!(ok.ok);
}

#[tokio::test]
async fn get_mode_answers_the_declared_shape() {
    let mode: ModeResponse = shape("GET", &format!("{SESSION}/mode"), None).await;
    assert_eq!(mode.mode.as_deref(), Some("plan"));
}

#[tokio::test]
async fn set_session_title_answers_the_declared_shape() {
    let ok: OkResponse = shape(
        "PUT",
        &format!("{SESSION}/title"),
        Some(json!({"title": "Merkle tree sync design"})),
    )
    .await;
    assert!(ok.ok);
}

#[tokio::test]
async fn auto_title_answers_the_declared_shape() {
    let titled: TitleResponse = shape("POST", &format!("{SESSION}/auto-title"), None).await;
    assert_eq!(titled.title, "Merkle tree sync design");
}

#[tokio::test]
async fn list_providers_answers_the_declared_shape() {
    let providers: ProvidersResponse = shape("GET", "/api/providers", None).await;
    assert!(providers.providers.is_empty());
}

// =========================================================================
// The reply structs carry the daemon's object through unchanged
// =========================================================================

/// `session.get`'s own object survives `SessionRow` field for field.
///
/// The fixture is the daemon's projection (`server/session/list.rs:302`),
/// including the nested `agent` record whose other fields no client reads.
/// Naming the reply had to keep every one of them, so this compares the whole
/// object rather than one field.
#[test]
fn a_session_row_writes_back_the_object_session_get_sent() {
    let sent = json!({
        "session_id": "test-session-001",
        "type": "chat",
        "kilns": ["test-kiln"],
        "workspace": "/tmp/test-kiln",
        "state": "active",
        "started_at": "2026-01-01T00:00:00Z",
        "title": null,
        "continued_from": null,
        "parent_session_id": null,
        "agent": {
            "agent_type": "internal",
            "provider": "ollama",
            "model": "ollama:llama3.2",
            "mode": "edit",
            "system_prompt": "",
            "precognition_enabled": true,
            "context_strategy": "recent"
        }
    });

    let row: SessionRow = serde_json::from_value(sent.clone()).expect("the row reads the object");
    assert_eq!(
        serde_json::to_value(row).expect("the row writes JSON"),
        sent
    );
}

/// `session.list`'s row survives the same struct, including the three fields
/// `session.get` never sends.
///
/// A field absent from one shape must stay absent, not arrive as `null`: that
/// is what the `Option<Option<T>>` fields buy, and it is what keeps one struct
/// honest about two daemon objects.
#[test]
fn a_session_row_writes_back_the_object_session_list_sent() {
    let sent = json!({
        "session_id": "test-session-001",
        "type": "chat",
        "kilns": ["test-kiln"],
        "workspace": null,
        "state": "active",
        "started_at": "2026-01-01T00:00:00Z",
        "last_activity": null,
        "title": "Merkle tree sync design",
        "agent_model": "ollama:llama3.2",
        "event_count": 12,
        "archived": false,
        "parent_session_id": null
    });

    let row: SessionRow = serde_json::from_value(sent.clone()).expect("the row reads the object");
    let written = serde_json::to_value(row).expect("the row writes JSON");
    assert_eq!(written, sent);
    // The three fields `session.get` omits are absent there and present here,
    // and neither spelling turns into the other.
    assert!(written.get("agent").is_none());
}

/// The scope echo survives its struct. `workspace` is `null` for a session
/// with no workspace, and `null` is not the same answer as an absent key.
#[test]
fn a_scope_response_writes_back_the_object_the_daemon_sent() {
    let sent = json!({
        "session_id": "test-session-001",
        "kilns": ["test-kiln"],
        "workspace": null
    });

    let scope: SessionScopeResponse =
        serde_json::from_value(sent.clone()).expect("the scope reads the object");
    assert_eq!(
        serde_json::to_value(scope).expect("the scope writes JSON"),
        sent
    );
}
