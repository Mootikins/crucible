//! Tests for the session routes.

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
    use crate::test_support::{build_mock_state, build_test_app, start_mock_daemon};
    let (mock, client) = start_mock_daemon().await;
    let response = build_test_app(build_mock_state(client))
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
    let request = mock.received_params("session.create").unwrap();
    assert_eq!(request["agent_card"], "researcher");
    assert_eq!(request["workspace"], "/work/project");
    assert_eq!(request["configure_agent"], true);
    assert!(request.get("agent_name").is_none());
    assert!(!mock
        .received_methods()
        .iter()
        .any(|method| method == "session.configure_agent"));
}

/// The daemon owns the endpoint check, so the route forwards the endpoint
/// as the browser sent it. A daemon refusal (`-32602`) becomes a 422 through
/// `daemon_err`, as for every other caller-fixable create error.
#[tokio::test]
async fn create_session_forwards_the_endpoint_to_the_daemon_unchecked() {
    use crate::test_support::{build_mock_state, build_test_app, start_mock_daemon};
    let (mock, client) = start_mock_daemon().await;
    let response = build_test_app(build_mock_state(client))
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
    let request = mock.received_params("session.create").unwrap();
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
    let state = crate::test_support::build_mock_state(client);
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
        methods.iter().any(|m| m == "session.create"),
        "web must forward the create (with the agent spec) to the daemon: {methods:?}"
    );
    assert!(
        !methods.iter().any(|m| m == "agents.resolve_profile"),
        "profile resolution moved daemon-side; web must NOT resolve it: {methods:?}"
    );
    assert!(
        !methods.iter().any(|m| m == "session.subscribe"),
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
// Session scope (kilns/workspace) Tests
// =========================================================================

async fn send_json(
    method: &str,
    uri: &str,
    body: serde_json::Value,
) -> (axum::http::StatusCode, serde_json::Value) {
    crate::test_support::request_json(method, uri, Some(body)).await
}

#[tokio::test]
async fn connect_kiln_returns_updated_scope() {
    let (status, json) = send_json(
        "POST",
        "/api/session/test-session-001/kilns/connect",
        serde_json::json!({"kiln": "extra-kiln"}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    assert_eq!(json["kilns"][1], "extra-kiln");
}

#[tokio::test]
async fn disconnect_kiln_returns_updated_scope() {
    let (status, json) = send_json(
        "POST",
        "/api/session/test-session-001/kilns/disconnect",
        serde_json::json!({"kiln": "extra-kiln"}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    assert_eq!(json["kilns"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn set_workspace_accepts_null_for_detach() {
    let (status, json) = send_json(
        "PUT",
        "/api/session/test-session-001/workspace",
        serde_json::json!({ "workspace": null }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    // Detach falls back to the kiln path (mock echoes the default).
    assert_eq!(json["workspace"], "/tmp/test-kiln");
}

#[tokio::test]
async fn set_workspace_attaches_project_dir() {
    let (status, json) = send_json(
        "PUT",
        "/api/session/test-session-001/workspace",
        serde_json::json!({ "workspace": "/repos/crucible" }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    assert_eq!(json["workspace"], "/repos/crucible");
}

// =========================================================================
// export_session Tests
// =========================================================================

#[tokio::test]
async fn export_session_returns_text_markdown_content_type() {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_mock_state(client);
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
    let state = crate::test_support::build_mock_state(client);
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
    let state = crate::test_support::build_mock_state(client);
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
    let state = crate::test_support::build_mock_state(client);
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
// auto_title Tests
// =========================================================================

#[tokio::test]
async fn auto_title_returns_200_with_title_field() {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_mock_state(client);
    let app = crate::test_support::build_test_app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/auto-title")
                .body(axum::body::Body::empty())
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
        json.get("title").is_some(),
        "Response should contain 'title' field"
    );
    assert!(json["title"].is_string(), "Title should be a string");
}

#[tokio::test]
async fn auto_title_delegates_to_daemon_generate_title() {
    // Title generation is daemon-owned (topic-based LLM with truncation
    // fallback); the web route only forwards and unwraps the result.
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_mock_state(client);
    let app = crate::test_support::build_test_app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/auto-title")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        json["title"].as_str().unwrap(),
        "Merkle tree sync design",
        "Title should come from the daemon's session.generate_title"
    );
}

// =========================================================================
// Session creation smart defaults & provider filtering
// =========================================================================

#[tokio::test]
async fn test_create_session_without_provider_uses_detected_default() {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_mock_state(client);
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
    let state = crate::test_support::build_mock_state(client);
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

#[tokio::test]
async fn test_list_providers_with_kiln_query_param_returns_200() {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_mock_state(client);
    let app = crate::test_support::build_test_app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/providers?kiln=/tmp/test-kiln")
                .body(axum::body::Body::empty())
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
        json["providers"].is_array(),
        "Response must have 'providers' array when kiln query param is provided"
    );
}

/// The modes route forwards the daemon's list verbatim. Asserting the
/// field names here is the point: the browser reads `current_mode_id` and
/// `modes[].id`, and a rename on either side would otherwise surface as a
/// mode chip that silently falls back to its placeholder.
#[tokio::test]
async fn list_modes_returns_the_daemon_s_modes_and_current_mode() {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_mock_state(client);
    let app = crate::test_support::build_test_app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/session/test-session-001/modes")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["current_mode_id"], "ask");
    let ids: Vec<&str> = json["modes"]
        .as_array()
        .expect("modes must be an array")
        .iter()
        .map(|m| m["id"].as_str().expect("mode id"))
        .collect();
    assert_eq!(ids, vec!["ask", "plan", "propose"]);
    // The route passes `writes` through; a descriptor without it reads `apply`.
    assert_eq!(json["modes"][0]["writes"], "apply");
    assert_eq!(json["modes"][2]["writes"], "propose");
}

/// Reading a session's history must not bring the session back to life.
///
/// The history route called `session.resume_from_storage`, which sets an
/// ended session to `Active`, saves it, and runs the start checks, which can
/// pull a container. A page that only showed the transcript revived the
/// session every time it loaded.
#[tokio::test]
async fn reading_the_history_does_not_resume_the_session() {
    use crate::test_support::{build_mock_state, build_test_app, start_mock_daemon};
    let (mock, client) = start_mock_daemon().await;
    let response = build_test_app(build_mock_state(client))
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/session/test-session-001/history?limit=5&offset=2")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let methods = mock.received_methods();
    assert!(
        !methods.iter().any(|m| m.starts_with("session.resume")),
        "a history read must not resume the session: {methods:?}"
    );
    let params = mock
        .received_params("session.history")
        .unwrap_or_else(|| panic!("the history read must use session.history: {methods:?}"));
    assert_eq!(params["session_id"], "test-session-001");
    assert_eq!(params["limit"], 5);
    assert_eq!(params["offset"], 2);
}
