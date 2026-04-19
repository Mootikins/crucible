//! Shared test fixture for rpc_integration tests.

use anyhow::Result;
use crucible_daemon::Server;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

/// Test fixture that starts a real daemon server for integration testing
pub struct TestServer {
    _temp_dir: TempDir,
    pub socket_path: PathBuf,
    _server_handle: JoinHandle<()>,
    shutdown_handle: tokio::sync::broadcast::Sender<()>,
}

impl TestServer {
    pub async fn start() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");

        let server = Server::bind(&socket_path, None).await?;
        let shutdown_handle = server.shutdown_handle();

        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Wait for server to be ready
        tokio::time::sleep(Duration::from_millis(50)).await;

        Ok(Self {
            _temp_dir: temp_dir,
            socket_path,
            _server_handle: server_handle,
            shutdown_handle,
        })
    }

    pub async fn shutdown(self) {
        let _ = self.shutdown_handle.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
