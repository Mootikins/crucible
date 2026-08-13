//! WebSocket origin validation and the localhost-only gate on shell routes.

use super::caller_is_loopback;
use super::host::{normalize_authority, request_authority, HostVerified};
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

/// Rejects WebSocket upgrades whose `Origin` isn't in the allow-list.
///
/// Browsers do not apply CORS/same-origin policy to WebSocket handshakes, so
/// `CorsLayer` never fires for them — a malicious page could otherwise open
/// `ws://localhost:PORT/api/terminal/ws` and, because the request originates
/// from loopback, sail past the localhost auth bypass into a real shell
/// (Cross-Site WebSocket Hijacking → RCE). Browsers *do* always send `Origin`
/// on WS handshakes, so validating it against the same list used for CORS
/// blocks cross-site pages while allowing the app's own frontend.
///
/// A missing `Origin` is therefore passed through: it cannot be a browser, and
/// this guard's only job is CSWSH. It is *not* a claim that such a client is
/// local. What stands between it and the PTY is `bearer_auth` — plus, while
/// `[web] remote_shell` is off, [`localhost_only_shell_auth`]. With the opt-in
/// on, that gate returns on its first line and `bearer_auth` is the whole of
/// it, which is why the opt-in is fail-closed on an API key being configured
/// ([`remote_shell_active`]).
pub async fn websocket_origin_guard(
    State(allowed): State<Arc<Vec<HeaderValue>>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        // Same-origin is always fine — CSWSH means a CROSS-site page driving
        // the socket. The static allowlist can't know every host the server is
        // reachable as (LAN IP, hostname), but the request's own authority
        // can — *only* once `enforce_host` has confirmed that authority is
        // one of ours. Left unverified, Origin == Host is trivially satisfied
        // by a rebound attacker domain, since both come from the same request.
        let same_origin = request.extensions().get::<HostVerified>().is_some()
            && origin_matches_authority(origin, request_authority(&request).as_deref());
        if !same_origin && !allowed.iter().any(|a| a == origin) {
            // Name the authority and the remedy. This refusal is invisible
            // where it matters: a browser never exposes a failed WS
            // handshake's status or body to page script, so the panel sees
            // only "the socket closed" and shows a reconnect button that can
            // never succeed. The log is the operator's only channel, so it
            // has to carry the fix, not just the verdict.
            tracing::warn!(
                ?origin,
                authority = ?request_authority(&request),
                host_verified = request.extensions().get::<HostVerified>().is_some(),
                "Rejecting WebSocket upgrade: Origin is not an authority this server answers to. \
                 If this is a name you reach the server by, add it to `[web] allowed_hosts` \
                 in config.toml and restart"
            );
            return (
                StatusCode::FORBIDDEN,
                "Origin not allowed — add this host to `[web] allowed_hosts` in config.toml",
            )
                .into_response();
        }
    }
    next.run(request).await
}

/// `Origin: http(s)://<authority>` names the same authority the request was
/// addressed to. The Origin side goes through `normalize_authority` — the
/// other side already has — so case or trailing-dot spellings cannot make two
/// different names compare equal.
fn origin_matches_authority(origin: &HeaderValue, authority: Option<&str>) -> bool {
    let (Ok(origin), Some(authority)) = (origin.to_str(), authority) else {
        return false;
    };
    origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .and_then(normalize_authority)
        .is_some_and(|origin_authority| origin_authority == authority)
}

/// Whether non-loopback shell/terminal access is active. Fail-closed: the
/// `[web] remote_shell = true` config opt-in (or `cru web --remote-shell`)
/// only takes effect when an API key is configured — otherwise it would hand
/// any LAN peer an UNAUTHENTICATED PTY. With a key, remote requests still
/// pass bearer_auth (Bearer header or session cookie) like every other API
/// route.
pub fn remote_shell_active(opted_in: bool, api_key_configured: bool) -> bool {
    opted_in && api_key_configured
}

/// State for [`localhost_only_shell_auth`]: whether the loopback restriction
/// is lifted (already fail-closed via [`remote_shell_active`]), plus what a
/// credential must match once it is.
#[derive(Clone)]
pub struct ShellGateState {
    pub allow_remote: bool,
    /// The configured API key, and the session store that mints tokens for it.
    /// `None` when no key is configured, which also means `allow_remote` is
    /// false (see [`remote_shell_active`]).
    pub credentials: Option<super::ApiKeyState>,
}

/// Loopback-only gate on the shell and terminal routes, lifted only by the
/// fail-closed `[web] remote_shell` opt-in.
///
/// "Is the caller loopback?" is answered by [`caller_is_loopback`], the same
/// function `bearer_auth` uses for its own bypass — this middleware used to
/// answer it separately and the two disagreed about the same header.
///
/// **Loopback stops being a free pass once `remote_shell` is on.** `bearer_auth`
/// waves through any loopback caller, which is defensible for reading notes and
/// indefensible for a PTY, because with the opt-in active the Host allow-list
/// becomes the only thing standing between a rebound name and a shell: a
/// browser fetching an allow-listed authority that resolves to 127.0.0.1
/// arrives as a loopback caller, `enforce_host` marks it `HostVerified`, and the
/// WebSocket Origin guard's `Origin == Host` shortcut then passes honestly.
/// Verified before this change by asking for a PTY with no cookie and no key:
/// `Host: <allow-listed>` returned 101.
///
/// Requiring a credential here costs one sign-in on the operator's own machine
/// and removes the whole chain. A localhost-only server — `remote_shell` off,
/// which is the default — is untouched, because there the loopback restriction
/// is itself the defence and nothing outside the machine can reach these routes.
pub async fn localhost_only_shell_auth(
    State(state): State<Arc<ShellGateState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !state.allow_remote {
        // Opt-in off: loopback-only, exactly as before.
        return if caller_is_loopback(&request) {
            next.run(request).await
        } else {
            forbidden_response()
        };
    }
    if shell_caller_is_authenticated(&state, &request) {
        next.run(request).await
    } else {
        unauthenticated_shell_response()
    }
}

/// A Bearer header or a session cookie matching the configured key.
///
/// Deliberately re-checked here rather than delegating to `bearer_auth`: that
/// layer's loopback bypass is the thing being closed, so consulting it would
/// answer the wrong question.
fn shell_caller_is_authenticated(state: &ShellGateState, request: &Request<Body>) -> bool {
    let Some(creds) = &state.credentials else {
        // `remote_shell_active` cannot be true without a key, so this is
        // unreachable in practice — refuse rather than assume.
        return false;
    };
    let Some(expected) = creds.api_key.as_deref() else {
        return false;
    };
    let headers = request.headers();
    let bearer_ok = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .is_some_and(|token| super::verify_api_key(creds, token));
    bearer_ok
        || super::auth_cookie_values(headers).any(|token| creds.sessions.verify(token, expected))
}

fn unauthenticated_shell_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": {
                "code": StatusCode::UNAUTHORIZED.as_u16(),
                "message": "Shell access requires the API key, including from localhost, \
                            because `[web] remote_shell` is enabled",
            }
        })),
    )
        .into_response()
}

fn forbidden_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": {
                "code": StatusCode::FORBIDDEN.as_u16(),
                "message": "Shell routes are restricted to localhost",
            }
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::ConnectInfo;
    use axum::routing::get;
    use axum::{middleware, Router};
    use std::net::SocketAddr;
    use tower::ServiceExt;

    // --- WebSocket Origin guard tests (CSWSH defense) ---

    fn test_router_with_origin_guard() -> Router {
        let allowed = Arc::new(vec![HeaderValue::from_static("http://127.0.0.1:8080")]);
        Router::new()
            .route("/api/terminal/ws", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(
                allowed,
                websocket_origin_guard,
            ))
    }

    #[tokio::test]
    async fn origin_guard_allows_matching_origin() {
        let req = Request::builder()
            .uri("/api/terminal/ws")
            .header(header::ORIGIN, "http://127.0.0.1:8080")
            .body(Body::empty())
            .unwrap();
        let response = test_router_with_origin_guard().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn origin_guard_rejects_cross_site_origin() {
        // The CSWSH attack: a malicious page's Origin must be rejected even
        // though the request itself comes from loopback.
        let req = Request::builder()
            .uri("/api/terminal/ws")
            .header(header::ORIGIN, "https://evil.example")
            .body(Body::empty())
            .unwrap();
        let response = test_router_with_origin_guard().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn origin_guard_allows_missing_origin_non_browser() {
        // Non-browser clients send no Origin, and CSWSH is a browser attack —
        // so this guard has nothing to say about them. Whether they reach the
        // PTY is settled by the layers below, not here: see
        // `a_remote_originless_upgrade_still_needs_credentials_when_remote_shell_is_on`.
        let req = Request::builder()
            .uri("/api/terminal/ws")
            .body(Body::empty())
            .unwrap();
        let response = test_router_with_origin_guard().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_remote_originless_upgrade_still_needs_credentials_when_remote_shell_is_on() {
        // The invariant the guard's doc used to get wrong: with `[web]
        // remote_shell = true` there is no localhost gate left — it returns on
        // its first line — so an Origin-less remote upgrade meets nothing but
        // `bearer_auth`. This drives the terminal stack exactly as `server.rs`
        // layers it, from a LAN address, with no credentials at all.
        let key_state = Arc::new(crate::middleware::auth::ApiKeyState::new_at(
            Some("secret-key".to_string()),
            crate::middleware::auth::HostPolicy::from_bind("127.0.0.1", 3000, &[])
                .expect("bind-derived policy"),
            None,
        ));
        let allowed = Arc::new(vec![HeaderValue::from_static("http://127.0.0.1:3000")]);
        let terminal = Router::new()
            .route("/api/terminal/ws", get(|| async { "pty" }))
            .layer(middleware::from_fn_with_state(
                Arc::new(ShellGateState {
                    allow_remote: true,
                    credentials: Some((*key_state).clone()),
                }),
                localhost_only_shell_auth,
            ))
            .layer(middleware::from_fn_with_state(
                allowed,
                websocket_origin_guard,
            ))
            .layer(middleware::from_fn_with_state(
                key_state,
                crate::middleware::auth::bearer_auth,
            ));

        let mut req = Request::builder()
            .uri("/api/terminal/ws")
            .header(header::HOST, "127.0.0.1:3000")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 8], 5000))));

        let response = terminal.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    // --- Same-origin WebSocket acceptance ---

    #[tokio::test]
    async fn origin_guard_allows_same_origin_lan_host() {
        // A LAN browser at http://192.168.0.16:3001 isn't in the static
        // allowlist, but its Origin matches a Host that bearer_auth already
        // validated against the server's own authorities (the marker stands in
        // for that layer, which runs outside this one in the real router).
        let mut req = Request::builder()
            .uri("/api/terminal/ws")
            .header(header::ORIGIN, "http://192.168.0.16:3001")
            .header(header::HOST, "192.168.0.16:3001")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(HostVerified);
        let response = test_router_with_origin_guard().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn origin_guard_still_rejects_cross_origin_with_mismatched_host() {
        let mut req = Request::builder()
            .uri("/api/terminal/ws")
            .header(header::ORIGIN, "http://evil.example")
            .header(header::HOST, "192.168.0.16:3001")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(HostVerified);
        let response = test_router_with_origin_guard().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn origin_guard_refuses_an_origin_matching_only_an_unverified_host() {
        // Origin == Host proves nothing on its own — both come from the same
        // request. Without a Host that the server accepts as its own, the
        // static allow-list is the only thing that can vouch for the Origin.
        let req = Request::builder()
            .uri("/api/terminal/ws")
            .header(header::ORIGIN, "http://evil.test")
            .header(header::HOST, "evil.test")
            .body(Body::empty())
            .unwrap();
        let response = test_router_with_origin_guard().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // --- Localhost shell auth tests ---

    /// The gate alone. `allow_remote` true means it demands a credential, so the
    /// helper supplies the key state it checks against.
    fn shell_router(allow_remote: bool) -> Router {
        let state = Arc::new(ShellGateState {
            allow_remote,
            credentials: allow_remote.then(|| {
                crate::middleware::auth::ApiKeyState::new_at(
                    Some("secret-key".to_string()),
                    crate::middleware::auth::HostPolicy::from_bind("127.0.0.1", 3000, &[])
                        .expect("no allowed_hosts entries to reject"),
                    None,
                )
            }),
        });
        Router::new()
            .route("/shell/run", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(
                state,
                localhost_only_shell_auth,
            ))
    }

    fn shell_req(ip: [u8; 4]) -> Request<Body> {
        let mut req = Request::builder()
            .uri("/shell/run")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from((ip, 5000))));
        req
    }

    #[tokio::test]
    async fn localhost_connect_info_is_allowed() {
        let response = shell_router(false)
            .oneshot(shell_req([127, 0, 0, 1]))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn non_localhost_connect_info_is_forbidden() {
        let response = shell_router(false)
            .oneshot(shell_req([10, 0, 0, 8]))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn non_localhost_forwarded_for_is_forbidden_even_with_local_connect_info() {
        let mut req = shell_req([127, 0, 0, 1]);
        req.headers_mut()
            .insert("x-forwarded-for", HeaderValue::from_static("203.0.113.10"));
        let response = shell_router(false).oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // --- Remote shell opt-in ([web] remote_shell) ---

    /// `shell_req` plus a valid Bearer token.
    fn authed_shell_req(ip: [u8; 4]) -> Request<Body> {
        let mut req = shell_req(ip);
        req.headers_mut().insert(
            "authorization",
            HeaderValue::from_static("Bearer secret-key"),
        );
        req
    }

    #[tokio::test]
    async fn remote_shell_gate_admits_a_credentialled_non_localhost_caller() {
        let response = shell_router(true)
            .oneshot(authed_shell_req([10, 0, 0, 8]))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn remote_shell_gate_refuses_an_uncredentialled_caller_even_from_loopback() {
        // The hole this closes, and the reason this gate stopped deferring to
        // `bearer_auth`: that layer waves loopback callers through, which is
        // fine for reading notes and not for a PTY. A browser at an
        // allow-listed name that resolves to 127.0.0.1 arrives AS a loopback
        // caller, `enforce_host` stamps it `HostVerified`, and the WebSocket
        // Origin guard's `Origin == Host` shortcut then passes honestly — so
        // before this, `Host: <allow-listed>` with no cookie and no key
        // returned 101 and a shell.
        for ip in [[127, 0, 0, 1], [10, 0, 0, 8]] {
            let status = shell_router(true)
                .oneshot(shell_req(ip))
                .await
                .unwrap()
                .status();
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{ip:?} must not reach a PTY without a credential"
            );
        }
    }

    #[tokio::test]
    async fn loopback_keeps_its_free_pass_while_the_opt_in_is_off() {
        // The default posture is untouched: with `remote_shell` off, the
        // loopback restriction IS the defence and nothing off-box can reach
        // these routes, so requiring a key would be friction for no gain.
        let status = shell_router(false)
            .oneshot(shell_req([127, 0, 0, 1]))
            .await
            .unwrap()
            .status();
        assert_eq!(status, StatusCode::OK);
    }

    #[test]
    fn remote_shell_active_is_fail_closed_without_an_api_key() {
        // Opt-in without a key = auth disabled — must NOT hand a LAN peer an
        // unauthenticated PTY.
        assert!(!remote_shell_active(true, false));
        assert!(remote_shell_active(true, true));
        assert!(!remote_shell_active(false, true));
    }
}
