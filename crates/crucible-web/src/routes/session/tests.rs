//! Tests for the session routes.

use crucible_core::protocol::rpc::RpcMethod;

use tower::ServiceExt;

// =========================================================================
// create_session Tests
// =========================================================================

async fn post_create_session(
    body: serde_json::Value,
) -> (axum::http::StatusCode, serde_json::Value) {
    crate::test_support::request_json("POST", "/api/session", Some(body)).await
}

#[tokio::test]
async fn create_session_works_without_any_kilns() {
    let (status, json) = post_create_session(serde_json::json!({})).await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    assert_eq!(json["session_id"], "test-session-001");
}

#[tokio::test]
async fn create_session_accepts_a_multi_kiln_set() {
    let (status, json) = post_create_session(serde_json::json!({
        "kilns": ["test-kiln", "extra-kiln"],
    }))
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
}

#[tokio::test]
async fn create_session_accepts_acp_agent() {
    let (status, json) = post_create_session(serde_json::json!({
        "agent_type": "acp",
        "agent_name": "claude",
    }))
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    assert_eq!(json["session_id"], "test-session-001");
}

#[tokio::test]
async fn create_session_forwards_a_card_without_reconfiguring_its_agent() {
    use crate::test_support::{build_state, build_test_app, start_mock_daemon};
    let (mock, client) = start_mock_daemon().await;
    let response = build_test_app(build_state(client))
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"agent_card":"researcher","workspace":"/work/project"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let request = mock.received_params(RpcMethod::SessionCreate).unwrap();
    assert_eq!(request["agent_card"], "researcher");
    assert_eq!(request["workspace"], "/work/project");
    assert_eq!(request["configure_agent"], true);
    assert!(request.get("agent_name").is_none());
    assert!(!mock
        .received_methods()
        .contains(&RpcMethod::SessionConfigureAgent));
}

/// The daemon owns the endpoint check, so the route forwards the endpoint
/// as the browser sent it. A daemon refusal (`-32602`) becomes a 422 through
/// `daemon_err`, as for every other caller-fixable create error.
#[tokio::test]
async fn create_session_forwards_the_endpoint_to_the_daemon_unchecked() {
    use crate::test_support::{build_state, build_test_app, start_mock_daemon};
    let (mock, client) = start_mock_daemon().await;
    let response = build_test_app(build_state(client))
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"endpoint":"http://169.254.169.254/latest/meta-data/"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let request = mock.received_params(RpcMethod::SessionCreate).unwrap();
    assert_eq!(
        request["endpoint"],
        "http://169.254.169.254/latest/meta-data/"
    );
}

#[tokio::test]
async fn create_session_rejects_acp_without_agent_name() {
    let (status, _) = post_create_session(serde_json::json!({
        "agent_type": "acp",
    }))
    .await;
    assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_session_rejects_unknown_acp_agent() {
    // The mock daemon resolves any profile name except "missing" to null.
    let (status, _) = post_create_session(serde_json::json!({
        "agent_type": "acp",
        "agent_name": "missing",
    }))
    .await;
    assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
}

/// `agent_name` with no `agent_type` is how this crate selects an agent *card*
/// — the deprecated alias the daemon keeps for exactly this caller. It resolves
/// on the internal branch, so an unresolvable name is a card error, and it must
/// reach the client as 422 rather than a 502 daemon fault.
#[tokio::test]
async fn create_session_rejects_an_unknown_agent_card() {
    let (status, body) = post_create_session(serde_json::json!({
        "agent_name": "missing",
    }))
    .await;
    assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body.to_string().contains("Unknown agent card"),
        "the daemon's card diagnostic must survive to the client: {body}"
    );
}

#[tokio::test]
async fn create_session_with_unknown_acp_agent_does_not_create_a_session() {
    // Regression: an unknown ACP agent must not orphan an agent-less
    // session. Resolution now lives in the daemon's session.create, which
    // rejects the unknown profile atomically (INVALID_PARAMS, no row). At
    // the web/wire level the invariants are: the web forwards a single
    // session.create carrying the agent spec, no longer resolves the
    // profile client-side (agents.resolve_profile), and does NOT proceed to
    // subscribe once create fails.
    let (mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_state(client);
    let app = crate::test_support::build_test_app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({
                        "agent_type": "acp",
                        "agent_name": "missing",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );

    let methods = mock.received_methods();
    assert!(
        methods.contains(&RpcMethod::SessionCreate),
        "web must forward the create (with the agent spec) to the daemon: {methods:?}"
    );
    assert!(
        !methods.contains(&RpcMethod::AgentsResolveProfile),
        "profile resolution moved daemon-side; web must NOT resolve it: {methods:?}"
    );
    assert!(
        !methods.contains(&RpcMethod::SessionSubscribe),
        "a failed create must not proceed to subscribe: {methods:?}"
    );
}

#[tokio::test]
async fn create_session_rejects_unknown_agent_type() {
    // Anything other than absent/"internal"/"acp" is a validation error,
    // not a silently-forwarded junk string on the internal branch.
    for bad in ["ACP", "internal-x", "external", ""] {
        let (status, _) = post_create_session(serde_json::json!({
            "agent_type": bad,
        }))
        .await;
        assert_eq!(
            status,
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "agent_type {bad:?} should be rejected"
        );
    }
}

#[tokio::test]
async fn create_session_errors_when_daemon_returns_no_session_id() {
    // Protocol drift: a create response missing session_id must fail loudly,
    // not proceed to configure_agent/subscribe against an empty id. The mock
    // drops session_id for the "__no_session_id__" sentinel session_type.
    let (status, _) = post_create_session(serde_json::json!({
        "session_type": "__no_session_id__",
    }))
    .await;
    assert_eq!(status, axum::http::StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn create_session_accepts_internal_agent_type() {
    let (status, json) = post_create_session(serde_json::json!({
        "agent_type": "internal",
    }))
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    assert_eq!(json["session_id"], "test-session-001");
}

// =========================================================================
// resume_session Tests
// =========================================================================

/// The two resume shapes read back as the variant that wrote them.
///
/// This is the gate on the variant order in `ResumeSessionResponse`. The live
/// shape's required fields are a subset of the restored one's, so an untagged
/// union that lists `Live` first answers `Live` for a restored history and
/// drops every event, with no error anywhere. Both directions are asserted,
/// because only one of them fails when the order is wrong.
#[tokio::test]
async fn resume_session_answers_the_warm_path_shape() {
    let (status, json) =
        crate::test_support::request_json("POST", "/api/session/test-session-001/resume", None)
            .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    // The warm path, because the mock's `session.resume` succeeds.
    assert!(json.get("history").is_none(), "warm resume: {json}");
    assert_eq!(json["state"], "active");
}

/// The cold path answers the restored history, and it reads back as
/// `Restored`.
#[tokio::test]
async fn a_cold_resume_answers_the_restored_history() {
    // "cold-resume-session" is the mock's own stand-in for a session
    // `session.resume` had to reload from storage: the daemon's reply names
    // `resumed_from_storage: true`, which is the ONLY thing that decides this
    // route also reads `session.history` — not a second daemon call failing.
    let (status, json) =
        crate::test_support::request_json("POST", "/api/session/cold-resume-session/resume", None)
            .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    assert_eq!(json["history"][0]["event"], "user_message");
}

// =========================================================================
// export_session Tests
// =========================================================================

#[tokio::test]
async fn export_session_returns_text_markdown_content_type() {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_state(client);
    let app = crate::test_support::build_test_app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/export")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/markdown"),
        "Expected text/markdown content-type, got: {}",
        content_type
    );
}

#[tokio::test]
async fn export_session_returns_markdown_body() {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_state(client);
    let app = crate::test_support::build_test_app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/export")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();

    // Should contain markdown content (either from render or fallback)
    assert!(!text.is_empty(), "Exported markdown should not be empty");
    // Fallback markdown includes session title
    assert!(
        text.contains("#") || text.contains("Test Session"),
        "Exported markdown should contain heading or session title"
    );
}

#[tokio::test]
async fn export_session_fallback_includes_session_metadata() {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_state(client);
    let app = crate::test_support::build_test_app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/export")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();

    // Fallback markdown should include metadata fields
    // The mock returns render_markdown with "# Test Session\n\nExported content"
    // But if render fails, fallback includes: title, started_at, model, state
    assert!(
        text.contains("Test Session") || text.contains("Date"),
        "Exported markdown should include session metadata"
    );
}

#[tokio::test]
async fn export_session_with_valid_session_returns_200() {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_state(client);
    let app = crate::test_support::build_test_app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/export")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Valid session with kiln should return 200
    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

// =========================================================================
// Session creation smart defaults & provider filtering
// =========================================================================

#[tokio::test]
async fn test_create_session_without_provider_uses_detected_default() {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_state(client);
    let app = crate::test_support::build_test_app(state);

    // Only kilns is required — provider and model should resolve from detected defaults
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({"kilns": ["test-kiln"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json.get("session_id").is_some(),
        "Response must contain session_id even without explicit provider/model"
    );
}

#[tokio::test]
async fn test_create_session_with_explicit_provider_still_works() {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_state(client);
    let app = crate::test_support::build_test_app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({
                        "kilns": ["test-kiln"],
                        "provider": "ollama",
                        "model": "llama3.2"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json.get("session_id").is_some(),
        "Response must contain session_id with explicit provider/model"
    );
    assert_eq!(json["session_id"], "test-session-001");
}
