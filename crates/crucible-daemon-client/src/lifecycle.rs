//! Daemon lifecycle management for single-binary pattern
//!
//! This module handles forking the `cru` binary as a db-server and
//! ensuring it's running before connecting.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Get the default socket path for db-server
///
/// Uses XDG_RUNTIME_DIR if available, otherwise falls back to /tmp.
/// This matches the default path used by `cru db-server`.
pub fn default_socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("crucible-db.sock")
    } else {
        PathBuf::from("/tmp/crucible-db.sock")
    }
}

/// Check if daemon is running (socket exists and accepts connections)
///
/// Unlike the PID-based check in crucible-daemon, this verifies the socket
/// is actually connectable, which is more reliable for our use case.
pub fn is_daemon_running(socket: &Path) -> bool {
    if !socket.exists() {
        return false;
    }

    // Try to connect to verify the socket is alive
    match std::os::unix::net::UnixStream::connect(socket) {
        Ok(_) => true,
        Err(e) => {
            debug!("Socket exists but connection failed: {}", e);
            false
        }
    }
}

/// Fork self as db-server daemon
///
/// Spawns the current executable with `db-server` subcommand.
/// The spawned process is detached and runs in the background.
pub fn fork_daemon(socket: &Path, idle_timeout: u64) -> Result<()> {
    let exe = std::env::current_exe().context("Failed to get current executable path")?;

    debug!(
        "Forking db-server: {} db-server --socket {:?} --idle-timeout {}",
        exe.display(),
        socket,
        idle_timeout
    );

    Command::new(&exe)
        .arg("db-server")
        .arg("--socket")
        .arg(socket)
        .arg("--idle-timeout")
        .arg(idle_timeout.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Failed to spawn db-server process")?;

    info!("Forked db-server process");
    Ok(())
}

/// Ensure daemon is running, forking if needed
///
/// If the daemon is already running, returns immediately.
/// Otherwise, forks a new daemon and waits for the socket with exponential backoff.
pub async fn ensure_daemon(socket: &Path, idle_timeout: u64) -> Result<()> {
    if is_daemon_running(socket) {
        debug!("Daemon already running at {:?}", socket);
        return Ok(());
    }

    info!("Daemon not running, forking new instance");
    fork_daemon(socket, idle_timeout)?;

    // Wait for socket with exponential backoff
    let mut delay = Duration::from_millis(50);
    let max_attempts = 10;

    for attempt in 0..max_attempts {
        tokio::time::sleep(delay).await;

        if is_daemon_running(socket) {
            info!("Daemon ready after {} attempts", attempt + 1);
            return Ok(());
        }

        delay = std::cmp::min(delay * 2, Duration::from_secs(1));

        if attempt > 5 {
            warn!(
                "Daemon not ready after {} attempts, retrying...",
                attempt + 1
            );
        }
    }

    anyhow::bail!(
        "Failed to start db-server daemon after {} attempts. \
         Check logs at ~/.crucible/db-server.log",
        max_attempts
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_socket_path_ends_with_sock() {
        let path = default_socket_path();
        assert!(path.ends_with("crucible-db.sock"));
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

        // Create a regular file (not a socket)
        std::fs::write(&socket, "not a socket").unwrap();

        // Should return false because it's not a real socket
        assert!(!is_daemon_running(&socket));
    }

    // Note: We don't test fork_daemon or ensure_daemon here because they require
    // actually running the binary. Those are tested in integration tests.
}
