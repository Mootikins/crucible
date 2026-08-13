//! Auth bootstrap: exchange the API key for an HttpOnly session cookie.
//!
//! Browsers must never carry the key in a URL (history/log/referrer leakage),
//! and EventSource cannot set an Authorization header — so the browser POSTs
//! the key once and authenticates every subsequent request (including SSE)
//! with the cookie. Programmatic clients keep using `Authorization: Bearer`.
//!
//! The cookie carries a *minted session token*, not the key. Putting the key
//! itself in the jar makes the long-lived credential unrevocable: the only way
//! to retire one browser is to rotate for every client at once. A token
//! expires on its own, can be dropped by `POST /api/auth/logout`, and dies
//! with the key it was minted from.
//!
//! Mounted OUTSIDE the bearer-auth layer: it is the way in.

use crate::middleware::auth::{
    auth_cookie_values, host_guard, request_is_tls, verify_api_key, ApiKeyState, AUTH_COOKIE,
    SESSION_TTL,
};
use axum::extract::{ConnectInfo, State};
use axum::http::{header::SET_COOKIE, HeaderMap, StatusCode};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Json, Router};
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

/// The request's peer address, absent when nothing installed [`ConnectInfo`].
///
/// `Option<ConnectInfo<_>>` is not an extractor in axum 0.8 — only the
/// infallible `Extension` wrapper around it is — and a mandatory `ConnectInfo`
/// would turn a missing peer into a 500 on the one route that has to stay
/// reachable.
type Peer = Option<Extension<ConnectInfo<SocketAddr>>>;

fn peer_addr(peer: Peer) -> Option<SocketAddr> {
    peer.map(|Extension(ConnectInfo(addr))| addr)
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    key: String,
}

pub fn auth_routes(api_key_state: Arc<ApiKeyState>) -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        // Unauthenticated, but not unaddressed: without this, a DNS-rebound
        // page could hammer login same-origin and read every answer, turning
        // the bootstrap into a key-guessing oracle with no CORS in its way.
        .layer(middleware::from_fn_with_state(
            api_key_state.clone(),
            host_guard,
        ))
        .with_state(api_key_state)
}

/// Cookie attributes shared by the mint and the clear.
///
/// `Secure` is conditional on the request having reached the browser over TLS
/// ([`request_is_tls`]), never unconditional: `cru web` serves plain HTTP on the
/// LAN, and a browser silently declines to store a `Secure` cookie delivered
/// over `http`, so setting it always would make login appear to succeed and
/// leave the user logged out. Behind a TLS-terminating proxy it is set, and then
/// the cookie stops being something a later plain-HTTP request to the same name
/// can carry back out.
///
/// The clear needs the same treatment as the mint: a `Secure` clear rejected on
/// a plain-HTTP connection would leave the cookie — and so the browser's idea of
/// its session — in place.
fn session_cookie(value: &str, max_age_secs: u64, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{AUTH_COOKIE}={value}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age_secs}{secure}"
    )
}

async fn login(
    State(state): State<Arc<ApiKeyState>>,
    headers: HeaderMap,
    peer: Peer,
    Json(req): Json<LoginRequest>,
) -> Response {
    if !verify_api_key(&state, &req.key) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": {
                    "code": StatusCode::UNAUTHORIZED.as_u16(),
                    "message": "Invalid API key",
                }
            })),
        )
            .into_response();
    }

    // Auth disabled (`api_key = ""`): nothing to exchange, and a token bound to
    // no key would be a credential the operator never asked to exist.
    let Some(api_key) = state.api_key.as_deref() else {
        return StatusCode::NO_CONTENT.into_response();
    };

    let token = state.sessions.issue(api_key);
    let cookie = session_cookie(
        &token,
        SESSION_TTL.as_secs(),
        request_is_tls(&headers, peer_addr(peer)),
    );
    ([(SET_COOKIE, cookie)], StatusCode::NO_CONTENT).into_response()
}

/// Drop the presented session and clear the cookie. Unauthenticated by design:
/// the only thing it can destroy is the credential the caller already holds.
async fn logout(State(state): State<Arc<ApiKeyState>>, headers: HeaderMap, peer: Peer) -> Response {
    // Revoke every presented token, not just the first: a shadowing cookie set
    // by a sibling localhost port would otherwise mask the real session and
    // leave it live after the user asked to be logged out.
    for token in auth_cookie_values(&headers) {
        state.sessions.revoke(token);
    }
    let cookie = session_cookie("", 0, request_is_tls(&headers, peer_addr(peer)));
    ([(SET_COOKIE, cookie)], StatusCode::NO_CONTENT).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::auth::{bearer_auth, HostPolicy};
    use axum::body::Body;
    use axum::http::{header, HeaderValue, Request};
    use axum::middleware;
    use axum::routing::get;
    use tower::ServiceExt;

    const TEST_PORT: u16 = 3000;

    // `new_at(.., None)` keeps the session store in memory. `ApiKeyState::new`
    // persists to `~/.config/crucible/sessions.json`, so using it here would make
    // the unit tests read and write the developer's real config directory.
    fn state(key: Option<&str>) -> Arc<ApiKeyState> {
        Arc::new(ApiKeyState::new_at(
            key.map(String::from),
            HostPolicy::from_bind("127.0.0.1", TEST_PORT, &[]).expect("bind-derived policy"),
            None,
        ))
    }

    fn login_request(key: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
            .header("content-type", "application/json")
            .body(Body::from(json!({ "key": key }).to_string()))
            .unwrap()
    }

    /// A request as a TLS-terminating proxy delivers it: the proxy dials this
    /// server's local port, so the connection is loopback and the browser's
    /// original scheme survives only in `X-Forwarded-Proto`.
    fn forwarded_from(req: Request<Body>, proto: &str, peer: [u8; 4]) -> Request<Body> {
        let mut req = req;
        req.headers_mut().insert(
            "x-forwarded-proto",
            HeaderValue::from_str(proto).expect("header value"),
        );
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from((peer, 5000))));
        req
    }

    /// Whether the `Set-Cookie` carries `Secure`, read as an attribute rather
    /// than searched for as a substring — the random session token sits in the
    /// same header and must not be able to answer this question.
    fn has_secure_attribute(resp: &Response) -> bool {
        resp.headers()
            .get(SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .expect("Set-Cookie present")
            .split(';')
            .skip(1)
            .any(|attr| attr.trim().eq_ignore_ascii_case("Secure"))
    }

    fn logout_request(cookie: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/auth/logout")
            .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
            .header("cookie", cookie)
            .body(Body::empty())
            .unwrap()
    }

    /// The `name=value` pair from a `Set-Cookie`, or `None`.
    fn cookie_pair(resp: &Response) -> Option<String> {
        Some(
            resp.headers()
                .get(SET_COOKIE)?
                .to_str()
                .ok()?
                .split(';')
                .next()?
                .to_string(),
        )
    }

    #[tokio::test]
    async fn login_with_valid_key_sets_httponly_cookie() {
        let app = auth_routes(state(Some("secret-key")));
        let resp = app.oneshot(login_request("secret-key")).await.unwrap();

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let cookie = resp
            .headers()
            .get(SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .expect("Set-Cookie present");
        assert!(cookie.starts_with(&format!("{AUTH_COOKIE}=")));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains(&format!("Max-Age={}", SESSION_TTL.as_secs())));
    }

    #[tokio::test]
    async fn login_with_wrong_key_is_unauthorized_and_sets_no_cookie() {
        let app = auth_routes(state(Some("secret-key")));
        let resp = app.oneshot(login_request("wrong")).await.unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(resp.headers().get(SET_COOKIE).is_none());
    }

    /// Drive a cookie through a bearer_auth-guarded route from a non-loopback
    /// address, so only the cookie can let it through.
    async fn remote_request_with_cookie(key_state: Arc<ApiKeyState>, cookie: &str) -> StatusCode {
        let protected = Router::new()
            .route("/api/test", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(key_state, bearer_auth));
        let mut req = Request::builder()
            .uri("/api/test")
            .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
            .header("cookie", cookie)
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));
        protected.oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn login_cookie_grants_access_to_protected_routes() {
        // Full cycle: login → take the Set-Cookie → hit a bearer_auth-guarded
        // route from a non-loopback address using only the cookie.
        let key_state = state(Some("secret-key"));
        let resp = auth_routes(key_state.clone())
            .oneshot(login_request("secret-key"))
            .await
            .unwrap();
        let cookie = cookie_pair(&resp).expect("Set-Cookie present");

        assert_eq!(
            remote_request_with_cookie(key_state, &cookie).await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn rotating_the_api_key_invalidates_outstanding_session_cookies() {
        // Sessions are bound to the key that minted them, so `cru web key
        // --rotate` retires every browser without having to reach any of them.
        let old_state = state(Some("old-key"));
        let resp = auth_routes(old_state.clone())
            .oneshot(login_request("old-key"))
            .await
            .unwrap();
        let cookie = cookie_pair(&resp).expect("Set-Cookie present");

        assert_eq!(
            remote_request_with_cookie(old_state.clone(), &cookie).await,
            StatusCode::OK
        );

        // Same process, same live session store — only the key has changed.
        let rotated = Arc::new(ApiKeyState {
            api_key: Some("new-key".to_string()),
            host_policy: old_state.host_policy.clone(),
            sessions: old_state.sessions.clone(),
        });
        assert_eq!(
            remote_request_with_cookie(rotated, &cookie).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn logout_revokes_the_presented_session() {
        let key_state = state(Some("secret-key"));
        let resp = auth_routes(key_state.clone())
            .oneshot(login_request("secret-key"))
            .await
            .unwrap();
        let cookie = cookie_pair(&resp).expect("Set-Cookie present");

        let resp = auth_routes(key_state.clone())
            .oneshot(logout_request(&cookie))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert_eq!(cookie_pair(&resp).as_deref(), Some("crucible_auth="));

        assert_eq!(
            remote_request_with_cookie(key_state, &cookie).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn two_logins_get_independent_revocable_sessions() {
        let key_state = state(Some("secret-key"));
        let first = cookie_pair(
            &auth_routes(key_state.clone())
                .oneshot(login_request("secret-key"))
                .await
                .unwrap(),
        )
        .unwrap();
        let second = cookie_pair(
            &auth_routes(key_state.clone())
                .oneshot(login_request("secret-key"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_ne!(first, second, "each login must mint its own token");

        auth_routes(key_state.clone())
            .oneshot(logout_request(&first))
            .await
            .unwrap();

        assert_eq!(
            remote_request_with_cookie(key_state.clone(), &first).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            remote_request_with_cookie(key_state, &second).await,
            StatusCode::OK,
            "revoking one browser must not log out the others"
        );
    }

    #[tokio::test]
    async fn login_mints_no_cookie_when_auth_is_disabled() {
        // `api_key = ""` means no auth at all; a session token bound to no key
        // would be a credential nobody asked for.
        let app = auth_routes(state(None));
        let resp = app.oneshot(login_request("anything")).await.unwrap();

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(resp.headers().get(SET_COOKIE).is_none());
    }

    #[tokio::test]
    async fn login_cookie_carries_a_derived_token_not_the_api_key() {
        // The long-lived credential must not be the thing sitting in the
        // browser jar: it cannot be revoked without rotating for everyone.
        let app = auth_routes(state(Some("secret-key")));
        let resp = app.oneshot(login_request("secret-key")).await.unwrap();

        let cookie = resp
            .headers()
            .get(SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .expect("Set-Cookie present");
        let value = cookie
            .split(';')
            .next()
            .unwrap()
            .strip_prefix(&format!("{AUTH_COOKIE}="))
            .expect("auth cookie");
        assert_ne!(value, "secret-key");
        assert!(!cookie.contains("secret-key"), "cookie leaks the API key");
    }

    // --- `Secure`, conditional on the scheme the browser actually used ---

    #[tokio::test]
    async fn a_cookie_minted_over_plain_http_carries_no_secure_attribute() {
        // `cru web` on the LAN is plain HTTP. A browser silently refuses to
        // store a `Secure` cookie delivered over `http`, so an unconditional
        // attribute would make login succeed and leave the user logged out.
        for req in [
            login_request("secret-key"),
            forwarded_from(login_request("secret-key"), "http", [127, 0, 0, 1]),
        ] {
            let resp = auth_routes(state(Some("secret-key")))
                .oneshot(req)
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::NO_CONTENT);
            assert!(
                !has_secure_attribute(&resp),
                "a cookie for an http request must not be Secure"
            );
        }
    }

    #[tokio::test]
    async fn a_cookie_minted_behind_a_tls_terminating_proxy_carries_secure() {
        // The only TLS deployment there is: a proxy (`cru tunnel`'s cloudflared
        // or tailscale funnel) terminates and forwards to the local port, so the
        // peer is loopback and `X-Forwarded-Proto` is the only surviving record
        // of the browser's scheme.
        let resp = auth_routes(state(Some("secret-key")))
            .oneshot(forwarded_from(
                login_request("secret-key"),
                "https",
                [127, 0, 0, 1],
            ))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(
            has_secure_attribute(&resp),
            "a cookie for an https request must be Secure"
        );
    }

    #[tokio::test]
    async fn a_forwarded_scheme_from_another_machine_does_not_decide_the_attribute() {
        // Only a proxy on this machine can have terminated TLS in front of us,
        // because that is the only thing this server's local port is dialed by.
        // A client on the LAN sending the header is just a client talking about
        // itself, so the header is discarded rather than believed.
        let resp = auth_routes(state(Some("secret-key")))
            .oneshot(forwarded_from(
                login_request("secret-key"),
                "https",
                [10, 0, 0, 1],
            ))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(!has_secure_attribute(&resp));
    }

    #[tokio::test]
    async fn the_clearing_cookie_carries_the_same_secure_attribute_as_the_mint() {
        // A `Secure` clear sent over plain HTTP is dropped, and a non-`Secure`
        // one is what logout has to send there — otherwise the browser keeps a
        // cookie the server has already revoked and still believes it is signed
        // in. Both directions, since the mint is conditional.
        let key_state = state(Some("secret-key"));
        let over_tls = auth_routes(key_state.clone())
            .oneshot(forwarded_from(
                logout_request("crucible_auth=whatever"),
                "https",
                [127, 0, 0, 1],
            ))
            .await
            .unwrap();
        assert!(has_secure_attribute(&over_tls));

        let over_http = auth_routes(key_state)
            .oneshot(logout_request("crucible_auth=whatever"))
            .await
            .unwrap();
        assert!(!has_secure_attribute(&over_http));
    }

    #[tokio::test]
    async fn login_from_a_rebound_host_is_refused_without_answering() {
        // The bootstrap is unauthenticated, so a rebound page reaching it
        // same-origin could otherwise guess keys and read every answer. It
        // must not even learn whether a guess was wrong.
        let app = auth_routes(state(Some("secret-key")));
        let req = Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header(header::HOST, "evil.test")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "key": "secret-key" }).to_string()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(resp.headers().get(SET_COOKIE).is_none());
    }
}
