use super::*;

#[tokio::test]
async fn test_session_pause_rpc_success_and_missing_param_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, &server.kiln_path, 30).await;

    let ok_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 31,
            "method": "session.pause",
            "params": { "session_id": session_id }
        }),
    )
    .await;
    assert!(
        ok_response["error"].is_null(),
        "session.pause failed: {ok_response:?}"
    );
    assert_eq!(ok_response["result"]["state"], "paused");

    let err_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 32,
            "method": "session.pause",
            "params": {}
        }),
    )
    .await;
    assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_resume_rpc_success_and_missing_param_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, &server.kiln_path, 40).await;

    let _pause_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 41,
            "method": "session.pause",
            "params": { "session_id": session_id }
        }),
    )
    .await;

    let ok_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "session.resume",
            "params": { "session_id": session_id }
        }),
    )
    .await;
    assert!(
        ok_response["error"].is_null(),
        "session.resume failed: {ok_response:?}"
    );
    assert_eq!(ok_response["result"]["state"], "active");

    let err_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 43,
            "method": "session.resume",
            "params": {}
        }),
    )
    .await;
    assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_lifecycle_create_pause_resume_end() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, &server.kiln_path, 44_000).await;

    let pause_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 44_001,
            "method": "session.pause",
            "params": { "session_id": session_id }
        }),
    )
    .await;
    assert!(pause_response["error"].is_null());
    assert_eq!(pause_response["result"]["state"], "paused");

    let resume_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 44_002,
            "method": "session.resume",
            "params": { "session_id": session_id }
        }),
    )
    .await;
    assert!(resume_response["error"].is_null());
    assert_eq!(resume_response["result"]["state"], "active");

    let end_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 44_003,
            "method": "session.end",
            "params": { "session_id": session_id }
        }),
    )
    .await;
    assert!(end_response["error"].is_null());
    assert_eq!(end_response["result"]["state"], "ended");

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_resume_active_session_returns_invalid_state_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, &server.kiln_path, 45_000).await;

    let resume_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 45_001,
            "method": "session.resume",
            "params": { "session_id": session_id }
        }),
    )
    .await;

    assert_eq!(resume_response["error"]["code"], INVALID_PARAMS);
    assert!(resume_response["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("resume"));

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_pause_after_end_returns_invalid_state_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, &server.kiln_path, 46_000).await;

    let end_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 46_001,
            "method": "session.end",
            "params": { "session_id": session_id }
        }),
    )
    .await;
    assert!(end_response["error"].is_null());

    let pause_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 46_002,
            "method": "session.pause",
            "params": { "session_id": session_id }
        }),
    )
    .await;

    assert_eq!(pause_response["error"]["code"], INVALID_PARAMS);
    assert!(pause_response["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("pause"));

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_configure_agent_rpc_success_and_missing_agent_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, &server.kiln_path, 50).await;

    let ok_response =
        configure_internal_mock_agent(&mut client, &session_id, 51, "mock-initial").await;
    assert!(
        ok_response["error"].is_null(),
        "session.configure_agent failed: {ok_response:?}"
    );
    assert_eq!(ok_response["result"]["configured"], true);

    let err_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 52,
            "method": "session.configure_agent",
            "params": {
                "session_id": session_id
            }
        }),
    )
    .await;
    assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_send_message_rpc_no_agent_configured_error_and_missing_content_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, &server.kiln_path, 60).await;

    let no_agent_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 61,
            "method": "session.send_message",
            "params": {
                "session_id": session_id,
                "content": "hello"
            }
        }),
    )
    .await;
    assert!(no_agent_response["error"].is_object());
    assert_eq!(no_agent_response["error"]["code"], INTERNAL_ERROR);

    let missing_content_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 62,
            "method": "session.send_message",
            "params": {
                "session_id": session_id
            }
        }),
    )
    .await;
    assert_eq!(missing_content_response["error"]["code"], INVALID_PARAMS);

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_cancel_rpc_success_and_missing_param_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, &server.kiln_path, 70).await;

    let ok_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 71,
            "method": "session.cancel",
            "params": { "session_id": session_id }
        }),
    )
    .await;
    assert!(
        ok_response["error"].is_null(),
        "session.cancel failed: {ok_response:?}"
    );
    assert!(ok_response["result"]["cancelled"].is_boolean());

    let err_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 72,
            "method": "session.cancel",
            "params": {}
        }),
    )
    .await;
    assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

    server.shutdown().await;
}
