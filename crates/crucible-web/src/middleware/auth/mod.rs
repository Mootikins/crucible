//! Authentication and address validation for the web server.
//!
//! [`bearer_auth`] is the single gate every API route passes through. It runs
//! the [`HostPolicy`] check first, because every relaxation after it — auth
//! disabled, loopback caller, same-origin WebSocket — assumes the request was
//! addressed to *us* rather than to a name that merely resolves to us.
//!
//! That check is strict for a loopback caller, who has those relaxations to
//! reach, and gives way to the key check for a caller on another machine, who
//! has none — see [`host_list_is_not_this_requests_defence`].

mod api_key;
mod host;
mod session;
mod shell;

pub use api_key::{
    api_key_path, generate_and_persist_key, resolve_api_key, resolve_api_key_at, verify_api_key,
};
pub use host::{local_names, HostPolicy, HostVerified};
pub use session::{sessions_path, SessionStore, SESSION_TTL};
pub use shell::{
    localhost_only_shell_auth, remote_shell_active, websocket_origin_guard, ShellGateState,
};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

/// Session cookie carrying a derived session token for browser clients (set by
/// `POST /api/auth/login`). HttpOnly, so page JS never touches it, and it
/// rides along on EventSource/SSE requests that cannot set headers.
pub const AUTH_COOKIE: &str = "crucible_auth";

/// Shared state for [`bearer_auth`]: the expected API key, the authorities the
/// server answers to, and the live browser sessions.
#[derive(Clone)]
pub struct ApiKeyState {
    /// The expected API key. `None` means auth is disabled.
    pub api_key: Option<String>,
    /// Authorities accepted in `Host`. Enforced even when `api_key` is `None`:
    /// rebinding is most dangerous precisely when auth is off.
    pub host_policy: HostPolicy,
    /// Sessions minted by `POST /api/auth/login`.
    pub sessions: SessionStore,
}

impl ApiKeyState {
    /// The server's state: sessions are restored from, and written back to,
    /// [`sessions_path`] so a `cru web` restart does not log every browser out.
    pub fn new(api_key: Option<String>, host_policy: HostPolicy) -> Self {
        Self::new_at(api_key, host_policy, sessions_path())
    }

    /// [`ApiKeyState::new`] with an injectable session-file path.
    ///
    /// Tests MUST use this with a `TempDir`-rooted path or `None`: the default
    /// path is the operator's real `~/.config/crucible/sessions.json`, and the
    /// store both READS the live credentials there and WRITES back to them.
    pub fn new_at(
        api_key: Option<String>,
        host_policy: HostPolicy,
        sessions_path: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            api_key,
            host_policy,
            sessions: SessionStore::persistent(sessions_path),
        }
    }
}

/// The one Host check. Returns a refusal to send, or marks the request
/// [`HostVerified`] and returns `None`.
///
/// Reached two ways: [`bearer_auth`] runs it first and unconditionally for the
/// API, and [`host_guard`] is the same check on its own for everything mounted
/// outside that layer — the login bootstrap, `/health`, and the static SPA.
/// A rebound page reaching an unauthenticated endpoint on the app's own origin
/// is a same-origin oracle even when it cannot get past auth, so coverage has
/// to be the whole surface rather than the authenticated part of it.
///
/// `/health` and the static SPA shell sit outside `bearer_auth` entirely, so
/// they get this check only from the [`host_guard`] that `server.rs` wraps the
/// merged app in. Nothing else covers them.
fn enforce_host(state: &ApiKeyState, request: &mut Request<Body>) -> Option<Response> {
    if state.host_policy.accepts(request) {
        request.extensions_mut().insert(HostVerified);
        return None;
    }
    if host_list_is_not_this_requests_defence(state, request) {
        return None;
    }
    tracing::warn!(
        host = ?request.headers().get(header::HOST),
        "Rejecting request addressed to an unexpected Host"
    );
    Some(forbidden_host_response())
}

/// Whether the Host allow-list is the wrong tool for *this* request, and the
/// key check behind it is the right one.
///
/// What rebinding is *for* is the loopback bypass. A page on `evil.test` whose
/// record is rebound to this machine runs in a browser ON it, so its requests
/// arrive from 127.0.0.1, skip auth entirely, and the Host allow-list is the
/// only thing left refusing them. That is worth every name it costs.
///
/// A request from ANOTHER machine has no such shortcut to reach: it presents
/// the API key or [`bearer_auth`] answers 401. Holding it to a list of names
/// buys nothing there, and costs every FQDN the operator's network resolves to
/// this box — `impulse`, `impulse.lan`, a tailnet name, a CNAME, a name only
/// the phone's resolver knows. None of them can be enumerated up front, which
/// is how a wildcard bind that worked by IP came to 403 by name and read as
/// though it had never left loopback.
///
/// Conditioned on a key being configured. Under `api_key = ""` there is no
/// check behind this one, so the allow-list *is* the defence and keeps its
/// strict reading — the same fail-closed shape as [`remote_shell_active`].
///
/// Fails closed on the peer, and note that this is the mirror image of
/// [`caller_is_loopback`]: there, an unknown peer is safely "not local", while
/// here an unknown peer must not be "remote" — that reading would hand the
/// relaxation to a request whose origin we cannot establish. So the address
/// has to be present and has to be non-loopback. A proxy on this machine is
/// therefore NOT covered (its connections arrive from 127.0.0.1); a public name
/// it forwards still belongs in `allowed_hosts`.
///
/// The request is deliberately NOT marked [`HostVerified`]: that marker means
/// "this authority is provably one of ours", which is the claim this path
/// declines to make, and the WebSocket guard reads it as licence to treat
/// `Origin == Host` as same-origin.
fn host_list_is_not_this_requests_defence(state: &ApiKeyState, request: &Request<Body>) -> bool {
    let peer_is_remote = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|peer| !peer.0.ip().is_loopback());

    state.api_key.is_some() && peer_is_remote && !caller_is_loopback(request)
}

/// [`enforce_host`] on its own, for routes mounted outside [`bearer_auth`].
pub async fn host_guard(
    state: axum::extract::State<Arc<ApiKeyState>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    match enforce_host(&state, &mut request) {
        Some(refusal) => refusal,
        None => next.run(request).await,
    }
}

/// Axum middleware that enforces `Authorization: Bearer <key>` on API routes.
///
/// Refuses any request whose `Host` is not an authority this server answers to
/// — unless the request came from another machine, where the key check below
/// is the stronger gate ([`host_list_is_not_this_requests_defence`]). The Host
/// check is first, because every relaxation below it — auth disabled, loopback
/// caller, same-origin WebSocket — assumes the request was addressed to *us*
/// rather than to a name that merely resolves to us.
///
/// Past the Host check, auth is bypassed when:
/// - No API key is configured (auth disabled)
/// - The caller is loopback by [`caller_is_loopback`]'s fail-closed reading —
///   the same one [`localhost_only_shell_auth`] uses
pub async fn bearer_auth(
    state: axum::extract::State<Arc<ApiKeyState>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    if let Some(refusal) = enforce_host(&state, &mut request) {
        return refusal;
    }

    // No key configured — auth disabled.
    let expected_key = match &state.api_key {
        Some(key) => key,
        None => return next.run(request).await,
    };

    // Localhost bypass: loopback callers skip the token check.
    if caller_is_loopback(&request) {
        return next.run(request).await;
    }

    // Check Authorization header for a valid Bearer token.
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    if let Some(header) = auth_header {
        return match header.strip_prefix("Bearer ") {
            Some(token) if constant_time_eq(token.as_bytes(), expected_key.as_bytes()) => {
                next.run(request).await
            }
            _ => unauthorized_response(),
        };
    }

    // Browser clients authenticate with the HttpOnly session cookie set by
    // POST /api/auth/login (EventSource cannot set headers; cookies ride along
    // automatically). Header, when present, always wins above. Tokens are
    // deliberately NOT accepted in the URL — query strings leak through
    // browser history, server logs, and referrers. The cookie carries a minted
    // session token, never the API key: the key would then be unrevocable
    // short of rotating it for every client at once.
    let session_ok = auth_cookie_values(request.headers())
        .any(|token| state.sessions.verify(token, expected_key));
    if session_ok {
        return next.run(request).await;
    }

    unauthorized_response()
}

/// Whether the request's *client* — not merely its TCP peer — is loopback.
///
/// The one answer to that question. `bearer_auth` grants an unauthenticated
/// bypass on it and [`localhost_only_shell_auth`] hands out a PTY on it; two
/// implementations of one question means the weaker one is the real policy, so
/// there is exactly this one.
///
/// Fails closed at every step. Loopback is claimed only when the peer address
/// is loopback *and* every hop named by `X-Forwarded-For` is loopback too:
///
/// - No `X-Forwarded-For` at all — nothing is in front of us, so the peer is
///   the client.
/// - A hop we cannot read as an address (unparseable, empty, an obfuscated
///   `_hidden`/`unknown` token, non-ASCII) proves nothing, and behind a proxy
///   the connection always arrives from loopback — so trusting the peer when
///   the header is unreadable would hand the bypass to any remote client that
///   can make one token unparseable.
/// - *Every* hop, not just the first: a proxy that appends rather than
///   replaces leaves the client's own spoofed `127.0.0.1` at the head of the
///   chain, and reading only the head would take the spoof at face value.
pub(super) fn caller_is_loopback<B>(request: &Request<B>) -> bool {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|peer| peer.0.ip().is_loopback())
        && forwarded_chain_is_loopback(request.headers())
}

fn forwarded_chain_is_loopback(headers: &HeaderMap) -> bool {
    headers.get_all("x-forwarded-for").iter().all(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .all(|hop| parse_forwarded_hop(hop.trim()).is_some_and(|ip| ip.is_loopback()))
        })
    })
}

/// A single `X-Forwarded-For` hop: a bare address, or one carrying a port
/// (some proxies emit `127.0.0.1:54321`).
fn parse_forwarded_hop(hop: &str) -> Option<IpAddr> {
    hop.parse::<IpAddr>()
        .ok()
        .or_else(|| hop.parse::<SocketAddr>().ok().map(|addr| addr.ip()))
}

/// Every [`AUTH_COOKIE`] value the request carries.
///
/// Every one, not the first. Cookies are not isolated by port, so any other
/// service — or an XSS — on a sibling `localhost` port can set its own
/// `crucible_auth` for `Path=/`, and the browser then sends both. Reading only
/// the first would let that page lock the real session out. Multiple `Cookie`
/// headers are read for the same reason plus a mundane one: HTTP/2 is entitled
/// to split them.
pub(crate) fn auth_cookie_values(headers: &HeaderMap) -> impl Iterator<Item = &str> + '_ {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|raw| raw.split(';'))
        .filter_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == AUTH_COOKIE).then_some(value)
        })
}

/// Constant-time byte comparison to prevent timing side-channel attacks
/// on bearer token validation.
///
/// Note: The early return on length mismatch leaks the key length via timing.
/// This is acceptable because the auto-generated API key is always 32 chars
/// (high entropy), making length-based attacks impractical.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

fn forbidden_host_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": {
                "code": StatusCode::FORBIDDEN.as_u16(),
                "message": "Request Host is not an address this server answers to",
            }
        })),
    )
        .into_response()
}

fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": {
                "code": StatusCode::UNAUTHORIZED.as_u16(),
                "message": "Missing or invalid Authorization: Bearer <key>",
            }
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use axum::routing::get;
    use axum::{middleware, Router};
    use tower::ServiceExt;

    /// The port every bearer test's [`HostPolicy`] is built for.
    const TEST_PORT: u16 = 3000;

    /// `new_at(.., None)`, never `new`: `new` reads and rewrites the real
    /// `~/.config/crucible/sessions.json`.
    fn bearer_state(api_key: Option<String>) -> Arc<ApiKeyState> {
        Arc::new(ApiKeyState::new_at(
            api_key,
            HostPolicy::from_bind("127.0.0.1", TEST_PORT, &[]),
            None,
        ))
    }

    fn router_with_state(state: Arc<ApiKeyState>) -> Router {
        Router::new()
            .route("/api/test", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(state, bearer_auth))
    }

    fn test_router_with_bearer(api_key: Option<String>) -> Router {
        router_with_state(bearer_state(api_key))
    }

    /// A request addressed to this server's own authority — what every real
    /// client sends. Tests exercising Host validation set their own.
    fn local_request() -> axum::http::request::Builder {
        Request::builder()
            .uri("/api/test")
            .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
    }

    #[tokio::test]
    async fn bearer_auth_accepts_a_minted_session_cookie() {
        // Rewritten from the old `bearer_auth_accepts_valid_session_cookie`,
        // which put the API key in the cookie — the defect, not the contract.
        let state = bearer_state(Some("secret-key".to_string()));
        let token = state.sessions.issue("secret-key");
        let app = router_with_state(state);

        let req = local_request()
            .header("cookie", format!("other=1; {AUTH_COOKIE}={token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_shadowing_cookie_cannot_lock_out_a_valid_session() {
        // Cookies are not isolated by port, so anything else on a sibling
        // localhost port can set its own `crucible_auth` for `Path=/`. Reading
        // only the first value would hand that page a logout button for the
        // real session. The second header also covers the mundane case: HTTP/2
        // may split `Cookie` across several headers.
        let state = bearer_state(Some("secret-key".to_string()));
        let token = state.sessions.issue("secret-key");
        let app = router_with_state(state);

        let req = local_request()
            .header("cookie", format!("{AUTH_COOKIE}=shadow; other=1"))
            .header("cookie", format!("{AUTH_COOKIE}={token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn bearer_auth_rejects_wrong_session_cookie() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let req = local_request()
            .header("cookie", format!("{AUTH_COOKIE}=wrong"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bearer_auth_header_wins_over_cookie() {
        // A wrong header must not fall through to a valid cookie —
        // explicit credentials fail loudly.
        let state = bearer_state(Some("secret-key".to_string()));
        let token = state.sessions.issue("secret-key");
        let app = router_with_state(state);

        let req = local_request()
            .header("cookie", format!("{AUTH_COOKIE}={token}"))
            .header("authorization", "Bearer wrong")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bearer_auth_no_longer_accepts_url_tokens() {
        // Regression: tokens in URLs leak via history/logs/referrers. The old
        // ?access_token= fallback must stay dead.
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let req = Request::builder()
            .uri("/api/test?access_token=secret-key&token=secret-key")
            .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bearer_auth_passes_when_no_key_configured() {
        let app = test_router_with_bearer(None);

        let req = local_request().body(Body::empty()).unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn bearer_auth_passes_with_valid_token() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let mut req = local_request()
            .header("authorization", "Bearer secret-key")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn bearer_auth_rejects_invalid_token() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let mut req = local_request()
            .header("authorization", "Bearer wrong-key")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bearer_auth_rejects_missing_header() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let mut req = local_request().body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bearer_auth_bypasses_for_localhost() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let mut req = local_request().body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// Every `X-Forwarded-For` shape both loopback gates must agree on, with
    /// the single answer they must both give.
    ///
    /// `bearer_auth` and `localhost_only_shell_auth` ask the same question —
    /// *is the caller loopback?* — about the same request. Two answers to one
    /// question is a bypass waiting for whichever middleware is more generous,
    /// so this table drives both.
    const FORWARDED_FOR_CASES: &[(&[&str], bool)] = &[
        // No proxy in front of us: the peer really is the client.
        (&[], true),
        (&["127.0.0.1"], true),
        (&["::1"], true),
        (&["127.0.0.1:4321"], true),
        // A named client hop — the whole point of the header.
        (&["203.0.113.9"], false),
        // Unparseable, empty, or obfuscated hops prove nothing. Failing open
        // here hands the bypass to anyone who can make the header unreadable.
        (&["not-an-ip"], false),
        (&[""], false),
        (&["_hidden"], false),
        (&["unknown"], false),
        // A proxy that *appends* leaves the client's own spoofed hop in front.
        (&["127.0.0.1, 203.0.113.9"], false),
        (&["203.0.113.9, 127.0.0.1"], false),
        // Split across headers (HTTP/2, or a smuggling attempt).
        (&["127.0.0.1", "203.0.113.9"], false),
    ];

    fn request_forwarded_from(peer: [u8; 4], forwarded: &[&str]) -> Request<Body> {
        let mut req = local_request().body(Body::empty()).unwrap();
        for value in forwarded {
            req.headers_mut()
                .append("x-forwarded-for", HeaderValue::from_str(value).unwrap());
        }
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from((peer, 5000))));
        req
    }

    #[tokio::test]
    async fn the_localhost_bypass_is_refused_unless_the_whole_forwarded_chain_is_loopback() {
        for (forwarded, trusted) in FORWARDED_FOR_CASES {
            let app = test_router_with_bearer(Some("secret-key".to_string()));
            let status = app
                .oneshot(request_forwarded_from([127, 0, 0, 1], forwarded))
                .await
                .unwrap()
                .status();
            let expected = if *trusted {
                StatusCode::OK
            } else {
                StatusCode::UNAUTHORIZED
            };
            assert_eq!(
                status,
                expected,
                "X-Forwarded-For {forwarded:?} must{} grant the localhost bypass",
                if *trusted { "" } else { " not" }
            );
        }
    }

    #[tokio::test]
    async fn both_loopback_gates_answer_the_same_way_for_the_same_request() {
        // The two middlewares used to implement this question separately, and
        // disagreed: `bearer_auth` failed open on an unparseable hop while the
        // shell gate failed closed. Same header, same request, opposite trust.
        let shell_gate = Arc::new(ShellGateState {
            allow_remote: false,
        });
        for (forwarded, trusted) in FORWARDED_FOR_CASES {
            let shell = Router::new()
                .route("/api/test", get(|| async { "ok" }))
                .layer(axum::middleware::from_fn_with_state(
                    shell_gate.clone(),
                    localhost_only_shell_auth,
                ));
            let status = shell
                .oneshot(request_forwarded_from([127, 0, 0, 1], forwarded))
                .await
                .unwrap()
                .status();
            let expected = if *trusted {
                StatusCode::OK
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(
                status, expected,
                "the shell gate must treat X-Forwarded-For {forwarded:?} exactly as bearer_auth does"
            );
        }
    }

    #[tokio::test]
    async fn a_loopback_forwarded_chain_cannot_vouch_for_a_remote_peer() {
        // The header only ever narrows trust: a request that did not arrive
        // from loopback is not local however friendly its X-Forwarded-For is.
        let app = test_router_with_bearer(Some("secret-key".to_string()));
        let status = app
            .oneshot(request_forwarded_from([10, 0, 0, 1], &["127.0.0.1"]))
            .await
            .unwrap()
            .status();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bearer_auth_bypasses_for_ipv6_localhost() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let mut req = Request::builder()
            .uri("/api/test")
            .header(header::HOST, format!("[::1]:{TEST_PORT}"))
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo("[::1]:5000".parse::<SocketAddr>().unwrap()));

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    // --- Host validation (DNS rebinding), through the real middleware ---

    #[tokio::test]
    async fn rebound_host_is_refused_before_the_loopback_bypass_applies() {
        // DNS rebinding: evil.test resolves to 127.0.0.1, so the connection
        // really does arrive from loopback and Origin really does equal Host.
        // Neither fact makes the request local — only the Host authority does.
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let mut req = Request::builder()
            .uri("/api/test")
            .header(header::HOST, "evil.test")
            .header(header::ORIGIN, "http://evil.test")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// One request from another machine, addressed by a name this server was
    /// never told about — the LAN client's ordinary case.
    fn remote_request(host: &str) -> axum::http::request::Builder {
        Request::builder()
            .uri("/api/test")
            .header(header::HOST, host)
    }

    fn from_remote_peer(builder: axum::http::request::Builder) -> Request<Body> {
        let mut req = builder.body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));
        req
    }

    #[tokio::test]
    async fn an_authenticated_client_on_another_machine_may_use_any_name_for_this_server() {
        // Every FQDN the operator's network resolves to this box — `impulse`,
        // `impulse.lan`, a tailnet name, a CNAME — without enumerating any of
        // them. Safe because the request came from another machine and so has
        // no loopback bypass to reach: it authenticated to get here.
        let app = test_router_with_bearer(Some("secret-key".to_string()));
        let req = from_remote_peer(
            remote_request("impulse.tail1234.ts.net:3000")
                .header("authorization", "Bearer secret-key"),
        );

        assert_eq!(app.oneshot(req).await.unwrap().status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn an_unknown_name_from_another_machine_still_has_to_authenticate() {
        // The relaxation moves the refusal from the Host check to the key
        // check; it does not remove one. A 403 here would mean the name was
        // the gate, a 200 would mean nothing was.
        let app = test_router_with_bearer(Some("secret-key".to_string()));
        let req = from_remote_peer(remote_request("evil.test:3000"));

        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn a_request_with_no_peer_address_is_held_to_the_host_list() {
        // The relaxation turns on "this came from another machine", so an
        // absent `ConnectInfo` has to read as "unknown", not as "remote".
        // Read as remote it would be handed to anything that reaches the
        // server without one — including the unauthenticated login bootstrap,
        // where a rebound page could then guess keys and read every answer.
        let app = test_router_with_bearer(Some("secret-key".to_string()));
        let req = remote_request("evil.test:3000")
            .body(Body::empty())
            .unwrap();

        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn with_auth_disabled_every_client_is_held_to_the_host_list() {
        // `api_key = ""`: there is no check behind the Host check, so the Host
        // check is the whole defence and keeps its strict reading.
        let app = test_router_with_bearer(None);
        let req = from_remote_peer(remote_request("evil.test:3000"));

        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn a_name_admitted_only_by_the_remote_relaxation_is_not_marked_verified() {
        // `HostVerified` means "this authority is provably one of ours", and
        // the WebSocket guard reads it as licence to treat Origin == Host as
        // same-origin. The relaxation declines to make that claim, so the
        // marker must be absent and the guard left with its static allow-list.
        let state = bearer_state(Some("secret-key".to_string()));
        let mut relaxed = from_remote_peer(remote_request("impulse.lan:3000"));
        assert!(enforce_host(&state, &mut relaxed).is_none());
        assert!(relaxed.extensions().get::<HostVerified>().is_none());

        let mut ours = from_remote_peer(remote_request(&format!("127.0.0.1:{TEST_PORT}")));
        assert!(enforce_host(&state, &mut ours).is_none());
        assert!(ours.extensions().get::<HostVerified>().is_some());
    }

    /// Drive one Host value through the real middleware from a loopback peer —
    /// the DNS-rebinding vantage point, where every other gate is already open.
    async fn status_for_host(host: &str) -> StatusCode {
        let app = test_router_with_bearer(Some("secret-key".to_string()));
        let mut req = Request::builder()
            .uri("/api/test")
            .header(header::HOST, host)
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));
        app.oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn every_spelling_of_a_rebound_host_is_refused() {
        // The variations that make a naive string allowlist leak: a trailing
        // dot (same name to a resolver), mixed case, an appended expected
        // port, and a name that merely *contains* an expected authority.
        for host in [
            "evil.test",
            "evil.test.",
            "EVIL.test",
            "evil.test:3000",
            "evil.test.:3000",
            "127.0.0.1.evil.test:3000",
            "localhost.evil.test:3000",
            "user@127.0.0.1:3000",
            "127.0.0.1:3000@evil.test",
            "127.0.0.1:3000/../evil.test",
        ] {
            assert_eq!(
                status_for_host(host).await,
                StatusCode::FORBIDDEN,
                "Host {host:?} must not be accepted"
            );
        }
    }

    #[tokio::test]
    async fn a_port_mismatch_is_refused() {
        // The rebound page controls the port it navigates to; only the port we
        // actually bound is ours.
        assert_eq!(
            status_for_host("127.0.0.1:3001").await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(status_for_host("127.0.0.1").await, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn a_missing_or_duplicated_host_is_refused() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));
        let mut req = Request::builder()
            .uri("/api/test")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));
        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN,
            "no Host at all must fail closed"
        );

        // Two Host headers: whichever one we picked, a layer behind us could
        // pick the other. Refuse rather than choose.
        let app = test_router_with_bearer(Some("secret-key".to_string()));
        let mut req = Request::builder()
            .uri("/api/test")
            .header(header::HOST, "127.0.0.1:3000")
            .body(Body::empty())
            .unwrap();
        req.headers_mut()
            .append(header::HOST, HeaderValue::from_static("evil.test"));
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));
        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn an_absolute_form_target_may_not_disagree_with_host() {
        // HTTP/1.1 absolute-form (and HTTP/2 `:authority`) puts an authority on
        // the URI. If it disagrees with Host, one of the two is a smuggled
        // value — refuse instead of picking a winner.
        let app = test_router_with_bearer(Some("secret-key".to_string()));
        let mut req = Request::builder()
            .uri("http://evil.test/api/test")
            .header(header::HOST, "127.0.0.1:3000")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));
        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn the_bind_authority_and_its_loopback_spellings_are_accepted() {
        // The legitimate cases the guard must not break.
        for host in [
            "127.0.0.1:3000",
            "localhost:3000",
            "LOCALHOST:3000",
            "localhost.:3000",
            "[::1]:3000",
            "[0:0:0:0:0:0:0:1]:3000",
        ] {
            assert_eq!(
                status_for_host(host).await,
                StatusCode::OK,
                "Host {host:?} is this server and must be accepted"
            );
        }
    }

    // --- Session token (the cookie is not the key) ---

    #[tokio::test]
    async fn the_api_key_itself_is_never_accepted_as_a_session_cookie() {
        // The cookie must carry a derived, revocable token; presenting the
        // long-lived credential in the cookie slot must fail.
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let mut req = Request::builder()
            .uri("/api/test")
            .header(header::HOST, "127.0.0.1:3000")
            .header("cookie", format!("{AUTH_COOKIE}=secret-key"))
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
