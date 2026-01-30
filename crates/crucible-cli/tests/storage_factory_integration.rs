//! Integration tests for storage factory
//!
//! These tests verify that `get_storage` correctly connects to the daemon
//! when configured for daemon mode, preventing database lock errors.
//!
//! These tests modify `XDG_RUNTIME_DIR` and use `#[serial]` to prevent conflicts.

use anyhow::Result;
use crucible_cli::config::CliConfig;
use crucible_cli::factories::get_storage;
use crucible_config::StorageMode;
use crucible_daemon::Server;
use crucible_daemon_client::lifecycle;
use serial_test::serial;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

/// Test fixture that starts a real daemon server for integration testing.
///
/// Important: This sets XDG_RUNTIME_DIR to ensure `lifecycle::default_socket_path()`
/// returns the correct path. This must happen before calling `get_storage`.
struct TestServer {
    _temp_dir: TempDir,
    _server_handle: JoinHandle<()>,
    shutdown_handle: tokio::sync::broadcast::Sender<()>,
}

impl TestServer {
    /// Start a daemon at the path that `lifecycle::default_socket_path()` will return.
    ///
    /// This works by setting XDG_RUNTIME_DIR first, then binding the server at that path.
    async fn start() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;

        // Set XDG_RUNTIME_DIR BEFORE computing socket path
        // This ensures lifecycle::default_socket_path() returns the right path
        std::env::set_var("XDG_RUNTIME_DIR", temp_dir.path().to_str().unwrap());

        // Now get the path that get_storage will look for
        let socket_path = lifecycle::default_socket_path();

        let server = Server::bind(&socket_path, None).await?;
        let shutdown_handle = server.shutdown_handle();

        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Wait for server to be ready
        tokio::time::sleep(Duration::from_millis(50)).await;

        Ok(Self {
            _temp_dir: temp_dir,
            _server_handle: server_handle,
            shutdown_handle,
        })
    }

    async fn shutdown(self) {
        let _ = self.shutdown_handle.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Create a test config with daemon mode
fn create_daemon_config(kiln_path: PathBuf) -> CliConfig {
    let mut config = CliConfig {
        kiln_path,
        ..Default::default()
    };

    // Configure for daemon mode
    config.storage = Some(crucible_config::StorageConfig {
        mode: StorageMode::Daemon,
        ..Default::default()
    });

    config
}

/// Test that get_storage connects to running daemon in daemon mode
///
/// This is the key test that verifies the CLI correctly uses daemon storage
/// when configured, avoiding the database lock error that occurs when both
/// the daemon and CLI try to open the same database file directly.
#[tokio::test]
#[serial]
async fn test_get_storage_connects_to_daemon() {
    let server = TestServer::start().await.expect("Failed to start daemon");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let config = create_daemon_config(kiln_dir.path().to_path_buf());

    // get_storage should return daemon-backed storage
    let storage = get_storage(&config)
        .await
        .expect("get_storage should succeed when daemon is running");

    // Verify it's daemon mode
    assert!(
        storage.is_daemon(),
        "Storage should be daemon-backed when daemon is running"
    );

    // Verify we can actually query through it using a supported method
    let result = storage.list_notes(None).await;
    assert!(result.is_ok(), "list_notes through daemon should work");

    server.shutdown().await;
}

/// Test that StorageHandle works in daemon mode with multiple requests
#[tokio::test]
#[serial]
async fn test_storage_handle_query_through_daemon() {
    let server = TestServer::start().await.expect("Failed to start daemon");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let config = create_daemon_config(kiln_dir.path().to_path_buf());
    let storage = get_storage(&config).await.expect("get_storage failed");

    // Multiple requests should all work
    for i in 0..3 {
        let result = storage.list_notes(None).await;
        assert!(
            result.is_ok(),
            "Request {} should succeed: {:?}",
            i,
            result.err()
        );
    }

    server.shutdown().await;
}

/// Test that is_daemon() and is_embedded() return correct values
#[tokio::test]
#[serial]
async fn test_storage_handle_mode_detection() {
    let server = TestServer::start().await.expect("Failed to start daemon");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let config = create_daemon_config(kiln_dir.path().to_path_buf());
    let storage = get_storage(&config).await.expect("get_storage failed");

    assert!(storage.is_daemon(), "Should report as daemon mode");
    assert!(!storage.is_embedded(), "Should not report as embedded mode");
    assert!(
        storage.try_embedded().is_none(),
        "try_embedded should return None in daemon mode"
    );

    server.shutdown().await;
}

/// Test that get_storage fails gracefully when daemon is not running
/// (when configured for daemon mode but no daemon available)
#[tokio::test]
#[serial]
async fn test_get_storage_fails_when_no_daemon() {
    // Set up a temp dir with no daemon running
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    std::env::set_var("XDG_RUNTIME_DIR", temp_dir.path().to_str().unwrap());

    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");
    let mut config = CliConfig {
        kiln_path: kiln_dir.path().to_path_buf(),
        ..Default::default()
    };

    // Short timeout so test doesn't hang
    config.storage = Some(crucible_config::StorageConfig {
        mode: StorageMode::Daemon,
        idle_timeout_secs: 1,
    });

    // This should either:
    // 1. Spawn cru-server daemon and connect (if binary available)
    // 2. Fail with daemon connection error
    let result = get_storage(&config).await;

    // We expect this to fail in test environment since there's no real `cru` binary
    // to fork. The important thing is it doesn't panic.
    match result {
        Ok(_storage) => {
            // If it succeeded, that's fine too (daemon was somehow started)
        }
        Err(e) => {
            let err = e.to_string();
            assert!(
                err.contains("daemon") || err.contains("connect") || err.contains("socket"),
                "Error should be about daemon/connection, got: {}",
                err
            );
        }
    }
}

/// Test that multiple storage handles can connect to same daemon
#[tokio::test]
#[serial]
async fn test_multiple_storage_handles_same_daemon() {
    let server = TestServer::start().await.expect("Failed to start daemon");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let config = create_daemon_config(kiln_dir.path().to_path_buf());

    // Create multiple storage handles
    let storage1 = get_storage(&config).await.expect("storage1 failed");
    let storage2 = get_storage(&config).await.expect("storage2 failed");
    let storage3 = get_storage(&config).await.expect("storage3 failed");

    // All should be daemon mode
    assert!(storage1.is_daemon());
    assert!(storage2.is_daemon());
    assert!(storage3.is_daemon());

    // All should be able to list notes
    let r1 = storage1.list_notes(None).await;
    let r2 = storage2.list_notes(None).await;
    let r3 = storage3.list_notes(None).await;

    assert!(r1.is_ok(), "storage1 list_notes failed: {:?}", r1.err());
    assert!(r2.is_ok(), "storage2 list_notes failed: {:?}", r2.err());
    assert!(r3.is_ok(), "storage3 list_notes failed: {:?}", r3.err());

    server.shutdown().await;
}

/// Test that concurrent queries through daemon storage work
#[tokio::test]
#[serial]
async fn test_concurrent_queries_through_daemon() {
    let server = TestServer::start().await.expect("Failed to start daemon");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let config = create_daemon_config(kiln_dir.path().to_path_buf());
    let storage = get_storage(&config).await.expect("get_storage failed");

    // Spawn multiple concurrent requests
    let mut handles = vec![];
    for i in 0..5 {
        let s = storage.clone();
        let handle = tokio::spawn(async move {
            for j in 0..3 {
                let result = s.list_notes(None).await;
                assert!(
                    result.is_ok(),
                    "Request {}-{} failed: {:?}",
                    i,
                    j,
                    result.err()
                );
            }
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for (i, handle) in handles.into_iter().enumerate() {
        handle
            .await
            .unwrap_or_else(|e| panic!("Task {} panicked: {:?}", i, e));
    }

    server.shutdown().await;
}
