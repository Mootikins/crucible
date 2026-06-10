use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

const ALLOW_REMOTE_SHELL_ENV: &str = "CRUCIBLE_ALLOW_REMOTE_SHELL";

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

    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            if constant_time_eq(token.as_bytes(), expected_key.as_bytes()) {
                next.run(request).await
            } else {
                unauthorized_response()
            }
        }
        _ => unauthorized_response(),
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
    match configured_key {
        // Explicitly set to empty string — auth disabled.
        Some("") => return None,
        // Explicitly set to a value — use it.
        Some(key) => return Some(key.to_string()),
        None => {}
    }

    let key_path = dirs::config_dir()?.join("crucible").join("api_key");

    if key_path.exists() {
        let contents = std::fs::read_to_string(&key_path).ok()?;
        let trimmed = contents.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    // Generate a random key.
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
            .open(&key_path)
            .ok()?;
        f.write_all(key.as_bytes()).ok()?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&key_path, &key).ok()?;
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

pub async fn localhost_only_shell_auth(request: Request<Body>, next: Next) -> Response {
    if allow_remote_shell() {
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

fn allow_remote_shell() -> bool {
    std::env::var(ALLOW_REMOTE_SHELL_ENV)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
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

    // --- Bearer auth tests ---

    fn test_router_with_bearer(api_key: Option<String>) -> Router {
        let state = Arc::new(ApiKeyState { api_key });
        Router::new()
            .route("/api/test", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(state, bearer_auth))
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

    // --- Localhost shell auth tests (pre-existing) ---

    #[tokio::test]
    async fn localhost_connect_info_is_allowed() {
        let app = Router::new()
            .route("/shell/run", get(|| async { "ok" }))
            .layer(middleware::from_fn(localhost_only_shell_auth));

        let mut req = Request::builder()
            .uri("/shell/run")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));

        let response = app.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn non_localhost_connect_info_is_forbidden() {
        let app = Router::new()
            .route("/shell/run", get(|| async { "ok" }))
            .layer(middleware::from_fn(localhost_only_shell_auth));

        let mut req = Request::builder()
            .uri("/shell/run")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 8], 5000))));

        let response = app.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn non_localhost_forwarded_for_is_forbidden_even_with_local_connect_info() {
        let app = Router::new()
            .route("/shell/run", get(|| async { "ok" }))
            .layer(middleware::from_fn(localhost_only_shell_auth));

        let mut req = Request::builder()
            .uri("/shell/run")
            .header("x-forwarded-for", "203.0.113.10")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));

        let response = app.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
