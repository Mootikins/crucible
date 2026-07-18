//! Daemon lifecycle utilities
//!
//! This module provides utilities for checking daemon status. The actual daemon
//! spawning is handled by `DaemonClient::start_daemon()`, and mutual exclusion
//! between daemons is enforced by the socket flock in `Server::bind`.

use std::path::{Path, PathBuf};
use tracing::debug;

pub fn default_socket_path() -> PathBuf {
    crucible_core::protocol::socket_path()
}

/// Check if daemon is running (socket exists and accepts connections)
pub fn is_daemon_running(socket: &Path) -> bool {
    if !socket.exists() {
        return false;
    }

    match std::os::unix::net::UnixStream::connect(socket) {
        Ok(_) => true,
        Err(e) => {
            debug!("Socket exists but connection failed: {}", e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_socket_path_ends_with_sock() {
        let path = default_socket_path();
        assert!(path.ends_with("crucible.sock"));
    }

    #[test]
    fn test_default_socket_path_is_absolute() {
        let path = default_socket_path();
        assert!(path.is_absolute());
    }

    #[test]
    fn test_is_daemon_running_false_when_no_socket() {
        let tmp = TempDir::new().unwrap();
        let socket = tmp.path().join("nonexistent.sock");
        assert!(!is_daemon_running(&socket));
    }

    #[test]
    fn test_is_daemon_running_false_when_socket_file_exists_but_not_listening() {
        let tmp = TempDir::new().unwrap();
        let socket = tmp.path().join("fake.sock");
        std::fs::write(&socket, "not a socket").unwrap();
        assert!(!is_daemon_running(&socket));
    }
}
