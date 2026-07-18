use super::*;

#[tokio::test]
async fn test_server_ping() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();

    // Spawn server
    let server_task = tokio::spawn(async move { server.run().await });

    // Give server time to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connect and send ping
    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"result\":\"pong\""));
    assert!(response.contains("\"id\":1"));

    // Shutdown
    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_kiln_open_missing_path_param() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(async move { server.run().await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    // Missing "path" parameter
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"kiln.open\",\"params\":{}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("error"));
    assert!(response.contains("-32602")); // INVALID_PARAMS

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_kiln_close_missing_path_param() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(async move { server.run().await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    // Missing "path" parameter
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"kiln.close\",\"params\":{}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("error"));
    assert!(response.contains("-32602")); // INVALID_PARAMS

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_kiln_list_returns_array() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    // Hermetic config root: without this the daemon loads the developer's real
    // ~/.crucible registry and opens those kilns at startup. Because that load
    // is async, it races this 50ms-later kiln.list — empty if the sleep wins,
    // populated if the load wins — which is exactly the observed flakiness. An
    // isolated (empty) home makes the list deterministically empty. nextest's
    // per-test process isolation keeps this env write from racing other tests.
    std::env::set_var("CRUCIBLE_HOME", tmp.path());

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(async move { server.run().await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"kiln.list\",\"params\":{}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"result\":[]")); // Empty array initially

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_search_vectors_rpc_success_and_missing_vector_error() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");
    let kiln_path = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln_path).unwrap();

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(async move { server.run().await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();

    let open_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "kiln.open",
            "params": { "path": kiln_path }
        }),
    )
    .await;
    assert!(
        open_response["error"].is_null(),
        "kiln.open failed: {open_response:?}"
    );

    let ok_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "search_vectors",
            "params": {
                "kiln": kiln_path,
                "vector": [0.1, 0.2, 0.3],
                "limit": 5
            }
        }),
    )
    .await;
    assert!(
        ok_response["error"].is_null(),
        "search_vectors failed: {ok_response:?}"
    );
    assert!(ok_response["result"].is_array());

    let err_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "search_vectors",
            "params": {
                "kiln": kiln_path
            }
        }),
    )
    .await;
    assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_session_list_rpc_returns_shape_and_accepts_invalid_filters() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");
    let kiln_path = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln_path).unwrap();

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(async move { server.run().await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    let _session_id = create_chat_session(&mut client, &kiln_path, 20).await;

    let ok_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 21,
            "method": "session.list",
            "params": {}
        }),
    )
    .await;
    assert!(
        ok_response["error"].is_null(),
        "session.list failed: {ok_response:?}"
    );
    assert!(ok_response["result"]["sessions"].is_array());
    assert!(ok_response["result"]["total"].as_u64().unwrap_or(0) >= 1);

    let invalid_filters_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 22,
            "method": "session.list",
            "params": {
                "type": 123,
                "state": ["bad"],
                "kiln": false
            }
        }),
    )
    .await;
    assert!(
        invalid_filters_response["error"].is_null(),
        "session.list should ignore invalid optional filters: {invalid_filters_response:?}"
    );
    assert!(invalid_filters_response["result"]["sessions"].is_array());

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_method_not_found() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(async move { server.run().await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"unknown.method\",\"params\":{}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("error"));
    assert!(response.contains("-32601")); // METHOD_NOT_FOUND

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_parse_error() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(async move { server.run().await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    // Invalid JSON
    client.write_all(b"{invalid json}\n").await.unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("error"));
    assert!(response.contains("-32700")); // PARSE_ERROR

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_shutdown_method() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let server_task = tokio::spawn(async move { server.run().await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"shutdown\",\"params\":{}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"result\":\"shutting down\""));

    // Server should shut down gracefully
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), server_task).await;

    assert!(result.is_ok(), "Server should shutdown within timeout");
}

#[tokio::test]
async fn test_kiln_open_nonexistent_path_fails() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(async move { server.run().await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    // Valid request format, but path doesn't exist
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"kiln.open\",\"params\":{\"path\":\"/nonexistent/path/to/kiln\"}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("error"));
    assert!(response.contains("-32603")); // INTERNAL_ERROR (can't open DB)

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_client_disconnect_closes_connection() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(async move { server.run().await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connect and immediately disconnect
    {
        let _client = UnixStream::connect(&sock_path).await.unwrap();
        // Client drops here, closing connection
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Server should still be running and accept new connections
    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"result\":\"pong\""));

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}
