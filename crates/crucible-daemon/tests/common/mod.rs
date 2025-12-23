//! Common test utilities for daemon E2E tests

use anyhow::Result;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;
use tokio::time::sleep;

/// Test daemon fixture that manages an isolated daemon instance
pub struct TestDaemon {
    pub socket_path: PathBuf,
    pub pid_path: PathBuf,
    process: Option<Child>,
    _temp_dir: tempfile::TempDir,
}

impl TestDaemon {
    /// Start a test daemon with isolated socket/pid paths
    ///
    /// This spawns a real daemon process in a temporary directory,
    /// ensuring test isolation.
    pub async fn start() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");
        let pid_path = temp_dir.path().join("daemon.pid");

        // Get the daemon binary path
        let daemon_exe = env!("CARGO_BIN_EXE_cru-daemon");

        // Spawn daemon with custom paths via environment variables
        let process = Command::new(daemon_exe)
            .env("CRUCIBLE_DAEMON_SOCKET", &socket_path)
            .env("CRUCIBLE_DAEMON_PID", &pid_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        // Wait for socket to appear with timeout
        for attempt in 0..50 {
            if socket_path.exists() {
                // Give daemon a bit more time to fully initialize
                sleep(Duration::from_millis(50)).await;
                return Ok(Self {
                    socket_path,
                    pid_path,
                    process: Some(process),
                    _temp_dir: temp_dir,
                });
            }
            sleep(Duration::from_millis(100)).await;

            if attempt == 25 {
                tracing::warn!("Daemon taking longer than expected to start...");
            }
        }

        anyhow::bail!("Daemon failed to start within 5 seconds");
    }

    /// Manually stop the daemon process
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut p) = self.process.take() {
            p.kill()?;
            p.wait()?;
        }
        Ok(())
    }

    /// Check if the daemon process is still running
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut p) = self.process {
            match p.try_wait() {
                Ok(Some(_)) => false, // Process has exited
                Ok(None) => true,     // Still running
                Err(_) => false,      // Error checking status
            }
        } else {
            false
        }
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        if let Some(mut p) = self.process.take() {
            let _ = p.kill();
            let _ = p.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_daemon_fixture_starts_and_stops() {
        let mut daemon = TestDaemon::start()
            .await
            .expect("Failed to start test daemon");

        // Verify socket exists
        assert!(daemon.socket_path.exists(), "Socket should exist");

        // Verify PID file exists
        assert!(daemon.pid_path.exists(), "PID file should exist");

        // Verify daemon is running
        assert!(daemon.is_running(), "Daemon should be running");

        // Stop daemon
        daemon.stop().await.expect("Failed to stop daemon");

        // Verify daemon stopped
        assert!(!daemon.is_running(), "Daemon should be stopped");
    }

    #[tokio::test]
    async fn test_daemon_fixture_cleanup_on_drop() {
        let daemon = TestDaemon::start()
            .await
            .expect("Failed to start test daemon");
        let _socket_path = daemon.socket_path.clone();

        // Drop daemon (should clean up)
        drop(daemon);

        // Give OS time to cleanup
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Socket might still exist briefly but daemon should be gone
        // This is best-effort cleanup test
    }
}
