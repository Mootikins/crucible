//! Streaming response implementation
//!
//! Implements the ACP streaming protocol where content is delivered via
//! `session/update` notifications followed by a final `PromptResponse`.

use serde_json::{json, Value};
use std::io::{self, Write};

/// Send a streaming response for a prompt request
///
/// This sends the correct ACP protocol sequence:
/// 1. Multiple `session/update` notifications (no id field)
/// 2. Final `PromptResponse` with matching id
///
/// Each notification contains a `SessionNotification` in params:
/// ```json
/// {
///   "jsonrpc": "2.0",
///   "method": "session/update",
///   "params": {
///     "sessionId": "...",
///     "update": {
///       "sessionUpdate": "agent_message_chunk",
///       "content": {
///         "type": "text",
///         "text": "chunk text"
///       }
///     }
///   }
/// }
/// ```
pub fn send_streaming_response(
    request: &Value,
    session_id: &str,
    chunks: &[&str],
    chunk_delay_ms: Option<u64>,
    send_final: bool,
    stdout: &mut io::Stdout,
) -> io::Result<()> {
    // Send each chunk as a session/update notification
    for chunk_text in chunks {
        let notification = create_agent_message_chunk_notification(session_id, chunk_text);

        writeln!(stdout, "{}", serde_json::to_string(&notification)?)?;
        stdout.flush()?;

        // Apply delay if configured
        if let Some(delay_ms) = chunk_delay_ms {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }

    // Send final response if requested
    if send_final {
        let final_response = create_prompt_response(request)?;
        writeln!(stdout, "{}", serde_json::to_string(&final_response)?)?;
        stdout.flush()?;
    }

    Ok(())
}

/// Create a `session/update` notification with `AgentMessageChunk`
///
/// Structure follows ACP spec:
/// - No `id` field (it's a notification)
/// - `method`: "session/update"
/// - `params`: Contains `SessionNotification` with `sessionId` and `update`
/// - `update`: Contains discriminator `sessionUpdate: "agent_message_chunk"`
fn create_agent_message_chunk_notification(session_id: &str, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {
                    "type": "text",
                    "text": text
                }
            }
        }
    })
}

/// Create a final `PromptResponse` with matching id
///
/// Structure:
/// - `id`: Matches the original request id
/// - `result`: Contains `PromptResponse` with `stopReason`
fn create_prompt_response(request: &Value) -> io::Result<Value> {
    let request_id = request
        .get("id")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Request missing id"))?;

    Ok(json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {
            "stopReason": "end_turn"
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_notification_structure() {
        let notification = create_agent_message_chunk_notification("test-session", "Hello");

        // Verify structure
        assert_eq!(notification["jsonrpc"], "2.0");
        assert_eq!(notification["method"], "session/update");
        assert!(
            notification.get("id").is_none(),
            "Notifications should not have id"
        );

        // Verify params structure
        let params = &notification["params"];
        assert_eq!(params["sessionId"], "test-session");

        // Verify update structure
        let update = &params["update"];
        assert_eq!(update["sessionUpdate"], "agent_message_chunk");
        assert_eq!(update["content"]["type"], "text");
        assert_eq!(update["content"]["text"], "Hello");
    }

    #[test]
    fn test_create_final_response() {
        let request = json!({"jsonrpc": "2.0", "id": 42, "method": "session/prompt"});
        let response = create_prompt_response(&request).unwrap();

        // Verify structure
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 42);
        assert_eq!(response["result"]["stopReason"], "end_turn");
    }

    #[test]
    fn test_final_response_with_string_id() {
        let request = json!({"jsonrpc": "2.0", "id": "abc123", "method": "session/prompt"});
        let response = create_prompt_response(&request).unwrap();

        // Should preserve string id
        assert_eq!(response["id"], "abc123");
    }
}
