mod acp_handle;
mod agent_factory;
mod agent_manager;
mod background_manager;
mod daemon_plugins;
mod embedding;
mod event_emitter;
mod file_watch_bridge;
mod kiln_manager;
mod llm_hooks;
mod multi_kiln_search;
mod lifecycle;
mod permission_bridge;
mod precognition;
mod project_manager;
mod protocol;
mod recording;
#[allow(dead_code)]
mod replay;
mod rpc;
mod rpc_helpers;
mod server;
mod session_bridge;
mod session_manager;
mod session_storage;
mod subscription;
mod tools_bridge;
mod trust_resolution;

use anyhow::Result;
use crucible_config::CliAppConfig;
use lifecycle::{remove_socket, socket_path, wait_for_shutdown};
use scopeguard::defer;
use server::Server;

#[tokio::main]
async fn main() -> Result<()> {
    // Install ring as the rustls CryptoProvider before any TLS usage.
    // Both ring and aws-lc-rs are compiled (via lancedb), so rustls can't auto-detect.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls CryptoProvider");

    tracing_subscriber::fmt::init();
    tracing::info!("cru-server starting");

    let sock_path = socket_path();

    let config = CliAppConfig::load(None, None, None).unwrap_or_default();
    let mcp_config = config.mcp.as_ref();
    let plugin_watch = config
        .plugins
        .get("watch")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let plugin_config = config.plugins.clone();
    let llm_config = if config.llm.has_providers() {
        Some(config.llm)
    } else {
        None
    };
    let permission_config = config.permissions.clone();
    let web_config = config.web;

    defer! {
        tracing::info!("Cleaning up daemon resources");
        remove_socket(&sock_path);
    }

    let server = Server::bind_with_plugin_config(
        &sock_path,
        mcp_config,
        plugin_config,
        plugin_watch,
        llm_config,
        permission_config,
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
