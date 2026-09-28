//! Integration tests for storage factory
//!
//! These tests verify that `get_storage` correctly connects to the daemon
//! when configured for daemon mode, preventing database lock errors.
//!
//! These tests modify `XDG_RUNTIME_DIR` and use `#[serial]` to prevent conflicts.

use anyhow::Result;
use crucible_cli::config::CliAppConfig;
use crucible_cli::factories::get_storage;
use crucible_core::test_support::EnvVarGuard;
use crucible_daemon::test_support::{InProcessDaemon, InProcessDaemonBuilder};
use serial_test::serial;
use std::path::PathBuf;
use std::time::Duration;

/// Start a daemon at the path `lifecycle::default_socket_path()` will
/// return, by setting `XDG_RUNTIME_DIR` before binding — the same path
/// `get_storage` resolves to when it looks for a running daemon.
async fn start_server() -> Result<InProcessDaemon> {
    InProcessDaemonBuilder::new()?
        .using_xdg_runtime_socket()
        .with_ready_timeout(Duration::from_secs(2))
        .start()
        .await
}

/// Create a test config (daemon mode is always used)
fn create_daemon_config(kiln_path: PathBuf) -> CliAppConfig {
    CliAppConfig {
        kiln_path,
        ..Default::default()
    }
}

/// Test that get_storage connects to running daemon in daemon mode
///
/// This is the key test that verifies the CLI correctly uses daemon storage
/// when configured, avoiding the database lock error that occurs when both
/// the daemon and CLI try to open the same database file directly.
#[tokio::test]
#[serial]
async fn test_get_storage_connects_to_daemon() {
    let server = start_server().await.expect("Failed to start daemon");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let config = create_daemon_config(kiln_dir.path().to_path_buf());

    // get_storage should return daemon-backed storage
    let storage = get_storage(&config)
        .await
        .expect("get_storage should succeed when daemon is running");

    // Verify we can actually query through it using a supported method

    // Verify we can actually query through it using a supported method
    let result = storage.list_notes(None).await;
    assert!(result.is_ok(), "list_notes through daemon should work");

    server.shutdown().await;
}

/// Test that CliStorageHandle works in daemon mode with multiple requests
#[tokio::test]
#[serial]
async fn test_storage_handle_query_through_daemon() {
    let server = start_server().await.expect("Failed to start daemon");
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

/// Test that CliStorageHandle provides access to daemon client
#[tokio::test]
#[serial]
async fn test_storage_handle_mode_detection() {
    let server = start_server().await.expect("Failed to start daemon");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");
    let config = create_daemon_config(kiln_dir.path().to_path_buf());
    let storage = get_storage(&config).await.expect("get_storage failed");
    let _ = storage.as_daemon_client(); // Confirms we can access the inner client
    server.shutdown().await;
}

/// Test that get_storage fails gracefully when daemon is not running
/// (when configured for daemon mode but no daemon available)
#[tokio::test]
#[serial]
async fn test_get_storage_fails_when_no_daemon() {
    // Set up a temp dir with no daemon running
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let _guard = EnvVarGuard::set(
        "XDG_RUNTIME_DIR",
        temp_dir.path().to_str().unwrap().to_string(),
    );

    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");
    let config = CliAppConfig {
        kiln_path: kiln_dir.path().to_path_buf(),
        ..Default::default()
    };

    // This should either:
    // 1. Spawn cru daemon serve and connect (if binary available)
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
    let server = start_server().await.expect("Failed to start daemon");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let config = create_daemon_config(kiln_dir.path().to_path_buf());

    // Create multiple storage handles
    let storage1 = get_storage(&config).await.expect("storage1 failed");
    let storage2 = get_storage(&config).await.expect("storage2 failed");
    let storage3 = get_storage(&config).await.expect("storage3 failed");

    // All should be able to list notes

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
    let server = start_server().await.expect("Failed to start daemon");
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
