use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::process::Child;

use crucible_core::types::acp::ToolCallInfo;

/// Configuration for the ACP client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Path to the agent executable or script
    pub agent_path: PathBuf,

    /// Command-line arguments to pass to the agent
    #[serde(default)]
    pub agent_args: Option<Vec<String>>,

    /// Working directory for the agent process
    pub working_dir: Option<PathBuf>,

    /// Environment variables to pass to the agent
    pub env_vars: Option<Vec<(String, String)>>,

    /// Timeout for agent operations (in milliseconds)
    pub timeout_ms: Option<u64>,

    /// Maximum number of retry attempts
    pub max_retries: Option<u32>,
}

/// Represents a spawned agent process
///
/// This struct wraps a child process and provides methods to interact with it.
#[derive(Debug)]
pub struct AgentProcess {
    #[allow(dead_code)]
    pub(super) child: Child,
}

impl AgentProcess {
    /// Check if the agent process is still running
    ///
    /// # Returns
    ///
    /// `true` if the process is running, `false` otherwise
    pub fn is_running(&self) -> bool {
        // For now, we assume the process is running if we have a handle to it
        // In a full implementation, we would check the process status
        true
    }
}

pub(super) enum ResponseSegment {
    Text(String),
    Tool { label: String, diff: Option<String> },
}

#[derive(Default)]
pub(super) struct StreamingState {
    pub(super) segments: Vec<ResponseSegment>,
    pub(super) tool_calls: Vec<ToolCallInfo>,
    pub(super) notification_count: usize,
    pub(super) tool_segment_index: std::collections::HashMap<String, usize>,
    pub(super) tool_block_active: bool,
    /// Raw accumulated text (for deduplication of full-text re-sends).
    /// Some ACP agents (e.g. cursor-acp) send the complete accumulated text
    /// as a final notification before the JSON-RPC response. We track the
    /// accumulated text here to detect and skip these re-sends.
    pub(super) accumulated_text: String,
}

impl StreamingState {
    pub(super) fn append_text(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.accumulated_text.push_str(text);
        let chunk = text.to_string();
        if let Some(ResponseSegment::Text(last)) = self.segments.last_mut() {
            last.push_str(&chunk);
        } else {
            self.segments.push(ResponseSegment::Text(chunk));
        }
        self.tool_block_active = false;
    }

    /// Check if incoming text is a full re-send of already-accumulated content.
    /// Some ACP agents (e.g. cursor-acp) emit the complete response as a final
    /// `session/update` notification. We detect this by checking if the incoming
    /// text equals the accumulated text so far.
    pub(super) fn is_duplicate_resend(&self, text: &str) -> bool {
        !self.accumulated_text.is_empty() && text.trim() == self.accumulated_text.trim()
    }

    pub(super) fn formatted_output(&self) -> String {
        let mut output = String::new();
        let mut in_tool_block = false;
        for seg in &self.segments {
            match seg {
                ResponseSegment::Text(text) => {
                    if in_tool_block {
                        // End tool block with blank line
                        output.push('\n');
                        in_tool_block = false;
                    }
                    output.push_str(text);
                }
                ResponseSegment::Tool { label, diff } => {
                    if !in_tool_block {
                        // Start tool block with blank line before
                        if !output.is_empty() && !output.ends_with('\n') {
                            output.push('\n');
                        }
                        output.push('\n');
                        in_tool_block = true;
                    }
                    // All tool calls indented in the block
                    output.push_str("  ");
                    output.push_str(label);
                    output.push('\n');

                    // Render diff if present (each line indented)
                    if let Some(diff_str) = diff {
                        for line in diff_str.lines() {
                            output.push_str("    ");
                            output.push_str(line);
                            output.push('\n');
                        }
                    }
                }
            }
        }
        // End tool block if we finished with tools
        if in_tool_block {
            output.push('\n');
        }
        output
    }

    pub(super) fn formatted_length(&self) -> usize {
        self.formatted_output().len()
    }

    pub(super) fn title_for_tool(&self, id: &str) -> Option<String> {
        self.tool_calls
            .iter()
            .find(|tool| tool.id.as_deref() == Some(id))
            .map(|tool| tool.title.clone())
    }
}

