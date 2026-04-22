//! Reconstruct a [`ConversationTree`] from a session JSONL log.
//!
//! Today's session logs record a linear conversation — no fan-out, no
//! branch, no resume-to-earlier-turn. The rebuilder reflects that: it
//! walks the JSONL in order and appends each event as a child of the
//! current leaf, producing a degenerate single-spine tree that matches
//! the session's lived order.
//!
//! If a future `LogEvent` gains optional `node_id` / `parent_id`
//! fields, this module is the place to honour them and rebuild actual
//! branches — scoped as follow-up Phase 1/2 work.

use std::path::Path;

use anyhow::{Context, Result};
use crucible_core::turn::{ConversationTree, NodeContent};
use tokio::fs;

use super::events::LogEvent;

/// Parse a session JSONL file and rebuild its conversation tree.
///
/// Unknown event types are skipped (they're metadata, not
/// conversation content). Parse errors on individual lines are
/// skipped with a warning to keep a partially-corrupt log recoverable.
pub async fn rebuild_tree_from_jsonl(path: &Path) -> Result<ConversationTree> {
    let content = fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read session log at {}", path.display()))?;
    Ok(rebuild_tree_from_str(&content))
}

/// Core rebuilder. Exposed as a pure function for testability.
pub fn rebuild_tree_from_str(jsonl: &str) -> ConversationTree {
    let mut tree = ConversationTree::new();
    for (line_no, line) in jsonl.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<LogEvent>(trimmed) {
            Ok(event) => apply_event_to_tree(&mut tree, &event),
            Err(e) => {
                tracing::warn!(line = line_no + 1, error = %e, "skipping malformed log line");
            }
        }
    }
    tree
}

fn apply_event_to_tree(tree: &mut ConversationTree, event: &LogEvent) {
    match event {
        LogEvent::User { content, .. } => {
            let parent = tree.current();
            tree.add_child_and_advance(
                parent,
                NodeContent::User {
                    text: content.clone(),
                },
            );
        }
        LogEvent::Assistant { content, .. } => {
            let parent = tree.current();
            tree.add_child_and_advance(
                parent,
                NodeContent::Agent {
                    text: content.clone(),
                },
            );
        }
        LogEvent::System { content, .. } => {
            let parent = tree.current();
            tree.add_child_and_advance(
                parent,
                NodeContent::System {
                    text: content.clone(),
                },
            );
        }
        LogEvent::Thinking { content, .. } => {
            let parent = tree.current();
            tree.add_child(
                parent,
                NodeContent::Thinking {
                    text: content.clone(),
                },
            );
        }
        LogEvent::ToolCall { id, name, args, .. } => {
            let parent = tree.current();
            tree.add_child(
                parent,
                NodeContent::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    args: args.clone(),
                },
            );
        }
        LogEvent::ToolResult {
            id, result, error, ..
        } => {
            let parent = tree.current();
            tree.add_child(
                parent,
                NodeContent::ToolResult {
                    id: id.clone(),
                    name: String::new(), // not carried by LogEvent
                    result: serde_json::Value::String(result.clone()),
                    error: error.clone(),
                },
            );
        }
        // Non-conversation events (init, summary, permission, bash, subagent):
        // skip — they're scheduler/observability metadata, not conversation
        // content.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_from_empty_is_just_root() {
        let tree = rebuild_tree_from_str("");
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.current(), tree.root());
    }

    #[test]
    fn rebuild_linear_user_assistant_sequence() {
        let jsonl = r#"{"type":"init","ts":"2026-04-21T00:00:00Z","session_id":"s1"}
{"type":"user","ts":"2026-04-21T00:00:01Z","content":"hello"}
{"type":"assistant","ts":"2026-04-21T00:00:02Z","content":"hi there"}
{"type":"user","ts":"2026-04-21T00:00:03Z","content":"bye"}
{"type":"assistant","ts":"2026-04-21T00:00:04Z","content":"goodbye"}
"#;
        let tree = rebuild_tree_from_str(jsonl);
        let path = tree.path_to_here(tree.current());
        // root + 4 content nodes
        assert_eq!(path.len(), 5, "path = {path:?}");
        assert!(matches!(
            &tree.get(path[1]).content,
            NodeContent::User { text } if text == "hello"
        ));
        assert!(matches!(
            &tree.get(path[2]).content,
            NodeContent::Agent { text } if text == "hi there"
        ));
        assert!(matches!(
            &tree.get(path[4]).content,
            NodeContent::Agent { text } if text == "goodbye"
        ));
    }

    #[test]
    fn rebuild_preserves_tool_call_and_result_siblings() {
        let jsonl = r#"{"type":"user","ts":"2026-04-21T00:00:00Z","content":"search X"}
{"type":"tool_call","ts":"2026-04-21T00:00:01Z","id":"c1","name":"search","args":{"q":"X"}}
{"type":"tool_result","ts":"2026-04-21T00:00:02Z","id":"c1","result":"found"}
{"type":"assistant","ts":"2026-04-21T00:00:03Z","content":"Got it"}
"#;
        let tree = rebuild_tree_from_str(jsonl);
        // user advances current; tool_call + tool_result attach as
        // siblings under user; assistant advances current under user.
        assert_eq!(tree.len(), 5); // root + user + call + result + agent

        // Verify a ToolCall and ToolResult node both exist.
        let has_tool_call = tree
            .iter()
            .any(|(_, n)| matches!(&n.content, NodeContent::ToolCall { name, .. } if name == "search"));
        assert!(has_tool_call, "missing ToolCall node");

        let has_tool_result = tree
            .iter()
            .any(|(_, n)| matches!(&n.content, NodeContent::ToolResult { id, .. } if id == "c1"));
        assert!(has_tool_result, "missing ToolResult node");
    }

    #[test]
    fn rebuild_tolerates_malformed_lines() {
        let jsonl = r#"{"type":"user","ts":"2026-04-21T00:00:00Z","content":"hello"}
this is not json
{"type":"assistant","ts":"2026-04-21T00:00:01Z","content":"hi"}
"#;
        let tree = rebuild_tree_from_str(jsonl);
        assert_eq!(tree.len(), 3); // root + user + agent
    }
}
