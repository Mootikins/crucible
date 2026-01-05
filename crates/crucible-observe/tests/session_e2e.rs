//! End-to-end tests for session logging and markdown export
//!
//! Tests the full pipeline:
//! 1. Create session → Append events (JSONL)
//! 2. Load events back (JSONL roundtrip)
//! 3. Export to Markdown (both imperative and serde-based)
//! 4. Verify content correctness

use crucible_observe::{
    events::TokenUsage, load_events, render_to_markdown, serde_md, LogEvent, RenderOptions,
    SessionType, SessionWriter,
};
use serde_json::json;
use tempfile::TempDir;

/// Create a sample conversation for testing
fn sample_conversation() -> Vec<LogEvent> {
    vec![
        LogEvent::system("You are a helpful coding assistant."),
        LogEvent::user("How do I read a file in Rust?"),
        LogEvent::assistant_with_model(
            "You can use std::fs::read_to_string() to read a file as a String:\n\n```rust\nuse std::fs;\n\nlet content = fs::read_to_string(\"path/to/file.txt\")?;\nprintln!(\"{}\", content);\n```\n\nThis will return the entire file contents as a String.",
            "claude-3-haiku",
            Some(TokenUsage { input: 25, output: 75 }),
        ),
        LogEvent::user("Can you show me with an actual file?"),
        LogEvent::tool_call("tc_001", "read_file", json!({"path": "Cargo.toml"})),
        LogEvent::tool_result("tc_001", "[package]\nname = \"example\"\nversion = \"0.1.0\""),
        LogEvent::assistant("Here's what I found in Cargo.toml:\n\nThe file contains basic package metadata."),
    ]
}

#[tokio::test]
async fn test_jsonl_roundtrip() {
    let dir = TempDir::new().unwrap();
    let sessions_dir = dir.path().join("sessions");

    // Create session and append events
    let mut writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
        .await
        .unwrap();

    let original_events = sample_conversation();
    for event in &original_events {
        writer.append(event.clone()).await.unwrap();
    }
    writer.flush().await.unwrap();

    // Load events back
    let loaded_events = load_events(writer.session_dir()).await.unwrap();

    // Verify count and content
    assert_eq!(loaded_events.len(), original_events.len());

    // Verify first event is system
    match &loaded_events[0] {
        LogEvent::System { content, .. } => {
            assert!(content.contains("helpful coding assistant"));
        }
        _ => panic!("Expected System event"),
    }

    // Verify tool call preserved
    match &loaded_events[4] {
        LogEvent::ToolCall { name, args, .. } => {
            assert_eq!(name, "read_file");
            assert_eq!(args["path"], "Cargo.toml");
        }
        _ => panic!("Expected ToolCall event"),
    }
}

#[tokio::test]
async fn test_markdown_export_imperative() {
    let dir = TempDir::new().unwrap();
    let sessions_dir = dir.path().join("sessions");

    // Create session with events
    let mut writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
        .await
        .unwrap();

    for event in sample_conversation() {
        writer.append(event).await.unwrap();
    }
    writer.flush().await.unwrap();

    // Load and render to markdown
    let events = load_events(writer.session_dir()).await.unwrap();
    let md = render_to_markdown(&events, &RenderOptions::default());

    // Verify markdown structure
    assert!(
        md.contains("<details>"),
        "Should have system prompt details"
    );
    assert!(md.contains("## User"), "Should have user heading");
    assert!(
        md.contains("## Assistant (claude-3-haiku)"),
        "Should have assistant with model"
    );
    assert!(
        md.contains("### Tool: `read_file`"),
        "Should have tool call"
    );
    assert!(md.contains("#### Result"), "Should have tool result");
    assert!(md.contains("*Tokens:"), "Should have token usage");

    // Verify content order (user before assistant)
    let user1_pos = md.find("How do I read a file").unwrap();
    let asst1_pos = md.find("std::fs::read_to_string").unwrap();
    assert!(user1_pos < asst1_pos, "User should come before assistant");
}

#[tokio::test]
async fn test_markdown_export_serde() {
    let dir = TempDir::new().unwrap();
    let sessions_dir = dir.path().join("sessions");

    // Create session with events
    let mut writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
        .await
        .unwrap();

    for event in sample_conversation() {
        writer.append(event).await.unwrap();
    }
    writer.flush().await.unwrap();

    // Load and render via serde_md
    let events = load_events(writer.session_dir()).await.unwrap();
    let md = serde_md::to_string_seq(&events).unwrap();

    // Verify markdown structure (serde variant)
    assert!(
        md.contains("<details>"),
        "Should have system prompt details"
    );
    assert!(md.contains("## User"), "Should have user heading");
    assert!(md.contains("## Assistant"), "Should have assistant heading");
    assert!(md.contains("### Tool:"), "Should have tool call");
    assert!(md.contains("#### Result"), "Should have tool result");
}

#[tokio::test]
async fn test_session_resume_append() {
    let dir = TempDir::new().unwrap();
    let sessions_dir = dir.path().join("sessions");

    // Create initial session
    let id = {
        let mut writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
            .await
            .unwrap();

        writer
            .append(LogEvent::system("System prompt"))
            .await
            .unwrap();
        writer
            .append(LogEvent::user("Initial message"))
            .await
            .unwrap();
        writer.flush().await.unwrap();

        writer.id().clone()
    };

    // Reopen and append (simulating resume)
    {
        let mut writer = SessionWriter::open(&sessions_dir, id.clone())
            .await
            .unwrap();

        assert_eq!(writer.event_count(), 2);

        writer
            .append(LogEvent::assistant("Resumed response"))
            .await
            .unwrap();
        writer.flush().await.unwrap();

        assert_eq!(writer.event_count(), 3);
    }

    // Verify full session
    let events = load_events(sessions_dir.join(id.as_str())).await.unwrap();
    assert_eq!(events.len(), 3);

    match &events[2] {
        LogEvent::Assistant { content, .. } => {
            assert_eq!(content, "Resumed response");
        }
        _ => panic!("Expected Assistant event"),
    }
}

#[tokio::test]
async fn test_error_event_roundtrip() {
    let dir = TempDir::new().unwrap();
    let sessions_dir = dir.path().join("sessions");

    let mut writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
        .await
        .unwrap();

    // Append error events
    writer
        .append(LogEvent::error("Rate limited", true))
        .await
        .unwrap();
    writer
        .append(LogEvent::error("Connection lost", false))
        .await
        .unwrap();
    writer.flush().await.unwrap();

    // Load and verify
    let events = load_events(writer.session_dir()).await.unwrap();

    match &events[0] {
        LogEvent::Error {
            message,
            recoverable,
            ..
        } => {
            assert_eq!(message, "Rate limited");
            assert!(*recoverable);
        }
        _ => panic!("Expected Error event"),
    }

    match &events[1] {
        LogEvent::Error {
            message,
            recoverable,
            ..
        } => {
            assert_eq!(message, "Connection lost");
            assert!(!recoverable);
        }
        _ => panic!("Expected Error event"),
    }
}

#[tokio::test]
async fn test_tool_truncated_roundtrip() {
    let dir = TempDir::new().unwrap();
    let sessions_dir = dir.path().join("sessions");

    let mut writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
        .await
        .unwrap();

    writer
        .append(LogEvent::tool_result_truncated(
            "tc_002",
            "...partial content...",
            true,
        ))
        .await
        .unwrap();
    writer.flush().await.unwrap();

    // Load and verify truncated flag preserved
    let events = load_events(writer.session_dir()).await.unwrap();

    match &events[0] {
        LogEvent::ToolResult {
            truncated, result, ..
        } => {
            assert!(*truncated);
            assert_eq!(result, "...partial content...");
        }
        _ => panic!("Expected ToolResult event"),
    }

    // Verify markdown indicates truncation
    let md = render_to_markdown(&events, &RenderOptions::default());
    assert!(md.contains("(truncated)"));
}

#[tokio::test]
async fn test_both_markdown_renderers_produce_valid_output() {
    let events = sample_conversation();

    // Imperative renderer
    let md_imperative = render_to_markdown(&events, &RenderOptions::default());

    // Serde-based renderer
    let md_serde = serde_md::to_string_seq(&events).unwrap();

    // Both should contain the essential elements
    for md in [&md_imperative, &md_serde] {
        assert!(md.contains("User"), "Missing user section");
        assert!(md.contains("Assistant"), "Missing assistant section");
        assert!(md.contains("read_file"), "Missing tool call");
        assert!(md.contains("Cargo.toml"), "Missing tool args");
    }
}

#[tokio::test]
async fn test_jsonl_file_is_valid_ndjson() {
    let dir = TempDir::new().unwrap();
    let sessions_dir = dir.path().join("sessions");

    let mut writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
        .await
        .unwrap();

    for event in sample_conversation() {
        writer.append(event).await.unwrap();
    }
    writer.flush().await.unwrap();

    // Read raw file content
    let jsonl_content = tokio::fs::read_to_string(writer.jsonl_path())
        .await
        .unwrap();

    // Each line should be valid JSON
    for (i, line) in jsonl_content.lines().enumerate() {
        if !line.trim().is_empty() {
            let parsed: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("Line {} is not valid JSON: {}", i + 1, e));

            // Should have a "type" field
            assert!(
                parsed.get("type").is_some(),
                "Line {} missing 'type' field",
                i + 1
            );

            // Should have a "ts" field
            assert!(
                parsed.get("ts").is_some(),
                "Line {} missing 'ts' field",
                i + 1
            );
        }
    }
}
