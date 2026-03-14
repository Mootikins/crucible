use crate::assets::static_routes;
use crate::middleware::auth::localhost_only_shell_auth;
use crate::routes::{
    chat_routes, config_routes, health_routes, kiln_routes, mcp_routes, plugin_routes,
    project_routes, search_routes, session_routes, shell_routes,
};
use crate::services::daemon;
use crate::{Result, WebError};
use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Method};
use axum::middleware;
use axum::Router;
use std::net::SocketAddr;
use tower_http::cors::{AllowOrigin, CorsLayer};

pub use crucible_config::{CliAppConfig, WebConfig};

const MAX_BODY_SIZE_10MB: usize = 10 * 1024 * 1024;

pub async fn start_server(web_config: &WebConfig, app_config: &CliAppConfig) -> Result<()> {
    let state = daemon::init_daemon(app_config.clone()).await?;

    // Wildcard CORS is dangerous here because `/api/shell/exec` can execute host shell commands.
    // Restricting origins prevents arbitrary websites from triggering command execution via browsers.
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(build_cors_origins(web_config)))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let app = Router::new()
        .nest(
            "/api/shell",
            shell_routes().layer(middleware::from_fn(localhost_only_shell_auth)),
        )
        .merge(chat_routes())
        .merge(config_routes())
        .merge(session_routes())
        .merge(project_routes())
        .merge(search_routes())
        .merge(plugin_routes())
        .merge(mcp_routes())
        .merge(kiln_routes())
        .with_state(state)
        .merge(health_routes())
        .merge(static_routes(web_config.static_dir.as_deref()))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE_10MB))
        .layer(cors);

    let addr: SocketAddr = format!("{}:{}", web_config.host, web_config.port)
        .parse()
        .map_err(|e| WebError::Config(format!("Invalid address: {e}")))?;

    tracing::info!("Starting web server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(WebError::Io)?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(WebError::Io)?;

    Ok(())
}

fn build_cors_origins(web_config: &WebConfig) -> Vec<HeaderValue> {
    let mut origins = Vec::new();

    let mut add_origin = |origin: &str| {
        let Ok(value) = HeaderValue::from_str(origin) else {
            tracing::warn!(origin, "Skipping invalid CORS origin");
            return;
        };

        if !origins.iter().any(|existing| existing == &value) {
            origins.push(value);
        }
    };

    add_origin(&format!("http://{}:{}", web_config.host, web_config.port));
    add_origin(&format!("http://127.0.0.1:{}", web_config.port));

    if cfg!(debug_assertions) {
        add_origin("http://localhost:5173");
    }

    if let Ok(extra_origins) = std::env::var("CRUCIBLE_CORS_ORIGINS") {
        for origin in extra_origins.split(',').map(str::trim).filter(|o| !o.is_empty()) {
            add_origin(origin);
        }
    }

    origins
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use tower::ServiceExt;

    #[test]
    fn test_max_body_size_is_10mb() {
        assert_eq!(MAX_BODY_SIZE_10MB, 10 * 1024 * 1024);
        assert_eq!(MAX_BODY_SIZE_10MB, 10_485_760);
    }

    #[test]
    fn build_cors_origins_includes_expected_defaults() {
        let web_config = WebConfig {
            enabled: true,
            host: "localhost".to_string(),
            port: 3000,
            static_dir: None,
        };

        let origins = build_cors_origins(&web_config);
        let has_origin = |value: &str| {
            let value = HeaderValue::from_str(value).unwrap();
            origins.iter().any(|origin| origin == &value)
        };

        assert!(has_origin("http://localhost:3000"));
        assert!(has_origin("http://127.0.0.1:3000"));
        if cfg!(debug_assertions) {
            assert!(has_origin("http://localhost:5173"));
        }
    }

    #[tokio::test]
    async fn cors_rejects_evil_origin_and_allows_localhost_origin() {
        let web_config = WebConfig {
            enabled: true,
            host: "localhost".to_string(),
            port: 3000,
            static_dir: None,
        };

        let cors = CorsLayer::new()
            .allow_origin(AllowOrigin::list(build_cors_origins(&web_config)))
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

        let app = Router::new()
            .route("/api/test", get(|| async { "ok" }))
            .layer(cors);

        let disallowed_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/test")
                    .header(header::ORIGIN, "https://evil.com")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(
            disallowed_response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );

        let allowed_response = app
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/test")
                    .header(header::ORIGIN, "http://localhost:3000")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            allowed_response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&HeaderValue::from_static("http://localhost:3000"))
        );
    }
}
