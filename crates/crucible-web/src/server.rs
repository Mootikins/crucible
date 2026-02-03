//! Axum server configuration and startup

use crate::assets::static_routes;
use crate::routes::{chat_routes, health_routes};
use crate::services::ChatService;
use crate::{Result, WebError};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Configuration for the web server
#[derive(Debug, Clone)]
pub struct WebConfig {
    /// Port to listen on
    pub port: u16,
    /// Host to bind to
    pub host: String,
    /// Optional override for static asset directory
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

/// Start the web server
pub async fn start_server(config: WebConfig) -> Result<()> {
    let chat_config = crate::services::chat::ChatServiceConfig {
        channel_buffer: 100,
    };
    let chat_service = Arc::new(ChatService::new(chat_config));

    // Initialize session
    chat_service.initialize().await?;

    // Build CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router
    let app = Router::new()
        .merge(health_routes())
        .merge(chat_routes(chat_service))
        .merge(static_routes(config.web_dir.as_deref()))
        .layer(cors);

    // Parse address
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|e| WebError::Config(format!("Invalid address: {}", e)))?;

    tracing::info!("Starting web server on http://{}", addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(WebError::Io)?;

    axum::serve(listener, app).await.map_err(WebError::Io)?;

    Ok(())
}
