//! End-to-end tests for the daemon
//!
//! These tests verify the daemon works correctly as a whole system,
//! testing the full request/response cycle with real processes.

mod common;

use common::TestDaemon;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Test basic daemon lifecycle: start, connect, ping, shutdown
#[tokio::test]
async fn test_e2e_daemon_lifecycle() {
    let mut daemon = TestDaemon::start().await.expect("Failed to start daemon");

    // Verify daemon is running
    assert!(daemon.is_running(), "Daemon should be running");

    // Connect client
    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect to daemon");

    // Send ping
    stream
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
        .await
        .expect("Failed to write ping");

    // Read response
    let mut buf = vec![0u8; 1024];
    let n = stream
        .read(&mut buf)
        .await
        .expect("Failed to read response");
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(
        response.contains("\"result\":\"pong\""),
        "Expected pong response"
    );
    assert!(
        response.contains("\"id\":1"),
        "Expected matching request ID"
    );

    // Send shutdown via RPC
    stream
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"shutdown\"}\n")
        .await
        .expect("Failed to write shutdown");

    // Read shutdown response
    let n = stream
        .read(&mut buf)
        .await
        .expect("Failed to read shutdown response");
    let response = String::from_utf8_lossy(&buf[..n]);
    assert!(
        response.contains("shutting down"),
        "Expected shutdown confirmation"
    );

    // Wait for daemon to exit
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify daemon stopped
    assert!(!daemon.is_running(), "Daemon should have stopped");

    daemon.stop().await.expect("Failed to cleanup daemon");
}

/// Test that multiple sequential requests work correctly
#[tokio::test]
async fn test_e2e_multiple_requests() {
    let mut daemon = TestDaemon::start().await.expect("Failed to start daemon");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect");

    // Use BufReader for line-by-line responses
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Send 5 ping requests
    for i in 1..=5 {
        let request = format!("{{\"jsonrpc\":\"2.0\",\"id\":{},\"method\":\"ping\"}}\n", i);
        writer
            .write_all(request.as_bytes())
            .await
            .expect("Failed to write request");

        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .expect("Failed to read response");

        assert!(
            line.contains("\"result\":\"pong\""),
            "Request {} should get pong",
            i
        );
        assert!(
            line.contains(&format!("\"id\":{}", i)),
            "Request {} should have matching ID",
            i
        );
    }

    daemon.stop().await.expect("Failed to stop daemon");
}

/// Test concurrent clients connecting to the same daemon
#[tokio::test]
async fn test_e2e_concurrent_clients() {
    let daemon = TestDaemon::start().await.expect("Failed to start daemon");

    let socket_path = daemon.socket_path.clone();

    // Spawn 10 concurrent clients
    let mut handles = vec![];
    for client_id in 0..10 {
        let socket = socket_path.clone();
        let handle = tokio::spawn(async move {
            // Connect
            let mut stream = UnixStream::connect(&socket)
                .await
                .expect("Client failed to connect");

            // Send ping
            let request = format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":{},\"method\":\"ping\"}}\n",
                client_id
            );
            stream
                .write_all(request.as_bytes())
                .await
                .expect("Failed to write");

            // Read response
            let mut buf = vec![0u8; 1024];
            let n = stream.read(&mut buf).await.expect("Failed to read");
            let response = String::from_utf8_lossy(&buf[..n]);

            assert!(
                response.contains("\"result\":\"pong\""),
                "Client {} should get pong",
                client_id
            );
            assert!(
                response.contains(&format!("\"id\":{}", client_id)),
                "Client {} should have matching ID",
                client_id
            );
        });
        handles.push(handle);
    }

    // Wait for all clients to complete
    for (i, handle) in handles.into_iter().enumerate() {
        handle
            .await
            .unwrap_or_else(|_| panic!("Client {} task failed", i));
    }

    // Cleanup
    drop(daemon);
}

/// Test kiln.list returns empty array initially
#[tokio::test]
async fn test_e2e_kiln_list_initially_empty() {
    let mut daemon = TestDaemon::start().await.expect("Failed to start daemon");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect");

    // Request kiln list
    stream
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"kiln.list\"}\n")
        .await
        .expect("Failed to write");

    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await.expect("Failed to read");
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(
        response.contains("\"result\":[]"),
        "Expected empty kiln list"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

/// Test daemon handles invalid JSON gracefully
#[tokio::test]
async fn test_e2e_invalid_json_handling() {
    let mut daemon = TestDaemon::start().await.expect("Failed to start daemon");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect");

    // Send invalid JSON
    stream
        .write_all(b"{invalid json}\n")
        .await
        .expect("Failed to write");

    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await.expect("Failed to read");
    let response = String::from_utf8_lossy(&buf[..n]);

    // Should get parse error
    assert!(response.contains("error"), "Expected error response");
    assert!(response.contains("-32700"), "Expected PARSE_ERROR code");

    // Daemon should still be running and accept new requests
    stream
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n")
        .await
        .expect("Failed to write ping");

    let n = stream.read(&mut buf).await.expect("Failed to read");
    let response = String::from_utf8_lossy(&buf[..n]);
    assert!(
        response.contains("\"result\":\"pong\""),
        "Daemon should still work after parse error"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

/// Test daemon handles unknown methods gracefully
#[tokio::test]
async fn test_e2e_unknown_method_handling() {
    let mut daemon = TestDaemon::start().await.expect("Failed to start daemon");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect");

    // Send request with unknown method
    stream
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"unknown.method\"}\n")
        .await
        .expect("Failed to write");

    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await.expect("Failed to read");
    let response = String::from_utf8_lossy(&buf[..n]);

    // Should get method not found error
    assert!(response.contains("error"), "Expected error response");
    assert!(
        response.contains("-32601"),
        "Expected METHOD_NOT_FOUND code"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

/// Test client disconnect doesn't crash daemon
#[tokio::test]
async fn test_e2e_client_disconnect_handling() {
    let mut daemon = TestDaemon::start().await.expect("Failed to start daemon");

    // Connect and immediately disconnect
    {
        let _stream = UnixStream::connect(&daemon.socket_path)
            .await
            .expect("Failed to connect");
        // Stream drops here, closing connection
    }

    // Wait a bit
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify daemon is still running
    assert!(daemon.is_running(), "Daemon should still be running");

    // Verify we can still connect
    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to reconnect");

    stream
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
        .await
        .expect("Failed to write");

    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await.expect("Failed to read");
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(
        response.contains("\"result\":\"pong\""),
        "Daemon should still work after client disconnect"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}
