//! Common utilities and shared components for Crucible CLI

use anyhow::Context;
use crucible_daemon::DaemonClient;

/// Connect to the Crucible daemon, starting it if necessary.
///
/// Standardizes the error message across all CLI commands.
pub async fn daemon_client() -> anyhow::Result<DaemonClient> {
    DaemonClient::connect_or_start()
        .await
        .context("Failed to connect to daemon. Is it running? Try: cru daemon start")
}

/// Connect to the daemon only if one is already running — `None` otherwise.
///
/// For commands where daemon state is a nice-to-have (`cru plugin list`'s
/// runtime section): auto-spawning a daemon as a side effect of a read-only
/// listing would be a surprise.
pub async fn daemon_client_if_running() -> Option<DaemonClient> {
    DaemonClient::connect().await.ok()
}

/// Connect to the Crucible daemon with event streaming, starting it if necessary.
///
/// Standardizes the error message across all CLI commands.
pub async fn daemon_client_with_events() -> anyhow::Result<(
    DaemonClient,
    tokio::sync::mpsc::UnboundedReceiver<crucible_daemon::SessionEvent>,
)> {
    DaemonClient::connect_or_start_with_events()
        .await
        .context("Failed to connect to daemon. Is it running? Try: cru daemon start")
}
