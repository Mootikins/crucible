pub use crucible_protocol::{remove_socket, socket_path};

use anyhow::Result;

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
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_socket_path_not_empty() {
        let path = socket_path();
        assert!(path.to_string_lossy().contains("crucible.sock"));
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
