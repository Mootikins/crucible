//! Auth bootstrap: exchange the API key for an HttpOnly session cookie.
//!
//! Browsers must never carry the key in a URL (history/log/referrer leakage),
//! and EventSource cannot set an Authorization header — so the browser POSTs
//! the key once and authenticates every subsequent request (including SSE)
//! with the cookie. Programmatic clients keep using `Authorization: Bearer`.
//!
//! Mounted OUTSIDE the bearer-auth layer: it is the way in.

use crate::web::middleware::auth::{verify_api_key, ApiKeyState, AUTH_COOKIE};
use axum::extract::State;
use axum::http::{header::SET_COOKIE, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

/// Session lifetime. Rotating the key (`cru web key --rotate`) invalidates
/// existing cookies immediately regardless of this value.
const SESSION_MAX_AGE_SECS: u64 = 30 * 24 * 60 * 60;

#[derive(Debug, Deserialize)]
struct LoginRequest {
    key: String,
}

pub fn auth_routes(api_key_state: Arc<ApiKeyState>) -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
        .with_state(api_key_state)
}

async fn login(State(state): State<Arc<ApiKeyState>>, Json(req): Json<LoginRequest>) -> Response {
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

    // No Secure attribute: `cru web` serves plain HTTP on the LAN; a Secure
    // cookie would be dropped there. TLS deployments come via `cru tunnel`
    // (terminating proxy), where the transport is already encrypted.
    let cookie = format!(
        "{AUTH_COOKIE}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={SESSION_MAX_AGE_SECS}",
        req.key
    );
    ([(SET_COOKIE, cookie)], StatusCode::NO_CONTENT).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::middleware::auth::bearer_auth;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::Request;
    use axum::middleware;
    use axum::routing::get;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    fn state(key: Option<&str>) -> Arc<ApiKeyState> {
        Arc::new(ApiKeyState {
            api_key: key.map(String::from),
        })
    }

    fn login_request(key: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "key": key }).to_string()))
            .unwrap()
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
        assert!(cookie.starts_with(&format!("{AUTH_COOKIE}=secret-key")));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Path=/"));
    }

    #[tokio::test]
    async fn login_with_wrong_key_is_unauthorized_and_sets_no_cookie() {
        let app = auth_routes(state(Some("secret-key")));
        let resp = app.oneshot(login_request("wrong")).await.unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(resp.headers().get(SET_COOKIE).is_none());
    }

    #[tokio::test]
    async fn login_cookie_grants_access_to_protected_routes() {
        // Full cycle: login → take the Set-Cookie → hit a bearer_auth-guarded
        // route from a non-loopback address using only the cookie.
        let key_state = state(Some("secret-key"));
        let login_app = auth_routes(key_state.clone());
        let resp = login_app
            .oneshot(login_request("secret-key"))
            .await
            .unwrap();
        let set_cookie = resp
            .headers()
            .get(SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap()
            .to_string();
        let cookie_pair = set_cookie.split(';').next().unwrap().to_string();

        let protected = Router::new()
            .route("/api/test", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(key_state, bearer_auth));
        let mut req = Request::builder()
            .uri("/api/test")
            .header("cookie", cookie_pair)
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));

        let resp = protected.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
