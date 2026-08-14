//! Tests for the session routes and the endpoint (SSRF) policy.

use super::*;
use crate::test_support::{arb_ipv4_private, arb_ipv4_public, arb_ipv6_loopback, arb_url_scheme};

use proptest::prelude::*;
use tower::ServiceExt;

/// Drive the async validator from a sync test body (proptest included).
fn block_on<T>(fut: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime")
        .block_on(fut)
}

/// A resolver that must never be reached: literal hosts are judged without
/// DNS, and the scheme check happens before any lookup. If a test trips
/// this, the validator is resolving something it should have decided on
/// directly.
async fn no_dns(host: String, _port: u16) -> ResolvedAddrs {
    panic!("unexpected DNS lookup for {host}");
}

/// Literal-host check as a non-loopback bind sees it: loopback refused.
fn check(endpoint: &str) -> Result<(), WebError> {
    block_on(validate_endpoint_with(endpoint, false, no_dns))
}

/// Literal-host check as a loopback bind sees it.
fn check_allowing_loopback(endpoint: &str) -> Result<(), WebError> {
    block_on(validate_endpoint_with(endpoint, true, no_dns))
}

/// Hostname check against a fixed answer set, standing in for DNS.
fn check_resolving_to(endpoint: &str, answers: &[&str]) -> Result<(), WebError> {
    check_resolving_to_with(endpoint, answers, false)
}

fn check_resolving_to_with(
    endpoint: &str,
    answers: &[&str],
    allow_loopback: bool,
) -> Result<(), WebError> {
    let answers: Vec<IpAddr> = answers.iter().map(|a| a.parse().expect("addr")).collect();
    block_on(validate_endpoint_with(
        endpoint,
        allow_loopback,
        |_host, _port| async { Ok(answers) },
    ))
}

proptest! {
    #[test]
    fn validate_endpoint_rejects_private_ipv4_addresses(ip in arb_ipv4_private()) {
        let endpoint = format!("http://{ip}/");
        prop_assert!(check(&endpoint).is_err());
    }

    #[test]
    fn validate_endpoint_accepts_public_ipv4_with_http_or_https(
        ip in arb_ipv4_public(),
        scheme in prop_oneof![Just("http"), Just("https")],
    ) {
        let endpoint = format!("{scheme}://{ip}/");
        prop_assert!(check(&endpoint).is_ok(), "{endpoint}");
    }

    #[test]
    fn validate_endpoint_rejects_non_http_schemes(
        scheme in arb_url_scheme().prop_filter("non-http scheme", |s| s != "http" && s != "https"),
    ) {
        let endpoint = format!("{scheme}://example.com");
        prop_assert!(check(&endpoint).is_err());
    }

    #[test]
    fn validate_endpoint_rejects_ipv6_loopback_on_a_non_loopback_bind(host in arb_ipv6_loopback()) {
        let host = host.trim_matches(['[', ']']);
        let endpoint = format!("http://[{host}]/");
        prop_assert!(check(&endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn a_loopback_bind_allows_the_default_local_ollama_endpoint() {
    // The headline local-LLM path, and the canonical fixture in this repo.
    // `cru web` binds loopback by default, so this must work out of the box
    // with no env var: the only browser that can reach a loopback bind is
    // already on this machine.
    for endpoint in ["http://127.0.0.1:11434", "http://[::1]:11434"] {
        assert!(check_allowing_loopback(endpoint).is_ok(), "{endpoint}");
    }
    assert!(check_resolving_to_with("http://localhost:11434", &["127.0.0.1", "::1"], true).is_ok());
}

#[test]
fn endpoint_policy_allows_loopback_exactly_when_the_bind_is_loopback() {
    // §W4: "Loopback stays allowed only when the bind is loopback."
    for bind in [
        "127.0.0.1",
        "127.0.1.1",
        "localhost",
        "LocalHost",
        "::1",
        "[::1]",
    ] {
        assert!(
            EndpointPolicy::from_bind(bind, false).allow_loopback,
            "bind {bind} is loopback"
        );
    }
    for bind in [
        "0.0.0.0",
        "::",
        "[::]",
        "192.168.1.20",
        "crucible.example.com",
        UNKNOWN_BIND,
    ] {
        assert!(
            !EndpointPolicy::from_bind(bind, false).allow_loopback,
            "bind {bind} is reachable from elsewhere"
        );
    }
}

#[test]
fn the_env_override_only_adds_loopback_on_a_non_loopback_bind() {
    assert!(EndpointPolicy::from_bind("0.0.0.0", true).allow_loopback);
    // Redundant but harmless on a loopback bind.
    assert!(EndpointPolicy::from_bind("127.0.0.1", true).allow_loopback);
}

#[test]
fn a_non_loopback_bind_refuses_loopback_endpoints() {
    // There the browser is a confused deputy: the server's own loopback is
    // exactly what it cannot otherwise reach.
    for endpoint in ["http://127.0.0.1:11434", "http://[::1]:8080"] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
    for endpoint in ["http://localhost:8080", "https://localhost:3000"] {
        assert!(
            check_resolving_to(endpoint, &["127.0.0.1", "::1"]).is_err(),
            "{endpoint}"
        );
    }
}

#[test]
fn allowing_loopback_permits_loopback_only_not_other_internal_ranges() {
    for endpoint in [
        "http://169.254.169.254/latest/meta-data/",
        "http://10.0.0.1",
        "http://[fd00::1]",
        "http://[::ffff:169.254.169.254]",
    ] {
        assert!(
            check_allowing_loopback(endpoint).is_err(),
            "{endpoint} must stay blocked even when loopback is allowed"
        );
    }
}

#[test]
fn loopback_opt_in_requires_an_explicit_truthy_value() {
    assert!(loopback_opt_in(Some("1")));
    assert!(loopback_opt_in(Some("true")));
    assert!(loopback_opt_in(Some(" TRUE ")));
    assert!(!loopback_opt_in(None));
    assert!(!loopback_opt_in(Some("")));
    assert!(!loopback_opt_in(Some("0")));
    assert!(!loopback_opt_in(Some("false")));
    assert!(!loopback_opt_in(Some("yes")));
}

#[test]
fn validate_endpoint_rejects_10_0_0_1() {
    assert!(check("http://10.0.0.1").is_err());
}

#[test]
fn validate_endpoint_rejects_192_168_1_1() {
    assert!(check("http://192.168.1.1").is_err());
}

#[test]
fn validate_endpoint_rejects_172_16_0_1() {
    assert!(check("http://172.16.0.1").is_err());
}

#[test]
fn validate_endpoint_rejects_ftp_scheme() {
    assert!(check("ftp://example.com").is_err());
}

#[test]
fn validate_endpoint_rejects_malformed_url() {
    assert!(check("not-a-url").is_err());
}

#[test]
fn validate_endpoint_rejects_empty_host() {
    assert!(check("http://").is_err());
}

#[test]
fn validate_endpoint_allows_a_domain_that_resolves_to_public_addresses() {
    assert!(check_resolving_to("http://example.com", &["93.184.216.34"]).is_ok());
}

#[test]
fn validate_endpoint_rejects_a_hostname_that_resolves_to_a_private_address() {
    // The hole this closes: the host is a name, not a literal, so the old
    // check never looked at it.
    assert!(check_resolving_to("http://internal.corp", &["10.0.0.5"]).is_err());
}

#[test]
fn validate_endpoint_rejects_a_hostname_that_resolves_to_the_cloud_metadata_address() {
    assert!(check_resolving_to("http://metadata.example/latest/", &["169.254.169.254"]).is_err());
}

#[test]
fn validate_endpoint_rejects_a_hostname_when_any_resolved_address_is_internal() {
    // A mixed answer set is the cheap version of rebinding: one public
    // record to pass a check that only looks at the first answer.
    assert!(check_resolving_to(
        "http://mixed.example",
        &["93.184.216.34", "169.254.169.254"]
    )
    .is_err());
}

#[test]
fn validate_endpoint_refuses_a_hostname_it_cannot_resolve() {
    let unresolvable = block_on(validate_endpoint_with(
        "http://nx.example",
        false,
        |_host, _port| async {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no such host",
            ))
        },
    ));
    assert!(
        unresolvable.is_err(),
        "an unresolvable host must fail closed"
    );

    let empty = block_on(validate_endpoint_with(
        "http://nx.example",
        false,
        |_host, _port| async { Ok(Vec::new()) },
    ));
    assert!(empty.is_err(), "an empty answer set must fail closed");
}

#[test]
fn validate_endpoint_rejects_shorthand_literal_encodings_of_loopback() {
    for endpoint in [
        "http://2130706433/",
        "http://127.1",
        "http://0x7f.0.0.1",
        "http://0177.0.0.1",
    ] {
        // Asserting on the message, not just `is_err`: these must be
        // refused because the URL parser normalized them to 127.0.0.1 and
        // the address check saw it — not because the URL failed to parse.
        let err = check(endpoint).expect_err(endpoint).to_string();
        assert!(err.contains("127.0.0.1"), "{endpoint}: {err}");
    }
}

#[test]
fn validate_endpoint_rejects_ipv4_mapped_ipv6_encodings_of_internal_addresses() {
    for endpoint in [
        "http://[::ffff:169.254.169.254]",
        "http://[::ffff:a9fe:a9fe]",
        "http://[::ffff:127.0.0.1]",
        "http://[::ffff:10.0.0.1]",
        "http://[::127.0.0.1]",
    ] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn validate_endpoint_rejects_ipv4_translated_ipv6_encoding_of_the_metadata_address() {
    assert!(check("http://[::ffff:0:169.254.169.254]").is_err());
}

#[test]
fn validate_endpoint_rejects_ipv6_outside_global_unicast() {
    for endpoint in [
        "http://[100::1]",          // discard-only prefix
        "http://[ff02::1]",         // link-local multicast
        "http://[64:ff9b:1::a9fe]", // RFC 6052 local-use NAT64 prefix
        "http://[2001:db8::1]",     // documentation
        "http://[0100:0000::1]",    // anything else non-global
    ] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn validate_endpoint_allows_global_unicast_ipv6() {
    assert!(check("http://[2606:4700::1111]").is_ok());
}

#[test]
fn validate_endpoint_rejects_unique_local_and_link_local_ipv6() {
    for endpoint in [
        "http://[fc00::1]",
        "http://[fd12:3456::1]",
        "http://[fe80::1]",
        "http://[fec0::1]",
    ] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn validate_endpoint_rejects_reserved_and_shared_ipv4_ranges() {
    for endpoint in [
        "http://0.1.2.3",
        "http://100.64.0.1",
        "http://192.0.0.1",
        "http://198.18.0.1",
        "http://224.0.0.1",
        "http://240.0.0.1",
    ] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn validate_endpoint_rejects_an_internal_host_hidden_in_userinfo() {
    assert!(check("http://example.com@169.254.169.254/").is_err());
}

#[test]
fn validate_endpoint_rejects_6to4_and_nat64_encodings_of_the_metadata_address() {
    for endpoint in [
        "http://[2002:a9fe:a9fe::]",
        "http://[64:ff9b::169.254.169.254]",
    ] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
}

// =========================================================================
// create_session Tests
// =========================================================================

async fn post_create_session(
    body: serde_json::Value,
) -> (axum::http::StatusCode, serde_json::Value) {
    crate::test_support::request_json("POST", "/api/session", Some(body)).await
}

#[tokio::test]
async fn create_session_works_without_a_kiln() {
    let (status, json) = post_create_session(serde_json::json!({})).await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    assert_eq!(json["session_id"], "test-session-001");
}

#[tokio::test]
async fn create_session_accepts_connect_kilns() {
    let (status, json) = post_create_session(serde_json::json!({
        "connect_kilns": ["/tmp/extra-kiln"],
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
async fn create_session_refuses_an_endpoint_targeting_an_internal_address() {
    // The route must not forward such an endpoint to the daemon at all.
    // Only addresses that stay blocked under either loopback setting are
    // listed here, so the assertion does not depend on the ambient env.
    for endpoint in [
        "http://169.254.169.254/latest/meta-data/",
        "http://[::ffff:169.254.169.254]/",
        "http://10.0.0.1:11434",
    ] {
        let (status, _) = post_create_session(serde_json::json!({
            "endpoint": endpoint,
        }))
        .await;
        assert_eq!(
            status,
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "endpoint {endpoint} should be refused"
        );
    }
}

/// POST /api/session through a router built for a specific bind, so the
/// whole seam is exercised: bind host → `EndpointPolicy` → the layer on
/// `session_routes_with` → the `Extension` `create_session` reads.
async fn post_create_session_bound_to(
    bind_host: &str,
    body: serde_json::Value,
) -> axum::http::StatusCode {
    let (_mock, client) = crate::test_support::start_mock_daemon().await;
    let state = crate::test_support::build_mock_state(client);
    let app = session_routes_with(EndpointPolicy::from_bind(bind_host, false)).with_state(state);

    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/api/session")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();

    app.oneshot(request).await.unwrap().status()
}

#[tokio::test]
async fn create_session_accepts_the_local_ollama_endpoint_on_a_loopback_bind() {
    // `cru web` with no flags: a session pointed at the local Ollama must
    // be created, not 422'd.
    for endpoint in ["http://127.0.0.1:11434", "http://[::1]:11434"] {
        let status =
            post_create_session_bound_to("127.0.0.1", serde_json::json!({ "endpoint": endpoint }))
                .await;
        assert_eq!(status, axum::http::StatusCode::OK, "endpoint {endpoint}");
    }
}

#[tokio::test]
async fn create_session_refuses_a_loopback_endpoint_on_a_wildcard_bind() {
    // `cru web --host 0.0.0.0`: a LAN browser must not aim the server at
    // the server's own loopback.
    let status = post_create_session_bound_to(
        "0.0.0.0",
        serde_json::json!({ "endpoint": "http://127.0.0.1:11434" }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_session_refuses_the_metadata_address_on_every_bind() {
    for bind in ["127.0.0.1", "0.0.0.0"] {
        let status = post_create_session_bound_to(
            bind,
            serde_json::json!({ "endpoint": "http://169.254.169.254/latest/meta-data/" }),
        )
        .await;
        assert_eq!(
            status,
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "bind {bind}"
        );
    }
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
        serde_json::json!({"kiln": "/tmp/extra-kiln"}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    assert_eq!(json["connected_kilns"][0], "/tmp/extra-kiln");
}

#[tokio::test]
async fn disconnect_kiln_returns_updated_scope() {
    let (status, json) = send_json(
        "POST",
        "/api/session/test-session-001/kilns/disconnect",
        serde_json::json!({"kiln": "/tmp/extra-kiln"}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    assert!(json["connected_kilns"].as_array().unwrap().is_empty());
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

    // Only kiln is required — provider and model should resolve from detected defaults
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({"kiln": "/tmp/test-kiln"}).to_string(),
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
                        "kiln": "/tmp/test-kiln",
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

    assert_eq!(json["current_mode_id"], "normal");
    let ids: Vec<&str> = json["modes"]
        .as_array()
        .expect("modes must be an array")
        .iter()
        .map(|m| m["id"].as_str().expect("mode id"))
        .collect();
    assert_eq!(ids, vec!["normal", "plan"]);
}
