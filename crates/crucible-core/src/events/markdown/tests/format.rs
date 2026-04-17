use serde_json::json;
use std::path::PathBuf;

use super::{test_path, TEST_TIMESTAMP_MS};
use crate::events::markdown::format::{days_to_ymd, format_timestamp, is_leap_year, quote_content};
use crate::events::{InternalSessionEvent, SessionEvent, SessionEventConfig, ToolCall};

#[test]
fn test_format_timestamp() {
    let ts = format_timestamp(Some(TEST_TIMESTAMP_MS));
    assert_eq!(ts, "2025-12-14T15:30:45.123");
}

#[test]
fn test_format_timestamp_epoch() {
    let ts = format_timestamp(Some(0));
    assert_eq!(ts, "1970-01-01T00:00:00.000");
}

#[test]
fn test_days_to_ymd_epoch() {
    let (y, m, d) = days_to_ymd(0);
    assert_eq!((y, m, d), (1970, 1, 1));
}

#[test]
fn test_days_to_ymd_leap_year() {
    // 2000-03-01 - this tests leap year handling
    // Days from 1970-01-01 to 2000-03-01
    // 30 years: 10957 days (accounting for leap years)
    let (y, m, d) = days_to_ymd(11017); // 2000-03-01
    assert_eq!(y, 2000);
    assert_eq!(m, 3);
    assert_eq!(d, 1);
}

#[test]
fn test_is_leap_year() {
    assert!(!is_leap_year(1970));
    assert!(is_leap_year(2000)); // divisible by 400
    assert!(!is_leap_year(1900)); // divisible by 100 but not 400
    assert!(is_leap_year(2024)); // divisible by 4
    assert!(!is_leap_year(2023));
}

#[test]
fn test_quote_content_single_line() {
    let quoted = quote_content("Hello world");
    assert_eq!(quoted, "> Hello world");
}

#[test]
fn test_quote_content_multiline() {
    let quoted = quote_content("Line 1\nLine 2\nLine 3");
    assert_eq!(quoted, "> Line 1\n> Line 2\n> Line 3");
}

#[test]
fn test_quote_content_empty() {
    let quoted = quote_content("");
    assert_eq!(quoted, "");
}

// ─────────────────────────────────────────────────────────────────────────
// Event type name tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_event_type_name_message_received() {
    let event = SessionEvent::MessageReceived {
        content: "test".into(),
        participant_id: "user".into(),
    };
    assert_eq!(event.event_type_name(), "MessageReceived");
}

#[test]
fn test_event_type_name_tool_called() {
    let event = SessionEvent::ToolCalled {
        name: "test".into(),
        args: json!({}),
        description: None,
        source: None,
    };
    assert_eq!(event.event_type_name(), "ToolCalled");
}

#[test]
fn test_event_type_name_tool_completed() {
    let event = SessionEvent::ToolCompleted {
        name: "test".into(),
        result: "".into(),
        error: None,
    };
    assert_eq!(event.event_type_name(), "ToolCompleted");
}

// ─────────────────────────────────────────────────────────────────────────
// Full markdown conversion tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn message_event_to_markdown() {
    let event = SessionEvent::MessageReceived {
        content: "Help me implement the task harness".into(),
        participant_id: "user".into(),
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - MessageReceived"));
    assert!(md.contains("**Participant:** user"));
    assert!(md.contains("> Help me implement the task harness"));
    assert!(md.ends_with("---\n"));
}

#[test]
fn message_event_multiline_content() {
    let event = SessionEvent::MessageReceived {
        content: "Line 1\nLine 2\nLine 3".into(),
        participant_id: "assistant".into(),
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("> Line 1\n> Line 2\n> Line 3"));
}

#[test]
fn tool_event_to_markdown() {
    let path = test_path("test.txt");
    let path_str = path.to_string_lossy();
    let event = SessionEvent::ToolCalled {
        name: "read_file".into(),
        args: json!({"path": path_str}),
        description: None,
        source: None,
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - ToolCalled"));
    assert!(md.contains("**Tool:** `read_file`"));
    assert!(md.contains("**Arguments:**"));
    assert!(md.contains(&format!("\"path\": \"{}\"", path_str)));
    assert!(md.ends_with("---\n"));
}

#[test]
fn tool_completed_event_to_markdown() {
    let event = SessionEvent::ToolCompleted {
        name: "read_file".into(),
        result: "File contents here".into(),
        error: None,
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - ToolCompleted"));
    assert!(md.contains("**Tool:** `read_file`"));
    assert!(md.contains("**Result:** File contents here"));
    assert!(!md.contains("**Error:**"));
}

#[test]
fn tool_completed_with_error_to_markdown() {
    let event = SessionEvent::ToolCompleted {
        name: "read_file".into(),
        result: "".into(),
        error: Some("File not found".into()),
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("**Error:** File not found"));
}

#[test]
fn tool_completed_long_result_to_markdown() {
    let long_result = "Line 1\nLine 2\nLine 3\nMore content here that spans multiple lines";
    let event = SessionEvent::ToolCompleted {
        name: "search".into(),
        result: long_result.into(),
        error: None,
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    // Long results should be in code blocks
    assert!(md.contains("**Result:**\n```\n"));
    assert!(md.contains(long_result));
    assert!(md.contains("\n```\n"));
}

#[test]
fn agent_responded_to_markdown() {
    let path = test_path("test.txt");
    let path_str = path.to_string_lossy();
    let event = SessionEvent::AgentResponded {
        content: "I'll help you with that.".into(),
        tool_calls: vec![ToolCall::new("read_file", json!({"path": path_str}))],
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - AgentResponded"));
    assert!(md.contains("**Content:**"));
    assert!(md.contains("I'll help you with that."));
    assert!(md.contains("**Tool Calls:**"));
    assert!(md.contains(&format!("- `read_file`: `{{\"path\":\"{}\"}}`", path_str)));
}

#[test]
fn agent_responded_with_call_id_to_markdown() {
    let event = SessionEvent::AgentResponded {
        content: "".into(),
        tool_calls: vec![
            ToolCall::new("search", json!({"query": "test"})).with_call_id("call_123")
        ],
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("- `search` (id: call_123):"));
}

#[test]
fn agent_thinking_to_markdown() {
    let event = SessionEvent::AgentThinking {
        thought: "Analyzing the codebase...".into(),
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - AgentThinking"));
    assert!(md.contains("*Analyzing the codebase...*"));
}

#[test]
fn session_started_to_markdown() {
    let event = SessionEvent::SessionStarted {
        config: SessionEventConfig::new("2025-12-14T1530-task")
            .with_folder("/kiln/Sessions/2025-12-14T1530-task")
            .with_max_context_tokens(100_000),
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - SessionStarted"));
    assert!(md.contains("**Session ID:** `2025-12-14T1530-task`"));
    assert!(md.contains("**Folder:** `/kiln/Sessions/2025-12-14T1530-task`"));
}

#[test]
fn session_compacted_to_markdown() {
    let event = SessionEvent::internal(InternalSessionEvent::SessionCompacted {
        summary: "Discussed task harness implementation.".into(),
        new_file: PathBuf::from("/kiln/Sessions/test/001-context.md"),
    });

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - SessionCompacted"));
    assert!(md.contains("**New File:** `/kiln/Sessions/test/001-context.md`"));
    assert!(md.contains("**Summary:**"));
    assert!(md.contains("Discussed task harness implementation."));
}

#[test]
fn session_ended_to_markdown() {
    let event = SessionEvent::SessionEnded {
        reason: "User closed session".into(),
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - SessionEnded"));
    assert!(md.contains("**Reason:** User closed session"));
}

#[test]
fn subagent_spawned_to_markdown() {
    let event = SessionEvent::internal(InternalSessionEvent::SubagentSpawned {
        id: "sub_abc123".into(),
        prompt: "Find all files related to task harness".into(),
    });

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - SubagentSpawned"));
    assert!(md.contains("**Subagent ID:** `sub_abc123`"));
    assert!(md.contains("**Prompt:**"));
    assert!(md.contains("> Find all files related to task harness"));
}

#[test]
fn subagent_completed_to_markdown() {
    let event = SessionEvent::internal(InternalSessionEvent::SubagentCompleted {
        id: "sub_abc123".into(),
        result: "Found 5 files.".into(),
    });

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - SubagentCompleted"));
    assert!(md.contains("**Subagent ID:** `sub_abc123`"));
    assert!(md.contains("**Result:**"));
    assert!(md.contains("> Found 5 files."));
}

#[test]
fn subagent_failed_to_markdown() {
    let event = SessionEvent::internal(InternalSessionEvent::SubagentFailed {
        id: "sub_abc123".into(),
        error: "Timeout exceeded".into(),
    });

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - SubagentFailed"));
    assert!(md.contains("**Subagent ID:** `sub_abc123`"));
    assert!(md.contains("**Error:** Timeout exceeded"));
}

#[test]
fn bash_task_spawned_to_markdown() {
    let event = SessionEvent::internal(InternalSessionEvent::BashTaskSpawned {
        id: "task-20250123-1830-abc123".into(),
        command: "cargo build --release".into(),
    });

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - BashTaskSpawned"));
    assert!(md.contains("**Task ID:** `task-20250123-1830-abc123`"));
    assert!(md.contains("**Command:**"));
    assert!(md.contains("cargo build --release"));
}

#[test]
fn bash_task_completed_to_markdown() {
    let event = SessionEvent::internal(InternalSessionEvent::BashTaskCompleted {
        id: "task-20250123-1830-abc123".into(),
        output: "Build succeeded\n".into(),
        exit_code: 0,
    });

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - BashTaskCompleted"));
    assert!(md.contains("**Task ID:** `task-20250123-1830-abc123`"));
    assert!(md.contains("**Exit Code:** 0"));
    assert!(md.contains("**Output:**"));
    assert!(md.contains("Build succeeded"));
}

#[test]
fn bash_task_failed_to_markdown() {
    let event = SessionEvent::internal(InternalSessionEvent::BashTaskFailed {
        id: "task-20250123-1830-abc123".into(),
        error: "Command not found".into(),
        exit_code: Some(127),
    });

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - BashTaskFailed"));
    assert!(md.contains("**Task ID:** `task-20250123-1830-abc123`"));
    assert!(md.contains("**Exit Code:** 127"));
    assert!(md.contains("**Error:** Command not found"));
}

#[test]
fn bash_task_failed_no_exit_code_to_markdown() {
    let event = SessionEvent::internal(InternalSessionEvent::BashTaskFailed {
        id: "task-20250123-1830-abc123".into(),
        error: "Timeout".into(),
        exit_code: None,
    });

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("**Exit Code:** none"));
}

#[test]
fn background_task_completed_to_markdown() {
    let event = SessionEvent::internal(InternalSessionEvent::BackgroundTaskCompleted {
        id: "task-20250123-1830-abc123".into(),
        kind: "bash".into(),
        summary: "Build completed successfully".into(),
    });

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - BackgroundTaskCompleted"));
    assert!(md.contains("**Task ID:** `task-20250123-1830-abc123`"));
    assert!(md.contains("**Kind:** bash"));
    assert!(md.contains("**Summary:**"));
    assert!(md.contains("> Build completed successfully"));
}

#[test]
fn custom_event_to_markdown() {
    let event = SessionEvent::Custom {
        name: "my_custom_event".into(),
        payload: json!({"key": "value", "count": 42}),
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("## 2025-12-14T15:30:45.123 - Custom"));
    assert!(md.contains("**Event Name:** `my_custom_event`"));
    assert!(md.contains("**Payload:**"));
    assert!(md.contains("\"key\": \"value\""));
    assert!(md.contains("\"count\": 42"));
}

// ─────────────────────────────────────────────────────────────────────────
// Edge cases
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn event_with_empty_content() {
    let event = SessionEvent::MessageReceived {
        content: "".into(),
        participant_id: "user".into(),
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    // Should still have structure, just no quoted content
    assert!(md.contains("**Participant:** user"));
}

#[test]
fn agent_responded_empty_content_with_tools() {
    let event = SessionEvent::AgentResponded {
        content: "".into(),
        tool_calls: vec![ToolCall::new("test", json!({}))],
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    // Should have tool calls but no content section
    assert!(!md.contains("**Content:**"));
    assert!(md.contains("**Tool Calls:**"));
}

#[test]
fn agent_responded_no_tools() {
    let event = SessionEvent::AgentResponded {
        content: "Just text".into(),
        tool_calls: vec![],
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.contains("**Content:**"));
    assert!(!md.contains("**Tool Calls:**"));
}

#[test]
fn markdown_ends_with_separator() {
    let event = SessionEvent::SessionEnded {
        reason: "done".into(),
    };

    let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

    assert!(md.ends_with("---\n"), "Markdown should end with separator");
}
