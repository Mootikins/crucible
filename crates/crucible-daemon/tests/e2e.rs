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

    let stream = UnixStream::connect(&daemon.socket_path)
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

// ─────────────────────────────────────────────────────────────────────────────
// Session Lifecycle Tests
// ─────────────────────────────────────────────────────────────────────────────

mod session_helpers {
    use std::path::{Path, PathBuf};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    /// Send a JSON-RPC request and parse the response
    pub async fn rpc_call(stream: &mut UnixStream, request: &str) -> serde_json::Value {
        stream
            .write_all(format!("{}\n", request).as_bytes())
            .await
            .expect("Failed to write request");

        let mut buf = vec![0u8; 8192];
        let n = stream
            .read(&mut buf)
            .await
            .expect("Failed to read response");
        serde_json::from_slice(&buf[..n]).expect("Failed to parse JSON response")
    }

    /// Extract the "result" field from a JSON-RPC response
    pub fn get_result(response: &serde_json::Value) -> &serde_json::Value {
        response.get("result").expect("Response should have result")
    }

    /// Extract a string field from a JSON object
    pub fn get_str<'a>(obj: &'a serde_json::Value, field: &str) -> &'a str {
        obj.get(field)
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("Should have string field '{}'", field))
    }

    /// Check if the "state" field contains the expected value (case-insensitive)
    pub fn assert_state_contains(result: &serde_json::Value, expected: &str, context: &str) {
        let state = result.get("state").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            state.to_lowercase().contains(&expected.to_lowercase()),
            "{}: expected state to contain '{}', got '{}'",
            context,
            expected,
            state
        );
    }

    /// Create a kiln directory in the daemon's temp directory
    pub fn create_kiln_dir(socket_path: &Path) -> PathBuf {
        let kiln_dir = socket_path.parent().unwrap().join("kiln");
        std::fs::create_dir_all(&kiln_dir).expect("Failed to create kiln dir");
        kiln_dir
    }

    /// Build a session.create request
    pub fn session_create_request(id: u32, kiln_path: &str) -> String {
        format!(
            r#"{{"jsonrpc":"2.0","id":{},"method":"session.create","params":{{"type":"chat","kiln":"{}"}}}}"#,
            id, kiln_path
        )
    }

    /// Build a session action request (pause, resume, end)
    pub fn session_action_request(id: u32, method: &str, session_id: &str) -> String {
        format!(
            r#"{{"jsonrpc":"2.0","id":{},"method":"session.{}","params":{{"session_id":"{}"}}}}"#,
            id, method, session_id
        )
    }
}

use session_helpers::*;

/// Test full session lifecycle: create → pause → resume → end
#[tokio::test]
async fn test_e2e_session_lifecycle() {
    let mut daemon = TestDaemon::start().await.expect("Failed to start daemon");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect");

    let kiln_dir = create_kiln_dir(&daemon.socket_path);
    let kiln_path = kiln_dir.to_string_lossy();

    // 1. Create session
    let response = rpc_call(&mut stream, &session_create_request(1, &kiln_path)).await;
    let result = get_result(&response);
    let session_id = get_str(result, "session_id");
    assert_state_contains(result, "active", "New session");

    // 2. Pause session
    let response = rpc_call(&mut stream, &session_action_request(2, "pause", session_id)).await;
    assert_state_contains(get_result(&response), "paused", "After pause");

    // 3. Resume session
    let response = rpc_call(
        &mut stream,
        &session_action_request(3, "resume", session_id),
    )
    .await;
    assert_state_contains(get_result(&response), "active", "After resume");

    // 4. End session
    let response = rpc_call(&mut stream, &session_action_request(4, "end", session_id)).await;
    assert_state_contains(get_result(&response), "ended", "After end");

    daemon.stop().await.expect("Failed to stop daemon");
}

/// Test session list returns created sessions
#[tokio::test]
async fn test_e2e_session_list() {
    let mut daemon = TestDaemon::start().await.expect("Failed to start daemon");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect");

    let kiln_dir = create_kiln_dir(&daemon.socket_path);
    let kiln_path = kiln_dir.to_string_lossy();

    // Create two sessions
    for i in 1..=2 {
        rpc_call(&mut stream, &session_create_request(i, &kiln_path)).await;
    }

    // List sessions
    let response = rpc_call(
        &mut stream,
        r#"{"jsonrpc":"2.0","id":3,"method":"session.list","params":{}}"#,
    )
    .await;

    let result = get_result(&response);
    let total = result.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
    assert_eq!(total, 2, "Should have 2 sessions");

    daemon.stop().await.expect("Failed to stop daemon");
}

/// Test model switching updates the session agent config
#[tokio::test]
async fn test_e2e_model_switching() {
    let mut daemon = TestDaemon::start().await.expect("Failed to start daemon");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect");

    let kiln_dir = create_kiln_dir(&daemon.socket_path);
    let kiln_path = kiln_dir.to_string_lossy();

    // 1. Create session
    let response = rpc_call(&mut stream, &session_create_request(1, &kiln_path)).await;
    let session_id = get_str(get_result(&response), "session_id");

    // 2. Configure agent with initial model
    let configure_request = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"session.configure_agent","params":{{"session_id":"{}","agent":{{"agent_type":"internal","provider":"ollama","model":"initial-model","system_prompt":"test"}}}}}}"#,
        session_id
    );
    let response = rpc_call(&mut stream, &configure_request).await;
    assert!(
        response.get("result").is_some(),
        "Configure should succeed: {:?}",
        response
    );

    // 3. Get session to verify initial model
    let get_request = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"session.get","params":{{"session_id":"{}"}}}}"#,
        session_id
    );
    let response = rpc_call(&mut stream, &get_request).await;
    let result = get_result(&response);
    let agent = result.get("agent").expect("Should have agent");
    let initial_model = agent.get("model").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(initial_model, "initial-model", "Should have initial model");

    // 4. Switch to new model
    let switch_request = format!(
        r#"{{"jsonrpc":"2.0","id":4,"method":"session.switch_model","params":{{"session_id":"{}","model_id":"switched-model"}}}}"#,
        session_id
    );
    let response = rpc_call(&mut stream, &switch_request).await;
    assert!(
        response.get("result").is_some(),
        "Switch should succeed: {:?}",
        response
    );
    let result = get_result(&response);
    assert_eq!(
        result.get("switched").and_then(|v| v.as_bool()),
        Some(true),
        "Should indicate switch succeeded"
    );

    // 5. Get session again to verify model was updated
    let get_request = format!(
        r#"{{"jsonrpc":"2.0","id":5,"method":"session.get","params":{{"session_id":"{}"}}}}"#,
        session_id
    );
    let response = rpc_call(&mut stream, &get_request).await;
    let result = get_result(&response);
    let agent = result.get("agent").expect("Should have agent after switch");
    let new_model = agent.get("model").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(
        new_model, "switched-model",
        "Model should be updated after switch"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

/// Test session persisted to disk
#[tokio::test]
async fn test_e2e_session_persistence() {
    let mut daemon = TestDaemon::start().await.expect("Failed to start daemon");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect");

    let kiln_dir = create_kiln_dir(&daemon.socket_path);
    let kiln_path = kiln_dir.to_string_lossy();

    // Create a session
    let response = rpc_call(&mut stream, &session_create_request(1, &kiln_path)).await;
    let session_id = get_str(get_result(&response), "session_id");

    // Check that meta.json was created
    let session_dir = kiln_dir.join(".crucible").join("sessions").join(session_id);

    // Poll for file to exist with timeout (avoids flaky sleep)
    let meta_file = session_dir.join("meta.json");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !meta_file.exists() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(
        meta_file.exists(),
        "meta.json should exist at {:?}",
        session_dir
    );

    daemon.stop().await.expect("Failed to stop daemon");
}
