use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::net::{IpAddr, SocketAddr};

const ALLOW_REMOTE_SHELL_ENV: &str = "CRUCIBLE_ALLOW_REMOTE_SHELL";

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

    #[tokio::test]
    async fn localhost_connect_info_is_allowed() {
        let app = Router::new().route("/shell/run", get(|| async { "ok" })).layer(middleware::from_fn(localhost_only_shell_auth));

        let mut req = Request::builder()
            .uri("/shell/run")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));

        let response = app.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn non_localhost_connect_info_is_forbidden() {
        let app = Router::new().route("/shell/run", get(|| async { "ok" })).layer(middleware::from_fn(localhost_only_shell_auth));

        let mut req = Request::builder()
            .uri("/shell/run")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 8], 5000))));

        let response = app.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn non_localhost_forwarded_for_is_forbidden_even_with_local_connect_info() {
        let app = Router::new().route("/shell/run", get(|| async { "ok" })).layer(middleware::from_fn(localhost_only_shell_auth));

        let mut req = Request::builder()
            .uri("/shell/run")
            .header("x-forwarded-for", "203.0.113.10")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));

        let response = app.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
