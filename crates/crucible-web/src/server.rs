use crate::assets::static_routes;
use crate::routes::{chat_routes, health_routes, project_routes, search_routes, session_routes};
use crate::services::daemon;
use crate::{Result, WebError};
use axum::extract::DefaultBodyLimit;
use axum::Router;
use axum::http::{header, Method};
use std::net::SocketAddr;
use tower_http::cors::{AllowOrigin, CorsLayer};

pub use crucible_config::WebConfig;

const MAX_BODY_SIZE_10MB: usize = 10 * 1024 * 1024;

pub async fn start_server(config: &WebConfig) -> Result<()> {
    let state = daemon::init_daemon().await?;

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
        .merge(static_routes(config.static_dir.as_deref()))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE_10MB))
        .layer(cors);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|e| WebError::Config(format!("Invalid address: {e}")))?;

    tracing::info!("Starting web server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(WebError::Io)?;

    axum::serve(listener, app).await.map_err(WebError::Io)?;

    Ok(())
}
