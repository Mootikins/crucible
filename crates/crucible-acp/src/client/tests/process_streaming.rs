use agent_client_protocol::StopReason;

use super::test_path;
use crate::client::types::{ClientConfig, StreamingState};
use crate::client::CrucibleAcpClient;

#[tokio::test]
async fn process_streaming_message_prioritizes_methods() {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let mut client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    let request_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "session/request_permission",
        "params": {}
    });

    let result = client
        .process_streaming_message(&request_payload, 1, &mut state)
        .await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
    assert_eq!(state.notification_count, 1);
}

#[tokio::test]
async fn process_streaming_message_returns_prompt_response() {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let mut client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    let response_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 5,
        "result": {
            "stopReason": "end_turn"
        }
    });

    let result = client
        .process_streaming_message(&response_payload, 5, &mut state)
        .await
        .expect("Should parse prompt response");
    assert!(result.is_some());
    assert_eq!(result.unwrap().stop_reason, StopReason::EndTurn);
}

#[tokio::test]
async fn process_streaming_message_tracks_available_commands() {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let mut client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "session-123",
            "update": {
                "sessionUpdate": "available_commands_update",
                "availableCommands": [
                    {
                        "name": "models",
                        "description": "Choose a model",
                        "input": null,
                        "meta": {
                            "secondary": ["claude-3.5-sonnet", "claude-3-opus"]
                        }
                    }
                ]
            }
        }
    });

    let result = client
        .process_streaming_message(&payload, 1, &mut state)
        .await
        .expect("Should parse notification");

    assert!(
        result.is_none(),
        "Notifications should not return prompt response"
    );
    assert_eq!(client.available_commands.len(), 1);
    assert_eq!(client.available_commands[0].name, "models");
}
