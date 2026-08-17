use super::*;

#[tokio::test]
async fn test_session_switch_model_rpc_success_and_empty_model_error() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, TestServer::KILN, 80).await;
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
    let session_id = create_chat_session(&mut client, TestServer::KILN, 90).await;

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
    let session_id = create_chat_session(&mut client, TestServer::KILN, 100).await;
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

/// `session.list_modes` must agree with `session.get_mode`. The two are
/// separate RPCs, so nothing structurally forces it: `session_modes()` used to
/// derive `current_mode_id` from declaration order, which meant a session in
/// `plan` was reported as being in whichever mode the defaults file declared
/// first. Both UIs would have hydrated the wrong chip from it.
#[tokio::test]
async fn session_list_modes_reports_the_session_s_own_current_mode() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, TestServer::KILN, 110).await;
    configure_internal_mock_agent(&mut client, &session_id, 111, "mock-initial").await;

    let before = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 112,
            "method": "session.list_modes",
            "params": { "session_id": session_id }
        }),
    )
    .await;
    assert!(
        before["error"].is_null(),
        "session.list_modes failed: {before:?}"
    );
    let ids: Vec<String> = before["result"]["modes"]
        .as_array()
        .expect("modes must be an array")
        .iter()
        .map(|m| m["id"].as_str().expect("mode id").to_string())
        .collect();
    assert!(
        ids.contains(&"normal".to_string()) && ids.contains(&"plan".to_string()),
        "the built-in modes must be listed, got {ids:?}"
    );
    assert!(
        ids.contains(
            &before["result"]["current_mode_id"]
                .as_str()
                .unwrap()
                .to_string()
        ),
        "current_mode_id must be one of the listed modes: {before:?}"
    );

    let set = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 113,
            "method": "session.set_mode",
            "params": { "session_id": session_id, "mode_id": "plan" }
        }),
    )
    .await;
    assert!(set["error"].is_null(), "session.set_mode failed: {set:?}");

    let after = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 114,
            "method": "session.list_modes",
            "params": { "session_id": session_id }
        }),
    )
    .await;
    assert_eq!(
        after["result"]["current_mode_id"], "plan",
        "list_modes must follow the session's mode, not declaration order"
    );

    let get = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 115,
            "method": "session.get_mode",
            "params": { "session_id": session_id }
        }),
    )
    .await;
    assert_eq!(
        get["result"]["mode"], after["result"]["current_mode_id"],
        "session.get_mode and session.list_modes must not disagree"
    );

    let err = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 116,
            "method": "session.list_modes",
            "params": {}
        }),
    )
    .await;
    assert_eq!(err["error"]["code"], INVALID_PARAMS);

    server.shutdown().await;
}
