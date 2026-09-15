use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    Router,
};
use crucible_web::{
    middleware::auth::{ApiKeyState, HostPolicy},
    server::{build_router, WebConfig},
    test_support::{build_mock_state, start_mock_daemon},
};
use std::{net::SocketAddr, sync::Arc};
use tower::ServiceExt;

fn request(method: &str, path: &str, local: bool) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost:3000")
        .extension(ConnectInfo(
            if local {
                "127.0.0.1:1234"
            } else {
                "192.0.2.1:1234"
            }
            .parse::<SocketAddr>()
            .unwrap(),
        ))
}

fn app(
    state: crucible_web::services::daemon::AppState,
    remote_shell: bool,
    key: Option<&str>,
) -> Router {
    let config = WebConfig {
        host: "0.0.0.0".into(),
        remote_shell,
        ..Default::default()
    };
    let credentials = Arc::new(ApiKeyState::new_at(
        key.map(str::to_owned),
        HostPolicy::from_web_config(&config).unwrap(),
        None,
    ));
    build_router(&config, state, credentials)
}

#[tokio::test]
async fn assembled_routes_require_credentials_before_dispatch_but_keep_bootstrap_public() {
    let (mock, client) = start_mock_daemon().await;
    let router = app(build_mock_state(client), false, Some("secret"));
    for path in [
        "/api/agents",
        "/api/chat/send",
        "/api/config",
        "/api/session/list",
        "/api/project/list",
        "/api/scm/clone",
        "/api/fs/move",
        "/api/search/vectors",
        "/api/plugins",
        "/api/surfaces",
        "/api/mcp/status",
        "/api/kiln/notes",
        "/api/canvas",
        "/api/layout",
        "/api/skills",
        "/api/webhook/probe",
        "/api/shell/exec",
        "/api/terminal/ws",
    ] {
        for token in [None, Some("wrong")] {
            let mut req = request("GET", path, false);
            if let Some(token) = token {
                req = req.header("authorization", format!("Bearer {token}"));
            }
            let response = router
                .clone()
                .oneshot(req.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        }
    }
    assert!(
        mock.received_methods().is_empty(),
        "refused requests must not reach the daemon"
    );
    let health = router
        .clone()
        .oneshot(
            request("GET", "/health", false)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    assert!(health.headers().contains_key("content-security-policy"));
    let login = router
        .clone()
        .oneshot(
            request("POST", "/api/auth/login", false)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"key":"secret"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::NO_CONTENT);
    let cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    for header in [("authorization", "Bearer secret"), ("cookie", cookie)] {
        let response = router
            .clone()
            .oneshot(
                request("GET", "/api/config", false)
                    .header(header.0, header.1)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    for path in ["/health", "/", "/api/config", "/api/auth/login"] {
        let mut req = request("GET", path, true).body(Body::empty()).unwrap();
        req.headers_mut()
            .insert("host", "evil.test".parse().unwrap());
        assert_eq!(
            router.clone().oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
}

#[tokio::test]
async fn assembled_terminal_requires_the_remote_opt_in_credentials_and_a_safe_origin() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    for (remote, key, local, token, origin, expected) in [
        (
            false,
            Some("secret"),
            false,
            Some("secret"),
            "http://localhost:3000",
            StatusCode::FORBIDDEN,
        ),
        (
            true,
            None,
            false,
            None,
            "http://localhost:3000",
            StatusCode::FORBIDDEN,
        ),
        (
            true,
            Some("secret"),
            true,
            None,
            "http://localhost:3000",
            StatusCode::UNAUTHORIZED,
        ),
        (
            true,
            Some("secret"),
            false,
            Some("secret"),
            "http://evil.test",
            StatusCode::FORBIDDEN,
        ),
        // Once admitted, the upgrade extractor rejects this non-upgrade request.
        (
            true,
            Some("secret"),
            false,
            Some("secret"),
            "http://localhost:3000",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let mut req = request("GET", "/api/terminal/ws", local).header("origin", origin);
        if let Some(token) = token {
            req = req.header("authorization", format!("Bearer {token}"));
        }
        let response = app(state.clone(), remote, key)
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            expected,
            "remote={remote} local={local} origin={origin}"
        );
    }
}
