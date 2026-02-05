mod acp_handle;
mod agent_factory;
mod agent_manager;
mod background_manager;
mod daemon_plugins;
mod file_watch_bridge;
mod kiln_manager;
mod lifecycle;
mod permission_bridge;
mod project_manager;
mod protocol;
mod rpc;
mod rpc_helpers;
mod server;
mod session_bridge;
mod session_manager;
mod session_storage;
mod subscription;
mod tools_bridge;

use anyhow::Result;
use crucible_config::CliAppConfig;
use lifecycle::{remove_socket, socket_path, wait_for_shutdown};
use scopeguard::defer;
use server::Server;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("cru-server starting");

    let sock_path = socket_path();

    let config = CliAppConfig::load(None, None, None).unwrap_or_default();
    let mcp_config = config.mcp.as_ref();
    let plugin_config = config.plugins.clone();
    let providers_config = config.providers.clone();
    let web_config = config.web.clone();

    defer! {
        tracing::info!("Cleaning up daemon resources");
        remove_socket(&sock_path);
    }

    let server = Server::bind_with_plugin_config(
        &sock_path,
        mcp_config,
        plugin_config,
        providers_config,
        web_config,
    )
    .await?;
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
