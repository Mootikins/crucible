//! Common test utilities for daemon E2E tests

use anyhow::Result;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;
use tokio::time::sleep;

/// Test daemon fixture that manages an isolated daemon instance
pub struct TestDaemon {
    pub socket_path: PathBuf,
    process: Option<Child>,
    /// Held to keep temp directory alive for duration of test
    #[allow(dead_code)]
    temp_dir: tempfile::TempDir,
}

impl TestDaemon {
    /// Start a test daemon with isolated socket path
    ///
    /// This spawns a real daemon process in a temporary directory,
    /// ensuring test isolation.
    pub async fn start() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");

        // Single binary: daemon runs via `cru daemon serve`.
        // Cannot use env!("CARGO_BIN_EXE_cru") — crucible-daemon cannot depend on crucible-cli
        // (circular: crucible-cli -> crucible-daemon). Locate `cru` at runtime instead.
        let cru_exe = std::env::var("CARGO_BIN_EXE_cru").unwrap_or_else(|_| {
            let test_exe = std::env::current_exe().expect("current_exe");
            let target_dir = test_exe
                .parent() // deps/
                .and_then(|p| p.parent()) // debug/ or release/
                .expect("target dir");
            target_dir.join("cru").to_string_lossy().to_string()
        });
        // Spawn daemon with custom socket path via environment variable
        let process = Command::new(&cru_exe)
            .args(["daemon", "serve"])
            .env("CRUCIBLE_SOCKET", &socket_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        // Wait for socket to appear with timeout
        for attempt in 0..100 {
            if socket_path.exists() {
                // Give daemon a bit more time to fully initialize
                sleep(Duration::from_millis(50)).await;
                return Ok(Self {
                    socket_path,
                    process: Some(process),
                    temp_dir,
                });
            }
            sleep(Duration::from_millis(100)).await;

            if attempt == 50 {
                tracing::warn!("Daemon taking longer than expected to start...");
            }
        }

        anyhow::bail!("Daemon failed to start within 10 seconds");
    }

    /// Start a test daemon with additional environment variables.
    pub async fn start_with_env(env_vars: Vec<(&str, &str)>) -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");

        let cru_exe = std::env::var("CARGO_BIN_EXE_cru").unwrap_or_else(|_| {
            let test_exe = std::env::current_exe().expect("current_exe");
            let target_dir = test_exe
                .parent()
                .and_then(|p| p.parent())
                .expect("target dir");
            target_dir.join("cru").to_string_lossy().to_string()
        });

        let mut cmd = Command::new(&cru_exe);
        cmd.args(["daemon", "serve"])
            .env("CRUCIBLE_SOCKET", &socket_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        for (key, value) in &env_vars {
            cmd.env(key, value);
        }

        let process = cmd.spawn()?;

        for attempt in 0..100 {
            if socket_path.exists() {
                sleep(Duration::from_millis(50)).await;
                return Ok(Self {
                    socket_path,
                    process: Some(process),
                    temp_dir,
                });
            }
            sleep(Duration::from_millis(100)).await;

            if attempt == 50 {
                tracing::warn!("Daemon taking longer than expected to start...");
            }
        }

        anyhow::bail!("Daemon failed to start within 10 seconds");
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

// Re-export canonical test mocks for integration tests
pub use crucible_daemon::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};
