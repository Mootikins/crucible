use super::*;

#[tokio::test]
async fn test_server_ping() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    // Connect and send ping
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"result\":\"pong\""));
    assert!(response.contains("\"id\":1"));

    server.shutdown().await;
}

#[tokio::test]
async fn test_kiln_open_missing_path_param() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

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

    server.shutdown().await;
}

#[tokio::test]
async fn test_kiln_close_missing_path_param() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

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

    server.shutdown().await;
}

#[tokio::test]
async fn test_kiln_list_returns_array() {
    // Inject an isolated data root (no env) so the daemon never loads the
    // developer's real ~/.crucible registry — the async startup load would
    // otherwise race this 50ms-later kiln.list and make it non-empty.
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"kiln.list\",\"params\":{}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"result\":[]")); // Empty array initially

    server.shutdown().await;
}

#[tokio::test]
async fn test_search_vectors_rpc_success_and_missing_vector_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    let open_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "kiln.open",
            "params": { "path": server.kiln_path }
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
                "kiln": TestServer::KILN,
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
                "kiln": TestServer::KILN
            }
        }),
    )
    .await;
    assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_list_rpc_returns_shape_and_accepts_invalid_filters() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let _session_id = create_chat_session(&mut client, TestServer::KILN, 20).await;

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

    server.shutdown().await;
}

#[tokio::test]
async fn test_method_not_found() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"unknown.method\",\"params\":{}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("error"));
    assert!(response.contains("-32601")); // METHOD_NOT_FOUND

    server.shutdown().await;
}

#[tokio::test]
async fn test_parse_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    // Invalid JSON
    client.write_all(b"{invalid json}\n").await.unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("error"));
    assert!(response.contains("-32700")); // PARSE_ERROR

    server.shutdown().await;
}

#[tokio::test]
async fn test_shutdown_method() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"shutdown\",\"params\":{}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"result\":\"shutting down\""));

    // Server should shut down gracefully
    let shut_down_in_time = server
        .await_shutdown_within(std::time::Duration::from_secs(1))
        .await;
    assert!(shut_down_in_time, "Server should shutdown within timeout");
}

#[tokio::test]
async fn test_kiln_open_nonexistent_path_fails() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

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

    server.shutdown().await;
}

#[tokio::test]
async fn test_client_disconnect_closes_connection() {
    let server = TestServer::start().await;

    // Connect and immediately disconnect
    {
        let _client = server.connect().await;
        // Client drops here, closing connection
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Server should still be running and accept new connections
    let mut client = server.connect().await;
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("\"result\":\"pong\""));

    server.shutdown().await;
}

/// Zero kilns is a legitimate session shape, and it must produce a genuinely
/// EMPTY set. It used to fall back to the daemon's data root — the PARENT of
/// the sessions root — so every kiln-less session carried an allowed root
/// enclosing every transcript the daemon had ever written.
#[tokio::test]
async fn a_kiln_less_create_attaches_no_kiln_at_all() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    let response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.create",
            "params": { "type": "chat" }
        }),
    )
    .await;

    assert!(
        response["error"].is_null(),
        "a kiln-less create must succeed: {response:?}"
    );
    assert_eq!(
        response["result"]["kilns"],
        json!([]),
        "a kiln-less session must not inherit the data root as a kiln: {response:?}"
    );

    server.shutdown().await;
}

/// Containment is deepest-match-wins, so an allowed root INSIDE the denied
/// sessions root beats the denial. Attaching another session's storage
/// directory as a kiln is therefore a way to hand the agent exactly the
/// transcript the deny root exists to close — and the catastrophic-roots floor
/// waves it through, because it is neither `/`, home, nor a system tree.
#[tokio::test]
async fn create_refuses_a_kiln_inside_the_session_storage_root() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    let victim = server.sessions_root().join("chat-victim");
    std::fs::create_dir_all(&victim).unwrap();

    // Two doors, both closed, and for different reasons — which is the point of
    // moving the floor to registration rather than replacing it.
    //
    // The path is not a name, so it never reaches a kiln at all. Nothing about
    // this refusal depends on the sessions-root rule remembering to fire; the
    // request cannot express the directory in the first place.
    let by_path = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.create",
            "params": { "type": "chat", "kilns": [victim] }
        }),
    )
    .await;
    assert_eq!(
        by_path["error"]["code"], -32602,
        "a directory is not a kiln name and must be refused: {by_path:?}"
    );
    assert!(
        by_path["result"].is_null(),
        "the refusal must not also have created a session: {by_path:?}"
    );

    // And the name-shaped door: a kiln nobody registered resolves to nothing,
    // so it is refused rather than attached as a kiln that reaches nowhere.
    let by_name = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "session.create",
            "params": { "type": "chat", "kilns": ["chat-victim"] }
        }),
    )
    .await;
    assert_eq!(
        by_name["error"]["code"], -32602,
        "an unregistered name must be refused, not attached: {by_name:?}"
    );

    server.shutdown().await;
}
