use agent_client_protocol::ToolCallStatus;

use super::types::{ResponseSegment, StreamingState};
use super::CrucibleAcpClient;
use crate::acp::streaming::humanize_tool_title;
use crucible_core::types::acp::{FileDiff, ToolCallInfo};

/// Format a slice of `FileDiff`s as a unified-diff text block, suitable for
/// embedding in the assistant's textual response. Returns None if every diff
/// produced no changed lines.
fn format_diffs_unified(diffs: &[FileDiff]) -> Option<String> {
    use similar::{ChangeTag, TextDiff};

    let mut output = String::new();
    for diff_entry in diffs {
        let entry_start = output.len();
        if !output.is_empty() {
            output.push_str("\n--- \n");
        }
        output.push_str(&format!("--- {}\n", diff_entry.path));
        output.push_str(&format!("+++ {}\n", diff_entry.path));

        let old = diff_entry.old_content.as_deref().unwrap_or("");
        let diff = TextDiff::from_lines(old, diff_entry.new_content.as_str());

        let mut wrote_any = false;
        for change in diff.iter_all_changes() {
            let line = change.to_string_lossy();
            let line_content = line.strip_suffix('\n').unwrap_or(&line);
            match change.tag() {
                ChangeTag::Delete => {
                    output.push_str(&format!("-{}\n", line_content));
                    wrote_any = true;
                }
                ChangeTag::Insert => {
                    output.push_str(&format!("+{}\n", line_content));
                    wrote_any = true;
                }
                ChangeTag::Equal => {}
            }
        }
        if !wrote_any {
            // Roll back the empty header for this path so the join stays clean.
            output.truncate(entry_start);
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

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
    /// Checks two sources in order:
    /// 1. Pre-computed diffs from protocol (e.g., ACP's ToolCallContent::Diff)
    /// 2. Synthesized diffs from tool name + arguments (delegates to
    ///    `crate::tools::diff_synth::synthesize_diffs` so tool-name normalization
    ///    and arg-fallback rules stay in one place).
    pub(super) fn generate_diff_for_write(&self, tool_call: &ToolCallInfo) -> Option<String> {
        // Pre-computed diffs from the protocol take priority.
        if !tool_call.diffs.is_empty() {
            return format_diffs_unified(&tool_call.diffs);
        }

        // Fall back to synthesizing from tool name + args. Single source of
        // truth for write-tool detection and argument shape. Resolve relative
        // paths against the session's working dir; tests pass absolute paths
        // and tolerate the empty fallback.
        let args = tool_call.arguments.as_ref()?;
        let workspace_root = self
            .config
            .working_dir
            .as_deref()
            .unwrap_or_else(|| std::path::Path::new(""));
        let synthesized = crate::tools::diff_synth::synthesize_diffs(
            &tool_call.title,
            args,
            workspace_root,
        );
        if synthesized.is_empty() {
            return None;
        }
        format_diffs_unified(&synthesized)
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
