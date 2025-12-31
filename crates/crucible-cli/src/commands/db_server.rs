//! Internal database server command
//!
//! This module implements the hidden `db-server` subcommand that runs
//! SurrealDB as a socket server. It's not intended for direct user
//! invocation - it's spawned automatically when `storage.mode = "daemon"`.

use std::path::PathBuf;

use anyhow::Result;
use tracing::info;

use crate::config::CliConfig;

/// Execute the database server
///
/// This is a stub implementation. The full implementation will:
/// 1. Open SurrealDB with the configured kiln path
/// 2. Listen on Unix socket for client connections
/// 3. Handle JSON-RPC requests from clients
/// 4. Auto-shutdown after idle_timeout seconds of no connections
pub async fn execute(
    _config: CliConfig,
    socket: Option<PathBuf>,
    idle_timeout: u64,
) -> Result<()> {
    let socket_path = socket.unwrap_or_else(default_socket_path);

    info!(
        "db-server stub: would listen on {} with {}s idle timeout",
        socket_path.display(),
        idle_timeout
    );

    // TODO: Task 4 will implement the actual server:
    // - Create SurrealDB client
    // - Bind Unix socket
    // - Accept connections and handle JSON-RPC
    // - Track active connections for idle detection
    // - Graceful shutdown on SIGTERM or idle timeout

    Ok(())
}

/// Get the default socket path
///
/// Uses XDG_RUNTIME_DIR if available, otherwise falls back to /tmp
fn default_socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("crucible-db.sock")
    } else {
        PathBuf::from("/tmp/crucible-db.sock")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_socket_path_ends_with_sock() {
        // Don't manipulate env vars - just verify the path structure
        let path = default_socket_path();
        assert!(path.ends_with("crucible-db.sock"));
    }

    #[test]
    fn test_default_socket_path_is_absolute() {
        let path = default_socket_path();
        assert!(path.is_absolute());
    }
}
