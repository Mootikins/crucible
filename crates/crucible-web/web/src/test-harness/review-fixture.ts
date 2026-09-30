/** Shared two-hunk Rust example for the real review preview and browser tests. */
export const SERVER_BASE = `use std::net::SocketAddr;
use std::time::Duration;

use axum::{routing::get, Router};
use tokio::net::TcpListener;

/// The settings of one server.
pub struct Config {
    pub addr: SocketAddr,
    pub timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            timeout: Duration::from_secs(30),
        }
    }
}

/// Builds the routes of the server.
fn routes() -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/version", get(|| async { env!("CARGO_PKG_VERSION") }))
}

/// Serves the routes until the process stops.
pub async fn serve(config: Config) -> std::io::Result<()> {
    let listener = TcpListener::bind(config.addr).await?;
    axum::serve(listener, routes()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_port_is_3000() {
        assert_eq!(Config::default().addr.port(), 3000);
    }
}
`;

export const SERVER_CURRENT = `use std::net::SocketAddr;
use std::time::Duration;

use axum::{routing::get, Router};
use tokio::net::TcpListener;

/// The settings of one server.
pub struct Config {
    pub addr: SocketAddr,
    pub timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            timeout: Duration::from_secs(10),
        }
    }
}

/// Builds the routes of the server.
fn routes() -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/version", get(|| async { env!("CARGO_PKG_VERSION") }))
}

/// Serves the routes until the process stops.
pub async fn serve(config: Config) -> std::io::Result<()> {
    let listener = TcpListener::bind(config.addr).await?;
    tracing::info!(addr = %config.addr, "the server listens");
    axum::serve(listener, routes())
        .with_graceful_shutdown(shutdown(config.timeout))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_port_is_3000() {
        assert_eq!(Config::default().addr.port(), 3000);
    }
}
`;
