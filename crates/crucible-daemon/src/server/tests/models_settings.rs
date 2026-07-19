use super::*;

#[tokio::test]
async fn test_session_switch_model_rpc_success_and_empty_model_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, &server.kiln_path, 80).await;
    let configure_response =
        configure_internal_mock_agent(&mut client, &session_id, 81, "mock-initial").await;
    assert!(
        configure_response["error"].is_null(),
        "configure failed: {configure_response:?}"
    );

    let ok_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 82,
            "method": "session.switch_model",
            "params": {
                "session_id": session_id,
                "model_id": "mock-switched"
            }
        }),
    )
    .await;
    assert!(
        ok_response["error"].is_null(),
        "session.switch_model failed: {ok_response:?}"
    );
    assert_eq!(ok_response["result"]["switched"], true);
    assert_eq!(ok_response["result"]["model_id"], "mock-switched");

    let err_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 83,
            "method": "session.switch_model",
            "params": {
                "session_id": session_id,
                "model_id": "   "
            }
        }),
    )
    .await;
    assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_list_models_rpc_success_and_missing_param_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, &server.kiln_path, 90).await;

    let ok_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 91,
            "method": "session.list_models",
            "params": {
                "session_id": session_id
            }
        }),
    )
    .await;
    assert!(
        ok_response["error"].is_null(),
        "session.list_models failed: {ok_response:?}"
    );
    assert!(ok_response["result"]["models"].is_array());

    let err_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 92,
            "method": "session.list_models",
            "params": {}
        }),
    )
    .await;
    assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

    server.shutdown().await;
}

#[tokio::test]
async fn test_models_list_rpc_no_session() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    // Call models.list with no params — should succeed without a session
    let response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "models.list",
            "params": {}
        }),
    )
    .await;
    assert!(
        response["error"].is_null(),
        "models.list failed: {response:?}"
    );
    assert!(
        response["result"]["models"].is_array(),
        "models.list should return a models array: {response:?}"
    );

    // Call models.list with a kiln_path — should also succeed
    let response_with_kiln = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "models.list",
            "params": {
                "kiln_path": server.tmp.path().to_string_lossy()
            }
        }),
    )
    .await;
    assert!(
        response_with_kiln["error"].is_null(),
        "models.list with kiln_path failed: {response_with_kiln:?}"
    );
    assert!(response_with_kiln["result"]["models"].is_array());

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_set_thinking_budget_rpc_success_and_missing_session_id_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, &server.kiln_path, 100).await;
    let configure_response =
        configure_internal_mock_agent(&mut client, &session_id, 101, "mock-budget").await;
    assert!(
        configure_response["error"].is_null(),
        "configure failed: {configure_response:?}"
    );

    let ok_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 102,
            "method": "session.set_thinking_budget",
            "params": {
                "session_id": session_id,
                "thinking_budget": 256
            }
        }),
    )
    .await;
    assert!(
        ok_response["error"].is_null(),
        "session.set_thinking_budget failed: {ok_response:?}"
    );
    assert_eq!(ok_response["result"]["thinking_budget"], 256);

    let err_response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 103,
            "method": "session.set_thinking_budget",
            "params": {
                "thinking_budget": 1
            }
        }),
    )
    .await;
    assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

    server.shutdown().await;
}
