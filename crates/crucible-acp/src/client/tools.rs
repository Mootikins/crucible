use agent_client_protocol::ToolCallStatus;

use super::types::{ResponseSegment, StreamingState};
use super::CrucibleAcpClient;
use crate::streaming::humanize_tool_title;
use crucible_core::types::acp::ToolCallInfo;

impl CrucibleAcpClient {
    pub(super) fn extract_tool_result(raw_output: Option<&serde_json::Value>) -> Option<String> {
        raw_output.map(Self::format_json_value)
    }

    pub(super) fn extract_tool_error(
        status: Option<ToolCallStatus>,
        raw_output: Option<&serde_json::Value>,
    ) -> Option<String> {
        let output_error = raw_output
            .and_then(|value| value.get("error"))
            .map(Self::format_json_value)
            .filter(|value| !value.is_empty());

        if output_error.is_some() {
            return output_error;
        }

        if status == Some(ToolCallStatus::Failed) {
            return Some("Tool call failed".to_string());
        }

        None
    }

    fn format_json_value(value: &serde_json::Value) -> String {
        if let Some(s) = value.as_str() {
            s.to_string()
        } else {
            value.to_string()
        }
    }

    pub(super) fn record_tool_call(&self, tool_call: ToolCallInfo, state: &mut StreamingState) {
        let args_str = tool_call
            .arguments
            .as_ref()
            .map(|args| serde_json::to_string(args).unwrap_or_else(|_| "<invalid>".to_string()))
            .unwrap_or_default();

        let formatted_args = if args_str.is_empty() {
            "()".to_string()
        } else {
            format!("({})", args_str)
        };

        let display_title = humanize_tool_title(&tool_call.title);
        let label = format!("▷ {}{}", display_title, formatted_args);
        let id = tool_call
            .id
            .clone()
            .unwrap_or_else(|| format!("{}::{}", tool_call.title, args_str));

        // Generate diff for write operations
        let diff = self.generate_diff_for_write(&tool_call);

        let has_prior_text = matches!(
            state.segments.last(),
            Some(ResponseSegment::Text(last)) if !last.trim().is_empty()
        );
        let _indent = has_prior_text || state.tool_block_active;

        if let Some(&idx) = state.tool_segment_index.get(&id) {
            if let Some(ResponseSegment::Tool {
                label: existing,
                diff: existing_diff,
            }) = state.segments.get_mut(idx)
            {
                *existing = label.clone();
                // Update diff if we have a new one (might have more complete args now)
                if diff.is_some() {
                    *existing_diff = diff.clone();
                }
            }
        } else {
            state
                .tool_segment_index
                .insert(id.clone(), state.segments.len());
            state.segments.push(ResponseSegment::Tool {
                label: label.clone(),
                diff,
            });
        }

        self.upsert_tool_info(tool_call, state);
        state.tool_block_active = true;
    }

    /// Generate a diff for write operations.
    ///
    /// Checks three sources in order:
    /// 1. Pre-computed diffs from protocol (e.g., ACP's ToolCallContent::Diff)
    /// 2. Tool arguments with path + content (for update_note, Write, etc.)
    /// 3. Edit tool arguments with old_string/new_string (find-and-replace)
    pub(super) fn generate_diff_for_write(&self, tool_call: &ToolCallInfo) -> Option<String> {
        use similar::{ChangeTag, TextDiff};

        // Check for pre-computed diffs first (preferred source)
        if !tool_call.diffs.is_empty() {
            let mut output = String::new();
            for diff_entry in &tool_call.diffs {
                if !output.is_empty() {
                    output.push_str("\n--- \n");
                }
                output.push_str(&format!("--- {}\n", diff_entry.path));
                output.push_str(&format!("+++ {}\n", diff_entry.path));

                let old = diff_entry.old_content.as_deref().unwrap_or("");
                let diff = TextDiff::from_lines(old, diff_entry.new_content.as_str());

                for change in diff.iter_all_changes() {
                    let tag = change.tag();
                    let line = change.to_string_lossy();
                    let line_content = line.strip_suffix('\n').unwrap_or(&line);

                    match tag {
                        ChangeTag::Delete => {
                            output.push_str(&format!("-{}\n", line_content));
                        }
                        ChangeTag::Insert => {
                            output.push_str(&format!("+{}\n", line_content));
                        }
                        ChangeTag::Equal => {
                            // Skip unchanged lines to keep output compact
                        }
                    }
                }
            }
            return if output.is_empty() {
                None
            } else {
                Some(output)
            };
        }

        // Fall back to generating diff from arguments

        // Detect write operations by tool name
        const WRITE_TOOLS: &[&str] = &[
            "Edit",
            "edit",
            "WriteFile",
            "write_file",
            "write_text_file",
            "update_note",
            "create_note",
            "Write",
            "write",
            "MultiEdit",
        ];

        let title = &tool_call.title;
        let is_write = WRITE_TOOLS.iter().any(|w| title.contains(w));
        if !is_write {
            return None;
        }

        // Extract arguments
        let args = tool_call.arguments.as_ref()?;
        let obj = args.as_object()?;

        // Get file path (try multiple common parameter names)
        let path = obj
            .get("path")
            .or_else(|| obj.get("file_path"))
            .or_else(|| obj.get("file"))
            .and_then(|v| v.as_str())?;

        // Read current file content (may not exist for creates)
        let old_content = std::fs::read_to_string(path).unwrap_or_default();

        // Determine new content based on tool type:
        // 1. Edit tool: apply old_string -> new_string replacement
        // 2. Write tools: use content directly
        let new_content = if let (Some(old_str), Some(new_str)) = (
            obj.get("old_string").and_then(|v| v.as_str()),
            obj.get("new_string").and_then(|v| v.as_str()),
        ) {
            // Edit tool: apply the string replacement
            let replace_all = obj
                .get("replace_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if replace_all {
                old_content.replace(old_str, new_str)
            } else {
                old_content.replacen(old_str, new_str, 1)
            }
        } else if let Some(content) = obj
            .get("content")
            .or_else(|| obj.get("new_content"))
            .or_else(|| obj.get("text"))
            .and_then(|v| v.as_str())
        {
            // Full file write
            content.to_string()
        } else {
            // No content found
            return None;
        };

        // Skip if no changes
        if old_content == new_content {
            return None;
        }

        // Generate unified diff
        let diff = TextDiff::from_lines(old_content.as_str(), new_content.as_str());
        let mut output = String::new();

        for change in diff.iter_all_changes() {
            let tag = change.tag();
            let line = change.to_string_lossy();
            let line_content = line.strip_suffix('\n').unwrap_or(&line);

            match tag {
                ChangeTag::Delete => {
                    output.push_str(&format!("-{}\n", line_content));
                }
                ChangeTag::Insert => {
                    output.push_str(&format!("+{}\n", line_content));
                }
                ChangeTag::Equal => {
                    // Skip unchanged lines to keep output compact
                }
            }
        }

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    pub(super) fn upsert_tool_info(&self, tool_call: ToolCallInfo, state: &mut StreamingState) {
        if let Some(id) = &tool_call.id {
            if let Some(existing) = state
                .tool_calls
                .iter_mut()
                .find(|t| t.id.as_deref() == Some(id.as_str()))
            {
                *existing = tool_call;
                return;
            }
        }
        state.tool_calls.push(tool_call);
    }
}
