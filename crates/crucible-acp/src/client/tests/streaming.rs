use std::io::Write;

use serde_json::json;
use tempfile::NamedTempFile;

use super::{test_path, upsert_tool_info};
use crate::client::types::{ClientConfig, StreamingState};
use crate::client::CrucibleAcpClient;
use crucible_core::types::acp::ToolCallInfo;

#[test]
fn streaming_state_merges_chunks_without_newlines() {
    let mut state = StreamingState::default();
    state.append_text("I'll rea");
    state.append_text("d a few notes from the kiln.");

    assert_eq!(
        state.formatted_output(),
        "I'll read a few notes from the kiln."
    );
}

#[test]
fn streaming_state_adds_padding_after_tools() {
    let config = ClientConfig {
        agent_path: test_path("agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();
    state.append_text("First chunk");

    let tool_call = ToolCallInfo::new("test_tool");
    client.record_tool_call(tool_call, &mut state);
    state.append_text("Response after the tool call.");

    assert_eq!(
        state.formatted_output(),
        "First chunk\n\n  ▷ Test Tool()\n\nResponse after the tool call."
    );
}

#[test]
fn tool_call_indents_after_text() {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    state.append_text("Hello");

    client.record_tool_call(
        ToolCallInfo::new("mcp__crucible__read_note")
            .with_id("tool-1")
            .with_arguments(json!({"path": "PRIME"})),
        &mut state,
    );

    state.append_text("World");

    let output = state.formatted_output();
    // Tool block has blank line before and after
    assert!(output.contains("Hello\n\n  ▷ Read Note"));
    assert!(output.contains("\n\nWorld"));
}

#[test]
fn tool_call_updates_existing_entry() {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    client.record_tool_call(
        ToolCallInfo::new("mcp__crucible__read_note")
            .with_id("tool-42")
            .with_arguments(json!({"path": "PRIME"})),
        &mut state,
    );

    client.record_tool_call(
        ToolCallInfo::new("mcp__crucible__read_note")
            .with_id("tool-42")
            .with_arguments(json!({"path": "PRIME.md"})),
        &mut state,
    );

    let output = state.formatted_output();
    assert_eq!(output.matches("▷ Read Note").count(), 1);
    assert!(output.contains("PRIME.md"));
}

#[test]
fn test_formatted_output_includes_diff() {
    let config = ClientConfig {
        agent_path: test_path("agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    // Create a temp file with initial content
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "old content").unwrap();
    let path = temp_file.path().to_string_lossy().to_string();

    // Record a write tool call
    client.record_tool_call(
        ToolCallInfo::new("update_note")
            .with_id("tool-1")
            .with_arguments(json!({
                "path": path,
                "content": "new content\n"
            })),
        &mut state,
    );

    let output = state.formatted_output();
    assert!(output.contains("▷ Update Note"), "Should have tool label");
    assert!(
        output.contains("-old content"),
        "Should show deleted line in diff"
    );
    assert!(
        output.contains("+new content"),
        "Should show inserted line in diff"
    );
}

// =========================================================================
// RED Tests: StreamingState Formatting Edge Cases
// These tests are designed to expose formatting issues (TDD approach)
// =========================================================================

#[test]
fn test_streaming_state_empty_text_handling() {
    // RED: Verify whitespace-only chunks don't create spurious newlines
    let mut state = StreamingState::default();
    state.append_text("Hello");
    state.append_text("   "); // whitespace only - should be ignored
    state.append_text("World");

    let output = state.formatted_output();
    // Whitespace-only text is ignored by append_text, so Hello and World
    // should be concatenated without extra spacing
    assert!(
        !output.contains("\n\n"),
        "Should not have double newlines from whitespace: {:?}",
        output
    );
    assert_eq!(output.trim(), "HelloWorld");
}

#[test]
fn test_streaming_state_consecutive_tools_no_double_spacing() {
    // RED: Multiple consecutive tools should be in one block with single spacing
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    client.record_tool_call(ToolCallInfo::new("tool1").with_id("t1"), &mut state);
    client.record_tool_call(ToolCallInfo::new("tool2").with_id("t2"), &mut state);
    client.record_tool_call(ToolCallInfo::new("tool3").with_id("t3"), &mut state);

    let output = state.formatted_output();
    // Should only have one blank line before the tool block, not between each tool
    // The tool block should have format: "\n\n  ▷ tool1()\n  ▷ tool2()\n  ▷ tool3()\n\n"
    let tool_section: &str = output.trim();
    let blank_line_pairs = tool_section.matches("\n\n").count();
    assert!(
        blank_line_pairs <= 1,
        "Should have max 1 blank line separator at start, got {} in: {:?}",
        blank_line_pairs,
        output
    );
}

#[test]
fn test_streaming_state_text_tool_text_formatting() {
    // RED: Text -> Tools -> Text should have proper separation
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    state.append_text("Before tools\n");
    client.record_tool_call(
        ToolCallInfo::new("read_file")
            .with_id("t1")
            .with_arguments(json!({"path": "test.md"})),
        &mut state,
    );
    state.append_text("After tools");

    let output = state.formatted_output();
    assert!(
        output.contains("Before tools"),
        "Should contain text before tools"
    );
    assert!(output.contains("▷"), "Should contain tool indicator");
    assert!(
        output.contains("After tools"),
        "Should contain text after tools"
    );
    // Verify proper blank line separation before tool block
    assert!(
        output.contains("\n\n  ▷"),
        "Tool block should have blank line before it: {:?}",
        output
    );
}

#[test]
fn test_tool_deduplication_different_ids_same_args() {
    // RED: Same tool+args but different IDs should both be recorded
    let mut state = StreamingState::default();

    let tool1 = ToolCallInfo::new("read_file")
        .with_id("call-1")
        .with_arguments(json!({"path": "test.md"}));
    let tool2 = ToolCallInfo::new("read_file")
        .with_id("call-2")
        .with_arguments(json!({"path": "test.md"}));

    // Use upsert_tool_info directly to test deduplication logic
    // (record_tool_call also modifies segments, we want to isolate the dedup logic)
    upsert_tool_info(tool1, &mut state);
    upsert_tool_info(tool2, &mut state);

    assert_eq!(
        state.tool_calls.len(),
        2,
        "Both tool calls should be recorded (different IDs)"
    );
}

#[test]
fn test_tool_deduplication_same_id_updates() {
    // Verify that same ID correctly updates existing entry
    let mut state = StreamingState::default();

    let tool1 = ToolCallInfo::new("read_file")
        .with_id("same-id")
        .with_arguments(json!({"path": "old.md"}));
    let tool2 = ToolCallInfo::new("read_file")
        .with_id("same-id")
        .with_arguments(json!({"path": "new.md"}));

    upsert_tool_info(tool1, &mut state);
    upsert_tool_info(tool2, &mut state);

    assert_eq!(
        state.tool_calls.len(),
        1,
        "Same ID should update, not duplicate"
    );
    // Should have the updated arguments
    let args = state.tool_calls[0].arguments.as_ref().unwrap();
    assert_eq!(
        args.get("path").and_then(|v| v.as_str()),
        Some("new.md"),
        "Arguments should be updated to new values"
    );
}
