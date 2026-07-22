use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

/// Session cookie carrying the API key for browser clients (set by
/// `POST /api/auth/login`). HttpOnly, so page JS never touches it, and it
/// rides along on EventSource/SSE requests that cannot set headers.
pub const AUTH_COOKIE: &str = "crucible_auth";

// ---------------------------------------------------------------------------
// Bearer token auth
// ---------------------------------------------------------------------------

/// Shared state holding the expected API key for Bearer auth.
#[derive(Clone)]
pub struct ApiKeyState {
    /// The expected API key. `None` means auth is disabled.
    pub api_key: Option<String>,
}

/// Axum middleware that enforces `Authorization: Bearer <key>` on API routes.
///
/// Bypasses auth when:
/// - No API key is configured (auth disabled)
/// - The request originates from a loopback address
pub async fn bearer_auth(
    state: axum::extract::State<Arc<ApiKeyState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // No key configured — auth disabled.
    let expected_key = match &state.api_key {
        Some(key) => key,
        None => return next.run(request).await,
    };

    // Localhost bypass: loopback callers skip the token check.
    let mut is_localhost = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().is_loopback())
        .unwrap_or(false);

    if is_localhost {
        // If behind a proxy, X-Forwarded-For reveals the real client
        let forwarded_for = request
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok());
        if let Some(forwarded) = forwarded_for {
            // If there's a forwarded header, don't trust localhost
            let first_ip = forwarded.split(',').next().unwrap_or("").trim();
            if !first_ip.is_empty() {
                if let Ok(ip) = first_ip.parse::<std::net::IpAddr>() {
                    if !ip.is_loopback() {
                        is_localhost = false;
                    }
                }
            }
        }
    }

    if is_localhost {
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
    // browser history, server logs, and referrers.
    if let Some(cookie_key) = auth_cookie_value(request.headers()) {
        if constant_time_eq(cookie_key.as_bytes(), expected_key.as_bytes()) {
            return next.run(request).await;
        }
    }

    unauthorized_response()
}

/// Extract the value of the [`AUTH_COOKIE`] from the `Cookie` header, if any.
fn auth_cookie_value(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    raw.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == AUTH_COOKIE).then(|| value.to_string())
    })
}

/// Constant-time check of a provided key against the configured one.
/// `None` configured key means auth is disabled — everything verifies.
pub fn verify_api_key(state: &ApiKeyState, provided: &str) -> bool {
    match &state.api_key {
        Some(expected) => constant_time_eq(provided.as_bytes(), expected.as_bytes()),
        None => true,
    }
}

/// Load API key from config, fall back to file, or generate a new one.
///
/// Resolution order:
/// 1. Explicit key from `WebConfig.api_key` (pass `configured_key`)
///    - Empty string `""` disables auth entirely (returns `None`).
/// 2. Read from `~/.config/crucible/api_key`
/// 3. Generate a random 32-char alphanumeric key and persist it there
pub fn resolve_api_key(configured_key: Option<&str>) -> Option<String> {
    resolve_api_key_at(configured_key, api_key_path())
}

/// [`resolve_api_key`] with an injectable key-file path.
///
/// Tests MUST use this with a TempDir-rooted path: the default path is the
/// developer's real `~/.config/crucible/api_key`, and the fallback both
/// READS that credential and, when absent/empty, WRITES a generated one —
/// neither may ever happen from a test.
pub fn resolve_api_key_at(
    configured_key: Option<&str>,
    key_path: Option<std::path::PathBuf>,
) -> Option<String> {
    match configured_key {
        // Explicitly set to empty string — auth disabled.
        Some("") => return None,
        // Explicitly set to a value — use it.
        Some(key) => return Some(key.to_string()),
        None => {}
    }

    let key_path = key_path?;

    if key_path.exists() {
        let contents = std::fs::read_to_string(&key_path).ok()?;
        let trimmed = contents.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    generate_and_persist_key(&key_path)
}

/// Path of the persisted API key file (`~/.config/crucible/api_key`).
pub fn api_key_path() -> Option<std::path::PathBuf> {
    Some(dirs::config_dir()?.join("crucible").join("api_key"))
}

/// Generate a fresh random key and persist it (0600 on unix), replacing any
/// existing one. Used at first startup and by `cru web key --rotate`.
pub fn generate_and_persist_key(key_path: &std::path::Path) -> Option<String> {
    use rand::RngExt;
    let key: String = rand::rng()
        .sample_iter(rand::distr::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    if let Some(parent) = key_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(key_path)
            .ok()?;
        f.write_all(key.as_bytes()).ok()?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(key_path, &key).ok()?;
    }

    tracing::info!("Generated new API key at {}", key_path.display());

    Some(key)
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

// ---------------------------------------------------------------------------
// Localhost-only shell auth (pre-existing)
// ---------------------------------------------------------------------------

/// Rejects WebSocket upgrades whose `Origin` isn't in the allow-list.
///
/// Browsers do not apply CORS/same-origin policy to WebSocket handshakes, so
/// `CorsLayer` never fires for them — a malicious page could otherwise open
/// `ws://localhost:PORT/api/terminal/ws` and, because the request originates
/// from loopback, sail past the localhost auth bypass into a real shell
/// (Cross-Site WebSocket Hijacking → RCE). Browsers *do* always send `Origin`
/// on WS handshakes, so validating it against the same list used for CORS
/// blocks cross-site pages while allowing the app's own frontend. A missing
/// `Origin` means a non-browser client (e.g. a native ws tool), which the
/// route's localhost gate already covers.
pub async fn websocket_origin_guard(
    State(allowed): State<Arc<Vec<HeaderValue>>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        // Same-origin is always fine — CSWSH means a CROSS-site page driving
        // the socket. The static allowlist can't know every host the server
        // is reachable as (LAN IP, hostname), but the request's own Host
        // header does.
        let same_origin = origin_matches_host(origin, request.headers().get(header::HOST));
        if !same_origin && !allowed.iter().any(|a| a == origin) {
            tracing::warn!(
                ?origin,
                "Rejecting WebSocket upgrade from disallowed Origin"
            );
            return (StatusCode::FORBIDDEN, "Origin not allowed").into_response();
        }
    }
    next.run(request).await
}

/// `Origin: http(s)://<authority>` matches the request's `Host: <authority>`.
fn origin_matches_host(origin: &HeaderValue, host: Option<&HeaderValue>) -> bool {
    let (Ok(origin), Some(Ok(host))) = (origin.to_str(), host.map(|h| h.to_str())) else {
        return false;
    };
    origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .is_some_and(|authority| authority == host)
}

/// Whether non-loopback shell/terminal access is active. Fail-closed: the
/// `[server] remote_shell = true` config opt-in (or `cru web --remote-shell`)
/// only takes effect when an API key is configured — otherwise it would hand
/// any LAN peer an UNAUTHENTICATED PTY. With a key, remote requests still
/// pass bearer_auth (Bearer header or session cookie) like every other API
/// route.
pub fn remote_shell_active(opted_in: bool, api_key_configured: bool) -> bool {
    opted_in && api_key_configured
}

/// State for [`localhost_only_shell_auth`]: whether the loopback restriction
/// is lifted (already fail-closed via [`remote_shell_active`]).
#[derive(Clone)]
pub struct ShellGateState {
    pub allow_remote: bool,
}

pub async fn localhost_only_shell_auth(
    State(state): State<Arc<ShellGateState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if state.allow_remote {
        return next.run(request).await;
    }

    let headers = request.headers();
    if !forwarded_for_is_localhost(headers) {
        return forbidden_response();
    }

    let remote_addr = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0);

    match remote_addr {
        Some(addr) if addr.ip().is_loopback() => next.run(request).await,
        _ => forbidden_response(),
    }
}

fn forwarded_for_is_localhost(headers: &HeaderMap) -> bool {
    let Some(value) = headers.get("x-forwarded-for") else {
        return true;
    };

    let Ok(value) = value.to_str() else {
        return false;
    };

    let Some(first_hop) = value.split(',').next().map(str::trim) else {
        return false;
    };

    let Some(ip_addr) = parse_ip_or_socket_addr(first_hop) else {
        return false;
    };

    ip_addr.is_loopback()
}

fn parse_ip_or_socket_addr(value: &str) -> Option<IpAddr> {
    if let Ok(ip) = value.parse::<IpAddr>() {
        return Some(ip);
    }

    if let Ok(addr) = value.parse::<SocketAddr>() {
        return Some(addr.ip());
    }

    None
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
    use axum::routing::get;
    use axum::{middleware, Router};
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
        // Non-browser clients send no Origin; the route's localhost gate covers them.
        let req = Request::builder()
            .uri("/api/terminal/ws")
            .body(Body::empty())
            .unwrap();
        let response = test_router_with_origin_guard().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    // --- Bearer auth tests ---

    fn test_router_with_bearer(api_key: Option<String>) -> Router {
        let state = Arc::new(ApiKeyState { api_key });
        Router::new()
            .route("/api/test", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(state, bearer_auth))
    }

    #[tokio::test]
    async fn bearer_auth_accepts_valid_session_cookie() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let req = Request::builder()
            .uri("/api/test")
            .header("cookie", format!("other=1; {AUTH_COOKIE}=secret-key"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn bearer_auth_rejects_wrong_session_cookie() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let req = Request::builder()
            .uri("/api/test")
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
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let req = Request::builder()
            .uri("/api/test")
            .header("cookie", format!("{AUTH_COOKIE}=secret-key"))
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
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn verify_api_key_matches_and_disabled_auth_accepts_all() {
        let enabled = ApiKeyState {
            api_key: Some("secret-key".into()),
        };
        assert!(verify_api_key(&enabled, "secret-key"));
        assert!(!verify_api_key(&enabled, "wrong"));

        let disabled = ApiKeyState { api_key: None };
        assert!(verify_api_key(&disabled, "anything"));
    }

    #[tokio::test]
    async fn bearer_auth_passes_when_no_key_configured() {
        let app = test_router_with_bearer(None);

        let req = Request::builder()
            .uri("/api/test")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn bearer_auth_passes_with_valid_token() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let mut req = Request::builder()
            .uri("/api/test")
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

        let mut req = Request::builder()
            .uri("/api/test")
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

        let mut req = Request::builder()
            .uri("/api/test")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bearer_auth_bypasses_for_localhost() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let mut req = Request::builder()
            .uri("/api/test")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn bearer_auth_bypasses_for_ipv6_localhost() {
        let app = test_router_with_bearer(Some("secret-key".to_string()));

        let mut req = Request::builder()
            .uri("/api/test")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo("[::1]:5000".parse::<SocketAddr>().unwrap()));

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn resolve_api_key_returns_none_for_empty_string() {
        assert!(resolve_api_key(Some("")).is_none());
    }

    #[test]
    fn resolve_api_key_returns_explicit_value() {
        assert_eq!(resolve_api_key(Some("my-key")), Some("my-key".to_string()));
    }

    // The file-fallback paths are exercised ONLY through the injectable
    // variant — resolve_api_key(None) reads (and can create) the real
    // ~/.config/crucible/api_key, which a test must never touch.
    #[test]
    fn resolve_api_key_at_reads_and_trims_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("api_key");
        std::fs::write(&path, "  stored-key\n").unwrap();
        assert_eq!(
            resolve_api_key_at(None, Some(path)),
            Some("stored-key".to_string())
        );
    }

    #[test]
    fn resolve_api_key_at_generates_and_persists_when_missing_or_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("api_key");

        let generated = resolve_api_key_at(None, Some(path.clone())).expect("generated key");
        assert_eq!(generated.len(), 32);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), generated);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        // A second resolve returns the persisted key, not a fresh one.
        assert_eq!(resolve_api_key_at(None, Some(path)), Some(generated));
    }

    #[test]
    fn resolve_api_key_at_without_path_disables_auth() {
        assert_eq!(resolve_api_key_at(None, None), None);
    }

    // --- Localhost shell auth tests ---

    fn shell_router(allow_remote: bool) -> Router {
        let state = Arc::new(ShellGateState { allow_remote });
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

    // --- Remote shell opt-in ([server] remote_shell) ---

    #[tokio::test]
    async fn remote_shell_gate_allows_non_localhost_when_active() {
        // bearer_auth (layered separately in the real router) still gates
        // the request — this middleware only lifts the loopback restriction.
        let response = shell_router(true)
            .oneshot(shell_req([10, 0, 0, 8]))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn remote_shell_active_is_fail_closed_without_an_api_key() {
        // Opt-in without a key = auth disabled — must NOT hand a LAN peer an
        // unauthenticated PTY.
        assert!(!remote_shell_active(true, false));
        assert!(remote_shell_active(true, true));
        assert!(!remote_shell_active(false, true));
    }

    // --- Same-origin WebSocket acceptance ---

    #[tokio::test]
    async fn origin_guard_allows_same_origin_lan_host() {
        // A LAN browser at http://192.168.0.16:3001 isn't in the static
        // allowlist, but its Origin matches the request's own Host.
        let req = Request::builder()
            .uri("/api/terminal/ws")
            .header(header::ORIGIN, "http://192.168.0.16:3001")
            .header(header::HOST, "192.168.0.16:3001")
            .body(Body::empty())
            .unwrap();
        let response = test_router_with_origin_guard().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn origin_guard_still_rejects_cross_origin_with_mismatched_host() {
        let req = Request::builder()
            .uri("/api/terminal/ws")
            .header(header::ORIGIN, "http://evil.example")
            .header(header::HOST, "192.168.0.16:3001")
            .body(Body::empty())
            .unwrap();
        let response = test_router_with_origin_guard().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
