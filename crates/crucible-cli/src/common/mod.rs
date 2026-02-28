//! Common utilities and shared components for Crucible CLI

use anyhow::Context;
use crucible_rpc::DaemonClient;

/// Connect to the Crucible daemon, starting it if necessary.
///
/// Standardizes the error message across all CLI commands.
pub async fn daemon_client() -> anyhow::Result<DaemonClient> {
    DaemonClient::connect_or_start()
        .await
        .context("Failed to connect to daemon. Is it running? Try: cru daemon start")
}
