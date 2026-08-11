//! End-to-end tests for session logging and markdown export
//!
//! Tests the full pipeline:
//! 1. Write a session log (JSONL)
//! 2. Load events back (JSONL roundtrip)
//! 3. Export to Markdown (both imperative and serde-based)
//! 4. Verify content correctness

use chrono::Utc;
use crucible_daemon::{
    events::TokenUsage, load_events, render_to_markdown, serde_md, LogEvent, RenderOptions,
    SessionId, SessionType,
};
use serde_json::json;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Create a sample conversation for testing
fn sample_conversation() -> Vec<LogEvent> {
    vec![
        LogEvent::system("You are a helpful coding assistant."),
        LogEvent::user("How do I read a file in Rust?"),
        LogEvent::assistant_with_model(
            "You can use std::fs::read_to_string() to read a file as a String:\n\n```rust\nuse std::fs;\n\nlet content = fs::read_to_string(\"path/to/file.txt\")?;\nprintln!(\"{}\", content);\n```\n\nThis will return the entire file contents as a String.",
            "claude-3-haiku",
            Some(TokenUsage {
                prompt_tokens: 25,
                completion_tokens: 75,
                total_tokens: 100,
                cache_read_tokens: Some(12),
                cache_creation_tokens: None,
            }),
        ),
        LogEvent::user("Can you show me with an actual file?"),
        LogEvent::tool_call("tc_001", "read_file", json!({"path": "Cargo.toml"})),
        LogEvent::tool_result("tc_001", "[package]\nname = \"example\"\nversion = \"0.1.0\""),
        LogEvent::assistant("Here's what I found in Cargo.toml:\n\nThe file contains basic package metadata."),
    ]
}

/// Write a session log directly. `SessionWriter` used to do this, but it
/// had no production callers, so every test that used it was round-tripping
/// against a format the daemon never produced.
async fn write_session_log(sessions_dir: &Path, lines: &[String]) -> PathBuf {
    let id = SessionId::new(SessionType::Chat, Utc::now());
    let session_dir = sessions_dir.join(id.as_str());
    tokio::fs::create_dir_all(&session_dir).await.unwrap();
    tokio::fs::write(session_dir.join("session.jsonl"), lines.join("\n") + "\n")
        .await
        .unwrap();
    session_dir
}

/// `LogEvent::to_jsonl` for a whole conversation. These tests cover the `View`
/// branch of the reader and the markdown renderers; both stay in scope, so the
/// events stay `LogEvent` and are serialized at the call site.
fn as_jsonl(events: &[LogEvent]) -> Vec<String> {
    events.iter().map(|e| e.to_jsonl().unwrap()).collect()
}

#[tokio::test]
async fn test_jsonl_roundtrip() {
    let dir = TempDir::new().unwrap();
    let sessions_dir = dir.path().join("sessions");

    let original_events = sample_conversation();
    let session_dir = write_session_log(&sessions_dir, &as_jsonl(&original_events)).await;

    // Load events back
    let loaded_events = load_events(&session_dir).await.unwrap();

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

    let session_dir = write_session_log(&sessions_dir, &as_jsonl(&sample_conversation())).await;

    // Load and render to markdown
    let events = load_events(&session_dir).await.unwrap();
    let md = render_to_markdown(&events, &RenderOptions::default());

    // Verify markdown structure
    assert!(
        md.contains("[!system]-"),
        "Should have system prompt callout"
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

    let session_dir = write_session_log(&sessions_dir, &as_jsonl(&sample_conversation())).await;

    // Load and render via serde_md
    let events = load_events(&session_dir).await.unwrap();
    let md = serde_md::to_string_seq(&events).unwrap();

    // Verify markdown structure (serde variant)
    assert!(
        md.contains("[!system]-"),
        "Should have system prompt callout"
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
    let session_dir = write_session_log(
        &sessions_dir,
        &as_jsonl(&[
            LogEvent::system("System prompt"),
            LogEvent::user("Initial message"),
        ]),
    )
    .await;

    let jsonl_path = session_dir.join("session.jsonl");
    assert_eq!(
        tokio::fs::read_to_string(&jsonl_path)
            .await
            .unwrap()
            .lines()
            .count(),
        2
    );

    // Append (simulating resume). This is what `append_event`
    // (`session_storage.rs`) does: open for append, write one line.
    let resumed = LogEvent::assistant("Resumed response").to_jsonl().unwrap();
    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .open(&jsonl_path)
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut file, format!("{resumed}\n").as_bytes())
        .await
        .unwrap();
    drop(file);

    // Verify full session
    let events = load_events(&session_dir).await.unwrap();
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

    let session_dir = write_session_log(
        &sessions_dir,
        &as_jsonl(&[
            LogEvent::error("Rate limited", true),
            LogEvent::error("Connection lost", false),
        ]),
    )
    .await;

    // Load and verify
    let events = load_events(&session_dir).await.unwrap();

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

    let session_dir = write_session_log(
        &sessions_dir,
        &as_jsonl(&[LogEvent::tool_result_truncated(
            "tc_002",
            "...partial content...",
            50000,
        )]),
    )
    .await;

    // Load and verify truncated flag preserved
    let events = load_events(&session_dir).await.unwrap();

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

    let session_dir = write_session_log(&sessions_dir, &as_jsonl(&sample_conversation())).await;

    // Read raw file content
    let jsonl_content = tokio::fs::read_to_string(session_dir.join("session.jsonl"))
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

/// The `Wire` branch, end to end through the renderer.
#[tokio::test]
async fn wire_format_log_renders_to_markdown() {
    let dir = TempDir::new().unwrap();
    let sessions_dir = dir.path().join("sessions");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fixtures/session_log_wire.jsonl");
    let lines: Vec<String> = std::fs::read_to_string(&fixture)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let session_dir = write_session_log(&sessions_dir, &lines).await;

    let events = load_events(&session_dir).await.unwrap();
    let md = render_to_markdown(&events, &RenderOptions::default());

    assert!(md.contains("## User"), "{md}");
    assert!(md.contains("how do I read a file"), "{md}");
    assert!(md.contains("Use std::fs::read_to_string."), "{md}");
    assert!(md.contains("*Tokens: 25 in, 75 out, 12 cached*"), "{md}");
}
