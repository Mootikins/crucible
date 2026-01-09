mod kiln_manager;
mod lifecycle;
mod protocol;
mod rpc_helpers;
mod server;
mod session_manager;
mod session_storage;

#[cfg(feature = "subscriptions")]
mod subscription;

use anyhow::Result;
use lifecycle::{remove_socket, socket_path, wait_for_shutdown};
use scopeguard::defer;
use server::Server;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("cru-server starting");

    let sock_path = socket_path();

    // Setup cleanup on exit
    defer! {
        tracing::info!("Cleaning up daemon resources");
        remove_socket(&sock_path);
    }

    // Bind server - will fail if socket already exists and is in use
    let server = Server::bind(&sock_path).await?;
    tracing::info!("Daemon started successfully");

    // Run server until shutdown
    tokio::select! {
        result = server.run() => {
            if let Err(e) = result {
                tracing::error!("Server error: {}", e);
            }
        }
        result = wait_for_shutdown() => {
            if let Err(e) = result {
                tracing::error!("Signal handler error: {}", e);
            }
            tracing::info!("Shutdown signal received");
        }
    }

    tracing::info!("cru-server shutting down");
    Ok(())
}
