use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use textwrap::{wrap, Options, WordSplitter};

use super::chat_app::Role;

#[derive(Debug, Clone)]
pub struct CachedMessage {
    pub id: String,
    pub role: Role,
    content: Arc<str>,
    wrapped: Option<(usize, Vec<String>)>,
}

impl CachedMessage {
    pub fn new(id: impl Into<String>, role: Role, content: impl AsRef<str>) -> Self {
        Self {
            id: id.into(),
            role,
            content: Arc::from(content.as_ref()),
            wrapped: None,
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn wrapped_lines(&mut self, width: usize) -> &[String] {
        if self.wrapped.as_ref().map(|(w, _)| *w) != Some(width) {
            let lines = wrap_content(&self.content, width);
            self.wrapped = Some((width, lines));
        }
        &self.wrapped.as_ref().unwrap().1
    }

    pub fn invalidate_wrap(&mut self) {
        self.wrapped = None;
    }
}

pub const TOOL_OUTPUT_MAX_TAIL_LINES: usize = 50;
pub const TOOL_OUTPUT_FILE_THRESHOLD_BYTES: usize = 10 * 1024;

#[derive(Debug, Clone)]
pub struct CachedToolCall {
    pub id: String,
    pub name: Arc<str>,
    pub args: Arc<str>,
    pub output_tail: VecDeque<Arc<str>>,
    pub output_path: Option<PathBuf>,
    pub output_total_bytes: usize,
    pub error: Option<String>,
    pub started_at: std::time::Instant,
    pub complete: bool,
}

impl CachedToolCall {
    pub fn new(id: impl Into<String>, name: impl AsRef<str>, args: impl AsRef<str>) -> Self {
        Self {
            id: id.into(),
            name: Arc::from(name.as_ref()),
            args: Arc::from(args.as_ref()),
            output_tail: VecDeque::new(),
            output_path: None,
            output_total_bytes: 0,
            error: None,
            started_at: std::time::Instant::now(),
            complete: false,
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

    pub fn set_output_path(&mut self, path: PathBuf) {
        self.output_path = Some(path);
    }

    pub fn should_spill_to_file(&self) -> bool {
        self.output_path.is_none() && self.output_total_bytes >= TOOL_OUTPUT_FILE_THRESHOLD_BYTES
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
}

impl CachedSubagent {
    pub fn new(id: impl Into<String>, prompt: impl AsRef<str>) -> Self {
        Self {
            id: Arc::from(id.into().as_str()),
            prompt: Arc::from(prompt.as_ref()),
            status: SubagentStatus::Running,
            summary: None,
            error: None,
            started_at: std::time::Instant::now(),
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
}

#[derive(Debug, Clone)]
pub enum CachedChatItem {
    Message(CachedMessage),
    ToolCall(CachedToolCall),
    ShellExecution(CachedShellExecution),
    Subagent(CachedSubagent),
}

impl CachedChatItem {
    pub fn id(&self) -> &str {
        match self {
            CachedChatItem::Message(m) => &m.id,
            CachedChatItem::ToolCall(t) => &t.id,
            CachedChatItem::ShellExecution(s) => &s.id,
            CachedChatItem::Subagent(s) => &s.id,
        }
    }

    pub fn content(&self) -> Option<&str> {
        match self {
            CachedChatItem::Message(m) => Some(m.content()),
            _ => None,
        }
    }

    pub fn as_message(&self) -> Option<&CachedMessage> {
        match self {
            CachedChatItem::Message(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_message_mut(&mut self) -> Option<&mut CachedMessage> {
        match self {
            CachedChatItem::Message(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_tool_call(&self) -> Option<&CachedToolCall> {
        match self {
            CachedChatItem::ToolCall(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_tool_call_mut(&mut self) -> Option<&mut CachedToolCall> {
        match self {
            CachedChatItem::ToolCall(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_shell_execution(&self) -> Option<&CachedShellExecution> {
        match self {
            CachedChatItem::ShellExecution(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_subagent(&self) -> Option<&CachedSubagent> {
        match self {
            CachedChatItem::Subagent(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_subagent_mut(&mut self) -> Option<&mut CachedSubagent> {
        match self {
            CachedChatItem::Subagent(s) => Some(s),
            _ => None,
        }
    }
}

fn wrap_content(content: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![content.to_string()];
    }

    let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);

    content
        .lines()
        .flat_map(|line| {
            if line.is_empty() {
                vec![String::new()]
            } else {
                wrap(line, &options)
                    .into_iter()
                    .map(|cow| cow.into_owned())
                    .collect()
            }
        })
        .collect()
}
