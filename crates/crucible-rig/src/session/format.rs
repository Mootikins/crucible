//! Markdown formatting for session logs
//!
//! This module provides pure functions for formatting session events
//! as markdown. No I/O is performed - these functions just convert
//! data structures to markdown strings.

use super::types::{SessionMetadata, Task, TaskStatus};
use chrono::{DateTime, Utc};
use serde_json::Value;

/// Format session metadata as YAML frontmatter
///
/// # Example Output
///
/// ```yaml
/// ---
/// type: session
/// workspace: crucible
/// started: 2024-12-24T19:30:00Z
/// ---
/// ```
pub fn format_frontmatter(meta: &SessionMetadata) -> String {
    let mut lines = vec![
        "---".to_string(),
        "type: session".to_string(),
        format!("workspace: {}", meta.workspace),
        format!("started: {}", meta.started.format("%Y-%m-%dT%H:%M:%SZ")),
    ];

    if let Some(ref ended) = meta.ended {
        lines.push(format!("ended: {}", ended.format("%Y-%m-%dT%H:%M:%SZ")));
    }

    if let Some(ref continued_from) = meta.continued_from {
        lines.push(format!("continued_from: {}", continued_from));
    }

    lines.push("---".to_string());

    let mut result = lines.join("\n");
    result.push_str("\n\n"); // Empty line after frontmatter
    result
}

/// Format a user message as markdown
///
/// # Example Output
///
/// ```markdown
/// ### User 19:30
///
/// Hello world
///
/// ```
pub fn format_user_message(content: &str, ts: DateTime<Utc>) -> String {
    format!("### User {}\n\n{}\n\n", ts.format("%H:%M"), content)
}

/// Format an agent response as markdown
///
/// # Example Output
///
/// ```markdown
/// ### Agent 19:30:15
///
/// Hi there!
///
/// ```
pub fn format_agent_response(content: &str, ts: DateTime<Utc>) -> String {
    format!("### Agent {}\n\n{}\n\n", ts.format("%H:%M:%S"), content)
}

/// Format a tool call as markdown
///
/// # Example Output
///
/// ```markdown
/// ### Tool: semantic_search 19:30:20
///
/// **Arguments:**
/// ```json
/// {"query": "rust notes", "limit": 5}
/// ```
///
/// **Result:**
/// ```json
/// {"count": 3, "notes": ["a", "b", "c"]}
/// ```
///
/// ```
pub fn format_tool_call(name: &str, args: &Value, result: &Value, ts: DateTime<Utc>) -> String {
    let args_pretty = serde_json::to_string_pretty(args).unwrap_or_else(|_| args.to_string());
    let result_pretty = serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string());

    format!(
        "### Tool: {} {}\n\n**Arguments:**\n```json\n{}\n```\n\n**Result:**\n```json\n{}\n```\n\n",
        name,
        ts.format("%H:%M:%S"),
        args_pretty,
        result_pretty
    )
}

/// Format a task list as markdown checkboxes
///
/// # Status Markers
///
/// - `[x]` - Completed
/// - `[~]` - In Progress
/// - `[ ]` - Pending
///
/// # Example Output
///
/// ```markdown
/// ## Tasks
///
/// - [x] Done
/// - [~] Working
/// - [ ] Todo
///
/// ```
pub fn format_task_list(tasks: &[Task]) -> String {
    if tasks.is_empty() {
        return String::new();
    }

    let mut lines = vec!["## Tasks".to_string(), String::new()];

    for task in tasks {
        let marker = match task.status {
            TaskStatus::Completed => "[x]",
            TaskStatus::InProgress => "[~]",
            TaskStatus::Pending => "[ ]",
        };
        lines.push(format!("- {} {}", marker, task.content));
    }

    lines.push(String::new()); // Empty line after task list
    lines.join("\n")
}

/// Format a complete session as markdown
///
/// This combines frontmatter, messages, and tasks into a complete
/// markdown document suitable for saving to disk.
#[allow(clippy::type_complexity)]
pub fn format_session(
    meta: &SessionMetadata,
    messages: &[(
        String,
        DateTime<Utc>,
        super::types::MessageRole,
        Option<(&str, &Value, &Value)>,
    )],
    tasks: &[Task],
) -> String {
    use super::types::MessageRole;

    let mut output = format_frontmatter(meta);
    output.push_str("# Session Log\n\n");

    for (content, ts, role, tool_info) in messages {
        match role {
            MessageRole::User => {
                output.push_str(&format_user_message(content, *ts));
            }
            MessageRole::Assistant => {
                output.push_str(&format_agent_response(content, *ts));
            }
            MessageRole::Tool | MessageRole::Function => {
                if let Some((name, args, result)) = tool_info {
                    output.push_str(&format_tool_call(name, args, result, *ts));
                }
            }
            MessageRole::System => {
                // System messages are typically not shown in session logs
                // but include them as blockquotes if present
                output.push_str(&format!(
                    "> **System** ({})\n> {}\n\n",
                    ts.format("%H:%M"),
                    content
                ));
            }
        }
    }

    output.push_str(&format_task_list(tasks));

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn test_timestamp(hour: u32, min: u32, sec: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 12, 24, hour, min, sec).unwrap()
    }

    #[test]
    fn test_format_user_message() {
        let ts = test_timestamp(19, 30, 0);
        let md = format_user_message("Hello world", ts);
        assert_eq!(md, "### User 19:30\n\nHello world\n\n");
    }

    #[test]
    fn test_format_user_message_multiline() {
        let ts = test_timestamp(10, 15, 0);
        let content = "First line\nSecond line\nThird line";
        let md = format_user_message(content, ts);
        assert!(md.contains("### User 10:15"));
        assert!(md.contains("First line\nSecond line\nThird line"));
    }

    #[test]
    fn test_format_agent_response() {
        let ts = test_timestamp(19, 30, 15);
        let md = format_agent_response("Hi there!", ts);
        assert_eq!(md, "### Agent 19:30:15\n\nHi there!\n\n");
    }

    #[test]
    fn test_format_agent_response_with_code() {
        let ts = test_timestamp(14, 22, 33);
        let content = "Here's some code:\n\n```rust\nfn main() {}\n```";
        let md = format_agent_response(content, ts);
        assert!(md.contains("### Agent 14:22:33"));
        assert!(md.contains("```rust"));
        assert!(md.contains("fn main()"));
    }

    #[test]
    fn test_format_tool_call() {
        let ts = test_timestamp(19, 30, 20);
        let args = serde_json::json!({"query": "rust notes", "limit": 5});
        let result = serde_json::json!({"count": 3, "notes": ["a", "b", "c"]});
        let md = format_tool_call("semantic_search", &args, &result, ts);

        assert!(md.contains("### Tool: semantic_search 19:30:20"));
        assert!(md.contains("**Arguments:**"));
        assert!(md.contains("```json"));
        assert!(md.contains("\"query\": \"rust notes\""));
        assert!(md.contains("**Result:**"));
        assert!(md.contains("\"count\": 3"));
    }

    #[test]
    fn test_format_tool_call_complex_result() {
        let ts = test_timestamp(8, 0, 0);
        let args = serde_json::json!({"path": "/notes/index.md"});
        let result = serde_json::json!({
            "content": "# Index\n\nThis is the index.",
            "metadata": {
                "title": "Index",
                "tags": ["meta", "navigation"]
            }
        });
        let md = format_tool_call("read_note", &args, &result, ts);

        assert!(md.contains("### Tool: read_note 08:00:00"));
        assert!(md.contains("\"path\": \"/notes/index.md\""));
        assert!(md.contains("\"title\": \"Index\""));
    }

    #[test]
    fn test_format_frontmatter() {
        let meta = SessionMetadata {
            workspace: "crucible".into(),
            started: test_timestamp(19, 30, 0),
            ended: None,
            continued_from: None,
        };
        let fm = format_frontmatter(&meta);

        assert!(fm.starts_with("---\n"));
        assert!(fm.contains("type: session"));
        assert!(fm.contains("workspace: crucible"));
        assert!(fm.contains("started: 2024-12-24T19:30:00Z"));
        assert!(fm.ends_with("---\n\n"));
        assert!(!fm.contains("ended:"));
        assert!(!fm.contains("continued_from:"));
    }

    #[test]
    fn test_format_frontmatter_with_ended() {
        let meta = SessionMetadata {
            workspace: "my-project".into(),
            started: test_timestamp(10, 0, 0),
            ended: Some(test_timestamp(12, 30, 45)),
            continued_from: None,
        };
        let fm = format_frontmatter(&meta);

        assert!(fm.contains("started: 2024-12-24T10:00:00Z"));
        assert!(fm.contains("ended: 2024-12-24T12:30:45Z"));
    }

    #[test]
    fn test_format_frontmatter_with_continuation() {
        let meta = SessionMetadata {
            workspace: "crucible".into(),
            started: test_timestamp(14, 0, 0),
            ended: None,
            continued_from: Some("crucible/2024-12-24_1000".into()),
        };
        let fm = format_frontmatter(&meta);

        assert!(fm.contains("continued_from: crucible/2024-12-24_1000"));
    }

    #[test]
    fn test_format_task_list() {
        let tasks = vec![
            Task {
                content: "Done".into(),
                status: TaskStatus::Completed,
            },
            Task {
                content: "Working".into(),
                status: TaskStatus::InProgress,
            },
            Task {
                content: "Todo".into(),
                status: TaskStatus::Pending,
            },
        ];
        let md = format_task_list(&tasks);

        assert!(md.contains("## Tasks"));
        assert!(md.contains("- [x] Done"));
        assert!(md.contains("- [~] Working"));
        assert!(md.contains("- [ ] Todo"));
    }

    #[test]
    fn test_format_task_list_empty() {
        let tasks: Vec<Task> = vec![];
        let md = format_task_list(&tasks);
        assert!(md.is_empty());
    }

    #[test]
    fn test_format_task_list_single() {
        let tasks = vec![Task {
            content: "Single task".into(),
            status: TaskStatus::InProgress,
        }];
        let md = format_task_list(&tasks);

        assert!(md.contains("## Tasks"));
        assert!(md.contains("- [~] Single task"));
    }

    #[test]
    fn test_format_session_complete() {
        use super::super::types::MessageRole;

        let meta = SessionMetadata {
            workspace: "test".into(),
            started: test_timestamp(10, 0, 0),
            ended: None,
            continued_from: None,
        };

        let args = serde_json::json!({"query": "test"});
        let result = serde_json::json!({"found": true});

        let messages: Vec<(
            String,
            DateTime<Utc>,
            MessageRole,
            Option<(&str, &Value, &Value)>,
        )> = vec![
            (
                "Hello".into(),
                test_timestamp(10, 0, 0),
                MessageRole::User,
                None,
            ),
            (
                "Hi!".into(),
                test_timestamp(10, 0, 5),
                MessageRole::Assistant,
                None,
            ),
            (
                "".into(),
                test_timestamp(10, 0, 10),
                MessageRole::Tool,
                Some(("search", &args, &result)),
            ),
        ];

        let tasks = vec![Task {
            content: "Test task".into(),
            status: TaskStatus::Completed,
        }];

        let md = format_session(&meta, &messages, &tasks);

        // Check structure
        assert!(md.starts_with("---\n"));
        assert!(md.contains("# Session Log"));
        assert!(md.contains("### User 10:00"));
        assert!(md.contains("Hello"));
        assert!(md.contains("### Agent 10:00:05"));
        assert!(md.contains("Hi!"));
        assert!(md.contains("### Tool: search 10:00:10"));
        assert!(md.contains("## Tasks"));
        assert!(md.contains("- [x] Test task"));
    }
}
