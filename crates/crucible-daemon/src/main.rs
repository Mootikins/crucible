mod kiln_manager;
mod lifecycle;
mod protocol;
mod server;
mod session_manager;
mod session_storage;
mod subscription;

use anyhow::{bail, Result};
use lifecycle::{
    is_daemon_running, pid_path, remove_pid_file, remove_socket, socket_path, wait_for_shutdown,
    write_pid_file,
};
use scopeguard::defer;
use server::Server;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("cru-daemon starting");

    // Check if daemon is already running
    if is_daemon_running() {
        bail!("Daemon is already running");
    }

    let sock_path = socket_path();
    let pid_file = pid_path();

    // Write PID file
    write_pid_file(&pid_file)?;

    // Setup cleanup on exit
    defer! {
        tracing::info!("Cleaning up daemon resources");
        remove_pid_file(&pid_file);
        remove_socket(&sock_path);
    }

    // Bind server
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

    tracing::info!("cru-daemon shutting down");
    Ok(())
}
