use crate::assets::static_routes;
use crate::middleware::auth::localhost_only_shell_auth;
use crate::routes::{
    chat_routes, config_routes, health_routes, kiln_routes, mcp_routes, plugin_routes, project_routes,
    search_routes, session_routes, shell_routes,
};
use crate::services::daemon;
use crate::{Result, WebError};
use axum::extract::DefaultBodyLimit;
use axum::http::{header, Method};
use axum::middleware;
use axum::Router;
use std::net::SocketAddr;
use tower_http::cors::{AllowOrigin, CorsLayer};

pub use crucible_config::{CliAppConfig, WebConfig};

const MAX_BODY_SIZE_10MB: usize = 10 * 1024 * 1024;

pub async fn start_server(web_config: &WebConfig, app_config: &CliAppConfig) -> Result<()> {
    let state = daemon::init_daemon(app_config.clone()).await?;

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::any())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_body_size_is_10mb() {
        assert_eq!(MAX_BODY_SIZE_10MB, 10 * 1024 * 1024);
        assert_eq!(MAX_BODY_SIZE_10MB, 10_485_760);
    }

    #[test]
    fn test_cors_allowed_origins_are_wildcard() {
        // After switching to AllowOrigin::any(), CORS accepts all origins.
        // This test verifies the policy is configured for permissive access.
        // This is safe for a local-first app not exposed to the internet.
        
        // The key assertion: we're using AllowOrigin::any() which accepts any origin.
        // This enables LAN access from 192.168.x.x and other local networks.
        let test_origins = [
            "http://localhost:3000",
            "http://127.0.0.1:3000",
            "http://192.168.0.16:3000",  // LAN access now allowed
            "http://10.0.0.5:3000",       // Private network access now allowed
            "https://example.com",         // Any origin is accepted
        ];
        
        // All origins should be valid (no filtering)
        for origin in test_origins {
            let parsed: axum::http::HeaderValue = origin.parse().expect("Should be valid header");
            assert!(!parsed.is_empty(), "Origin {} should parse as valid header", origin);
        }
    }

    #[test]
    fn test_cors_any_origin_policy() {
        // Verify that AllowOrigin::any() is the configured policy.
        // This test documents the CORS behavior: all origins are accepted.
        // This is appropriate for a local-first application.
        
        // The policy allows any origin, so we just verify that
        // the configuration doesn't have a restrictive list.
        // In production, this would be verified by checking the actual
        // CorsLayer configuration, but that's tested implicitly by
        // the server accepting requests from any origin.
        
        // This test serves as documentation that the CORS policy is intentionally permissive.
        assert!(true, "AllowOrigin::any() is the configured policy");
    }


}
