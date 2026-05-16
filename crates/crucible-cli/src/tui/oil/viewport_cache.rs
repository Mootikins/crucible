use crucible_core::types::acp::FileDiff;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

pub const TOOL_OUTPUT_MAX_TAIL_LINES: usize = 50;

/// Display-only representation of tool source, for rendering provenance badges.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolSourceDisplay {
    Core,
    Crucible,
    Mcp { server: Arc<str> },
    Plugin { name: Arc<str> },
}

impl ToolSourceDisplay {
    /// Badge label for non-internal sources.
    ///
    /// Core/Crucible tools are part of the runtime — provenance is implicit,
    /// so they render no badge. MCP and plugin tools surface their origin so
    /// the user can tell where a tool came from.
    pub fn badge_label(&self) -> Option<String> {
        match self {
            Self::Core | Self::Crucible => None,
            Self::Mcp { server } => Some(format!("mcp:{server}")),
            Self::Plugin { name } => Some(format!("plugin:{name}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CachedToolCall {
    pub id: String,
    pub name: Arc<str>,
    pub args: Arc<str>,
    /// LLM-assigned call ID for matching results to the correct tool call.
    /// When set, tool result lookups prefer this over name-based matching.
    pub call_id: Option<String>,
    pub output_tail: VecDeque<Arc<str>>,
    pub output_path: Option<PathBuf>,
    pub output_total_bytes: usize,
    pub error: Option<String>,
    pub started_at: std::time::Instant,
    pub complete: bool,
    /// Set to true when a delegation supersedes this tool call visually.
    pub superseded: bool,
    /// Optional human-readable description of what the tool does.
    pub description: Option<Arc<str>>,
    /// Optional source provenance for display (e.g., "[Crucible]" badge).
    pub source: Option<ToolSourceDisplay>,
    /// Optional primary argument from Lua tool display hook.
    pub lua_primary_arg: Option<Arc<str>>,
    /// File diffs surfaced by the agent (e.g. ACP `ToolCallContent::Diff`).
    /// Empty for tools that don't produce diffs or for backends that don't
    /// surface them yet. Rendered between header and result on completion.
    pub diffs: Vec<FileDiff>,
}

impl CachedToolCall {
    pub fn new(id: impl Into<String>, name: impl AsRef<str>, args: impl AsRef<str>) -> Self {
        Self {
            id: id.into(),
            name: Arc::from(name.as_ref()),
            args: Arc::from(args.as_ref()),
            call_id: None,
            output_tail: VecDeque::new(),
            output_path: None,
            output_total_bytes: 0,
            error: None,
            started_at: std::time::Instant::now(),
            complete: false,
            superseded: false,
            description: None,
            source: None,
            lua_primary_arg: None,
            diffs: Vec::new(),
        }
    }

    pub fn append_output(&mut self, delta: &str) {
        self.output_total_bytes += delta.len();
        for line in delta.lines() {
            self.output_tail.push_back(Arc::from(line));
            if self.output_tail.len() > TOOL_OUTPUT_MAX_TAIL_LINES {
                self.output_tail.pop_front();
            }
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.complete = true;
    }

    pub fn mark_complete(&mut self) {
        self.complete = true;
    }

    /// Replace the tool's file-diff snapshot. Used when ACP agents
    /// (e.g. Claude Code) defer diff content until after the initial
    /// tool_call frame and surface it via a follow-up
    /// `tool_call_diff_update` event.
    pub fn set_diffs(&mut self, diffs: Vec<FileDiff>) {
        self.diffs = diffs;
    }

    pub fn set_output_path(&mut self, path: PathBuf) {
        self.output_path = Some(path);
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    pub fn last_n_lines(&self, n: usize) -> Vec<&str> {
        let skip = self.output_tail.len().saturating_sub(n);
        self.output_tail
            .iter()
            .skip(skip)
            .map(|s| s.as_ref())
            .collect()
    }

    pub fn result(&self) -> String {
        self.output_tail
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone)]
pub struct CachedShellExecution {
    pub id: String,
    pub command: Arc<str>,
    pub exit_code: i32,
    pub output_tail: Vec<Arc<str>>,
    pub output_path: Option<PathBuf>,
}

impl CachedShellExecution {
    pub fn new(
        id: impl Into<String>,
        command: impl AsRef<str>,
        exit_code: i32,
        output_tail: Vec<String>,
        output_path: Option<PathBuf>,
    ) -> Self {
        Self {
            id: id.into(),
            command: Arc::from(command.as_ref()),
            exit_code,
            output_tail: output_tail
                .into_iter()
                .map(|s| Arc::from(s.as_str()))
                .collect(),
            output_path,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct CachedSubagent {
    pub id: Arc<str>,
    pub prompt: Arc<str>,
    pub status: SubagentStatus,
    pub summary: Option<Arc<str>>,
    pub error: Option<Arc<str>>,
    pub started_at: std::time::Instant,
    pub label: &'static str,
    pub target_agent: Option<String>,
}

impl CachedSubagent {
    pub fn new(id: impl Into<String>, prompt: impl AsRef<str>, label: &'static str) -> Self {
        Self {
            id: Arc::from(id.into().as_str()),
            prompt: Arc::from(prompt.as_ref()),
            status: SubagentStatus::Running,
            summary: None,
            error: None,
            started_at: std::time::Instant::now(),
            label,
            target_agent: None,
        }
    }

    pub fn mark_completed(&mut self, summary: &str) {
        self.status = SubagentStatus::Completed;
        self.summary = Some(Arc::from(summary));
    }

    pub fn mark_failed(&mut self, error: &str) {
        self.status = SubagentStatus::Failed;
        self.error = Some(Arc::from(error));
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    /// Whether the subagent has reached a terminal state (completed or failed).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            SubagentStatus::Completed | SubagentStatus::Failed
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_tool_call_new_metadata_defaults_to_none() {
        let tool = CachedToolCall::new("t1", "my_tool", "{}");
        assert!(!tool.superseded);
        assert!(tool.description.is_none());
        assert!(tool.source.is_none());
    }

    #[test]
    fn tool_source_badge_label_internal_returns_none() {
        assert_eq!(ToolSourceDisplay::Core.badge_label(), None);
        assert_eq!(ToolSourceDisplay::Crucible.badge_label(), None);
    }

    #[test]
    fn tool_source_badge_label_external_returns_label() {
        assert_eq!(
            ToolSourceDisplay::Mcp {
                server: Arc::from("gmail")
            }
            .badge_label(),
            Some("mcp:gmail".to_string())
        );
        assert_eq!(
            ToolSourceDisplay::Plugin {
                name: Arc::from("my_plugin")
            }
            .badge_label(),
            Some("plugin:my_plugin".to_string())
        );
    }
}
