//! Integration tests for session persistence
//!
//! Tests the full save -> resume flow.

use crucible_cli::session_logger::SessionLogger;
use crucible_observe::{LogEvent, SessionId};
use std::sync::Arc;
use tempfile::TempDir;

/// Test complete save and resume cycle
#[tokio::test]
async fn test_session_save_and_resume() {
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().to_path_buf();

    // === Phase 1: Create and save a session ===
    let logger = Arc::new(SessionLogger::new(kiln_path.clone()));

    // Log conversation
    logger.log_user_message("Hello, assistant!").await;
    logger.accumulate_assistant_chunk("Hello! How can I ").await;
    logger.accumulate_assistant_chunk("help you today?").await;
    logger.flush_assistant_message(Some("test-model")).await;
    logger.log_user_message("What is 2+2?").await;
    logger.accumulate_assistant_chunk("2+2 equals 4.").await;
    logger.flush_assistant_message(Some("test-model")).await;

    // Get session ID and finish
    let session_id = logger.session_id().await.expect("Session should exist");
    logger.finish().await;

    // === Phase 2: Resume the session ===
    let logger2 = Arc::new(SessionLogger::new(kiln_path));
    let events = logger2
        .resume_session(&session_id)
        .await
        .expect("Should be able to resume session");

    // === Verify: All messages present ===
    let user_messages: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            LogEvent::User { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();

    let assistant_messages: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            LogEvent::Assistant { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(user_messages, vec!["Hello, assistant!", "What is 2+2?"]);
    assert_eq!(
        assistant_messages,
        vec!["Hello! How can I help you today?", "2+2 equals 4."]
    );
}

/// Test that empty sessions (no messages) don't create files
#[tokio::test]
async fn test_empty_session_no_files() {
    let tmp = TempDir::new().unwrap();
    let logger = SessionLogger::new(tmp.path().to_path_buf());

    // Don't log anything, just finish
    logger.finish().await;

    // No session should be created
    assert!(logger.session_id().await.is_none());

    let sessions_dir = tmp.path().join(".crucible").join("sessions");
    if sessions_dir.exists() {
        let entries: Vec<_> = std::fs::read_dir(&sessions_dir).unwrap().collect();
        assert!(entries.is_empty(), "No session directories should exist");
    }
}

/// Test session ID parsing roundtrip
#[tokio::test]
async fn test_session_id_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let logger = Arc::new(SessionLogger::new(tmp.path().to_path_buf()));

    logger.log_user_message("Test").await;
    let session_id = logger.session_id().await.unwrap();
    logger.finish().await;

    // Parse and compare
    let parsed = SessionId::parse(session_id.as_str()).unwrap();
    assert_eq!(parsed.as_str(), session_id.as_str());
}

/// Test resuming a session allows appending new messages
#[tokio::test]
async fn test_resume_and_continue_conversation() {
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().to_path_buf();

    // === Phase 1: Create initial session ===
    let logger1 = Arc::new(SessionLogger::new(kiln_path.clone()));
    logger1.log_user_message("First message").await;
    logger1.accumulate_assistant_chunk("First response").await;
    logger1.flush_assistant_message(Some("test-model")).await;
    let session_id = logger1.session_id().await.unwrap();
    logger1.finish().await;

    // === Phase 2: Resume and continue ===
    let logger2 = Arc::new(SessionLogger::new(kiln_path.clone()));
    let events = logger2.resume_session(&session_id).await.unwrap();
    assert_eq!(events.len(), 2); // user + assistant

    // Add more to the conversation
    logger2.log_user_message("Second message").await;
    logger2.accumulate_assistant_chunk("Second response").await;
    logger2.flush_assistant_message(Some("test-model")).await;
    logger2.finish().await;

    // === Phase 3: Verify full conversation ===
    let logger3 = Arc::new(SessionLogger::new(kiln_path));
    let all_events = logger3.resume_session(&session_id).await.unwrap();

    let user_messages: Vec<_> = all_events
        .iter()
        .filter_map(|e| match e {
            LogEvent::User { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(user_messages, vec!["First message", "Second message"]);
}

/// Test that tool calls and results are persisted correctly
#[tokio::test]
async fn test_tool_call_persistence() {
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().to_path_buf();

    // Create session with tool call
    let logger = Arc::new(SessionLogger::new(kiln_path.clone()));
    logger.log_user_message("Read the file").await;
    logger
        .log_tool_call(
            "tc_001",
            "read_file",
            serde_json::json!({"path": "test.rs"}),
        )
        .await;
    logger.log_tool_result("tc_001", "fn main() {}").await;
    logger.accumulate_assistant_chunk("Done!").await;
    logger.flush_assistant_message(Some("test-model")).await;
    let session_id = logger.session_id().await.unwrap();
    logger.finish().await;

    // Resume and verify
    let logger2 = Arc::new(SessionLogger::new(kiln_path));
    let events = logger2.resume_session(&session_id).await.unwrap();

    // Find tool call event
    let tool_call = events
        .iter()
        .find(|e| matches!(e, LogEvent::ToolCall { .. }));
    assert!(tool_call.is_some(), "Tool call should be persisted");

    if let LogEvent::ToolCall { name, args, id, .. } = tool_call.unwrap() {
        assert_eq!(name, "read_file");
        assert_eq!(id, "tc_001");
        assert_eq!(args["path"], "test.rs");
    }

    // Find tool result event
    let tool_result = events
        .iter()
        .find(|e| matches!(e, LogEvent::ToolResult { .. }));
    assert!(tool_result.is_some(), "Tool result should be persisted");

    if let LogEvent::ToolResult {
        result, truncated, ..
    } = tool_result.unwrap()
    {
        assert_eq!(result, "fn main() {}");
        assert!(!truncated);
    }
}

/// Test error event persistence
#[tokio::test]
async fn test_error_persistence() {
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().to_path_buf();

    let logger = Arc::new(SessionLogger::new(kiln_path.clone()));
    logger.log_user_message("Do something").await;
    logger.log_error("Something went wrong", true).await;
    let session_id = logger.session_id().await.unwrap();
    logger.finish().await;

    // Resume and verify
    let logger2 = Arc::new(SessionLogger::new(kiln_path));
    let events = logger2.resume_session(&session_id).await.unwrap();

    let error_event = events.iter().find(|e| matches!(e, LogEvent::Error { .. }));
    assert!(error_event.is_some(), "Error should be persisted");

    if let LogEvent::Error {
        message,
        recoverable,
        ..
    } = error_event.unwrap()
    {
        assert_eq!(message, "Something went wrong");
        assert!(*recoverable);
    }
}

/// Test listing sessions returns all created sessions
#[tokio::test]
async fn test_list_sessions() {
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().to_path_buf();

    // Create first session
    let logger1 = SessionLogger::new(kiln_path.clone());
    logger1.log_user_message("Session 1").await;
    let id1 = logger1.session_id().await.unwrap();
    logger1.finish().await;

    // Create second session
    let logger2 = SessionLogger::new(kiln_path.clone());
    logger2.log_user_message("Session 2").await;
    let id2 = logger2.session_id().await.unwrap();
    logger2.finish().await;

    // List sessions - both should be present
    let logger3 = SessionLogger::new(kiln_path);
    let sessions = logger3.list_sessions().await;

    assert_eq!(sessions.len(), 2);
    // Both sessions should be in the list
    assert!(sessions.contains(&id1), "Session 1 should be in the list");
    assert!(sessions.contains(&id2), "Session 2 should be in the list");
}
