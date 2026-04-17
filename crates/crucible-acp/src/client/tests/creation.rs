use std::path::PathBuf;

use agent_client_protocol::{ContentBlock, SessionNotification, SessionUpdate};

use crate::client::types::ClientConfig;
use crate::client::CrucibleAcpClient;

#[test]
fn test_client_creation() {
    let config = ClientConfig {
        agent_path: PathBuf::from("/test/agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(5000),
        max_retries: Some(3),
    };
    let client = CrucibleAcpClient::new(config);
    assert_eq!(client.config().agent_path, PathBuf::from("/test/agent"));
}

#[test]
fn test_parse_opencode_agent_message_chunk() {
    // This is the exact format OpenCode sends
    let json = r#"{
        "sessionId": "ses_test",
        "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "hello"}
        }
    }"#;

    let result: std::result::Result<SessionNotification, serde_json::Error> =
        serde_json::from_str(json);
    match &result {
        Ok(notif) => {
            println!("Parsed notification successfully");
            match &notif.update {
                SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
                    ContentBlock::Text(text) => {
                        assert_eq!(text.text, "hello");
                        println!("Got text: {}", text.text);
                    }
                    other => panic!("Expected Text content, got {:?}", other),
                },
                other => panic!("Expected AgentMessageChunk, got {:?}", other),
            }
        }
        Err(e) => {
            panic!("Failed to parse: {}", e);
        }
    }
}
