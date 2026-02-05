use crate::assets::static_routes;
use crate::routes::{chat_routes, health_routes, search_routes, session_routes};
use crate::services::daemon;
use crate::{Result, WebError};
use axum::Router;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

#[derive(Debug, Clone)]
pub struct WebConfig {
    pub port: u16,
    pub host: String,
    pub web_dir: Option<String>,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            port: 3000,
            host: "127.0.0.1".to_string(),
            web_dir: None,
        }
    }
}

impl WebConfig {
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    pub fn with_web_dir(mut self, dir: impl Into<String>) -> Self {
        self.web_dir = Some(dir.into());
        self
    }
}

pub async fn start_server(config: WebConfig) -> Result<()> {
    let state = daemon::init_daemon().await?;

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(chat_routes())
        .merge(session_routes())
        .merge(search_routes())
        .with_state(state)
        .merge(health_routes())
        .merge(static_routes(config.web_dir.as_deref()))
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
