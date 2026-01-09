//! Daemon lifecycle: paths, socket management, shutdown

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Get the socket path for the daemon
pub fn socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("CRUCIBLE_DAEMON_SOCKET") {
        return PathBuf::from(path);
    }
    dirs::runtime_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("crucible")
        .join("daemon.sock")
}

/// Remove socket file if it exists
pub fn remove_socket(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Wait for SIGTERM or SIGINT signal
pub async fn wait_for_shutdown() -> Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;

        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM");
            }
            _ = sigint.recv() => {
                tracing::info!("Received SIGINT");
            }
        }
    }

    #[cfg(not(unix))]
    {
        // On non-Unix systems, just wait for Ctrl+C
        tokio::signal::ctrl_c().await?;
        tracing::info!("Received shutdown signal");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_socket_path_not_empty() {
        let path = socket_path();
        assert!(path.to_string_lossy().contains("crucible"));
    }

    #[test]
    fn test_remove_socket() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        // Create a dummy socket file
        fs::write(&sock_path, "").unwrap();
        assert!(sock_path.exists());

        remove_socket(&sock_path);
        assert!(!sock_path.exists());

        // Should not panic on nonexistent file
        remove_socket(&sock_path);
    }
}
