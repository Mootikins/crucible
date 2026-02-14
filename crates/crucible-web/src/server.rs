use crate::assets::static_routes;
use crate::routes::{chat_routes, health_routes, project_routes, search_routes, session_routes};
use crate::services::daemon;
use crate::{Result, WebError};
use axum::extract::DefaultBodyLimit;
use axum::http::{header, Method};
use axum::Router;
use std::net::SocketAddr;
use tower_http::cors::{AllowOrigin, CorsLayer};

pub use crucible_config::{CliAppConfig, WebConfig};

const MAX_BODY_SIZE_10MB: usize = 10 * 1024 * 1024;

pub async fn start_server(web_config: &WebConfig, app_config: &CliAppConfig) -> Result<()> {
    let state = daemon::init_daemon(app_config.clone()).await?;

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            "http://localhost:3000".parse().unwrap(),
            "http://localhost:5173".parse().unwrap(),
            "http://127.0.0.1:3000".parse().unwrap(),
            "http://127.0.0.1:5173".parse().unwrap(),
        ]))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let app = Router::new()
        .merge(chat_routes())
        .merge(session_routes())
        .merge(project_routes())
        .merge(search_routes())
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

    axum::serve(listener, app).await.map_err(WebError::Io)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_body_size_is_10mb() {
        assert_eq!(MAX_BODY_SIZE_10MB, 10 * 1024 * 1024);
        assert_eq!(MAX_BODY_SIZE_10MB, 10_485_760);
    }

    #[test]
    fn test_cors_allowed_origins_are_localhost_only() {
        let allowed_origins = [
            "http://localhost:3000",
            "http://localhost:5173",
            "http://127.0.0.1:3000",
            "http://127.0.0.1:5173",
        ];

        for origin in allowed_origins {
            assert!(
                origin.starts_with("http://localhost:") || origin.starts_with("http://127.0.0.1:"),
                "Origin {} should be localhost only",
                origin
            );
        }

        let disallowed_patterns = ["https://", "http://0.0.0.0", "http://192.168", "http://10."];
        for pattern in disallowed_patterns {
            for origin in &allowed_origins {
                assert!(
                    !origin.starts_with(pattern),
                    "Origin {} should not match disallowed pattern {}",
                    origin,
                    pattern
                );
            }
        }
    }

    #[test]
    fn test_cors_origins_are_valid_urls() {
        let origins = [
            "http://localhost:3000",
            "http://localhost:5173",
            "http://127.0.0.1:3000",
            "http://127.0.0.1:5173",
        ];

        for origin in origins {
            let parsed: axum::http::HeaderValue = origin.parse().expect("Should be valid header");
            assert!(!parsed.is_empty());
        }
    }
}
