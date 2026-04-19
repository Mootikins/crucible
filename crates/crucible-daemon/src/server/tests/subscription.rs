use super::*;

#[tokio::test]
async fn test_server_has_event_broadcast() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let event_tx = server.event_sender();

    // Subscribe a receiver so send() succeeds
    let mut rx = event_tx.subscribe();

    // Should be able to send events
    let event = SessionEventMessage::text_delta("test-session", "hello");
    assert!(event_tx.send(event).is_ok());

    // Verify the event was received
    let received = rx.recv().await.unwrap();
    assert_eq!(received.session_id, "test-session");
    assert_eq!(received.event, "text_delta");
}

#[tokio::test]
async fn test_session_subscribe_rpc() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"subscribed\""));
    assert!(response.contains("chat-test"));
    assert!(response.contains("\"client_id\""));

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_session_subscribe_multiple_sessions() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"session-1\",\"session-2\",\"session-3\"]}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"subscribed\""));
    assert!(response.contains("session-1"));
    assert!(response.contains("session-2"));
    assert!(response.contains("session-3"));

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_session_subscribe_wildcard() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"*\"]}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"subscribed\""));
    assert!(response.contains("\"*\""));

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_session_subscribe_missing_session_ids() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{}}\n",
        )
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("error"));
    assert!(response.contains("-32602")); // INVALID_PARAMS
    assert!(response.contains("session_ids"));

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_session_subscribe_invalid_session_ids_type() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    // session_ids is a string, not an array
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":\"not-an-array\"}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("error"));
    assert!(response.contains("-32602")); // INVALID_PARAMS
    assert!(
        response.contains("session_ids") || response.contains("invalid type"),
        "Expected error message to mention 'session_ids' or 'invalid type', got: {}",
        response
    );

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_session_unsubscribe_rpc() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();

    // First subscribe
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let _ = client.read(&mut buf).await.unwrap();

    // Then unsubscribe
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"session.unsubscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
        .await
        .unwrap();

    buf.fill(0);
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"unsubscribed\""));
    assert!(response.contains("chat-test"));

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_session_unsubscribe_missing_session_ids() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.unsubscribe\",\"params\":{}}\n",
        )
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("error"));
    assert!(response.contains("-32602")); // INVALID_PARAMS
    assert!(response.contains("session_ids"));

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_event_broadcast_to_subscriber() {
    use std::time::Duration;

    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let event_tx = server.event_sender();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();

    // Subscribe to a session
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 4096];
    let _ = client.read(&mut buf).await.unwrap(); // consume subscription response

    // Send event through broadcast channel
    let event = SessionEventMessage::text_delta("chat-test", "hello world");
    event_tx.send(event).unwrap();

    // Client should receive the event
    tokio::time::sleep(Duration::from_millis(100)).await;

    buf.fill(0);
    let n = tokio::time::timeout(Duration::from_millis(500), client.read(&mut buf))
        .await
        .expect("timeout waiting for event")
        .unwrap();

    let received = String::from_utf8_lossy(&buf[..n]);
    assert!(
        received.contains("\"type\":\"event\""),
        "Response: {}",
        received
    );
    assert!(
        received.contains("\"session_id\":\"chat-test\""),
        "Response: {}",
        received
    );
    assert!(received.contains("hello world"), "Response: {}", received);

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_event_not_sent_to_non_subscriber() {
    use std::time::Duration;

    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let event_tx = server.event_sender();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();

    // Subscribe to session "other-session"
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"other-session\"]}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 4096];
    let _ = client.read(&mut buf).await.unwrap(); // consume subscription response

    // Send event for "chat-test" (different session)
    let event = SessionEventMessage::text_delta("chat-test", "should not receive");
    event_tx.send(event).unwrap();

    // Client should NOT receive the event (timeout expected)
    tokio::time::sleep(Duration::from_millis(50)).await;

    buf.fill(0);
    let result = tokio::time::timeout(Duration::from_millis(100), client.read(&mut buf)).await;
    assert!(
        result.is_err(),
        "Should timeout - client shouldn't receive unsubscribed events"
    );

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}
