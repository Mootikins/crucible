use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

pub type TaskId = String;

pub fn generate_task_id() -> TaskId {
    use rand::Rng;
    let timestamp = Utc::now().format("%Y%m%d-%H%M");
    let mut rng = rand::rng();
    let random: String = (0..6)
        .map(|_| {
            let idx: u8 = rng.random_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + (idx - 10)) as char
            }
        })
        .collect();
    format!("task-{}-{}", timestamp, random)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskKind {
    Subagent {
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },
    Bash {
        command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workdir: Option<PathBuf>,
    },
}

impl TaskKind {
    pub fn name(&self) -> &'static str {
        match self {
            TaskKind::Subagent { .. } => "subagent",
            TaskKind::Bash { .. } => "bash",
        }
    }

    pub fn summary(&self) -> String {
        match self {
            TaskKind::Subagent { prompt, .. } => truncate(prompt, 80),
            TaskKind::Bash { command, .. } => truncate(command, 80),
        }
    }
}

impl fmt::Display for TaskKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskKind::Subagent { prompt, .. } => write!(f, "subagent: {}", truncate(prompt, 50)),
            TaskKind::Bash { command, .. } => write!(f, "bash: {}", truncate(command, 50)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: TaskId,
    pub session_id: String,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_path: Option<PathBuf>,
}

impl TaskInfo {
    pub fn new(session_id: String, kind: TaskKind) -> Self {
        Self {
            id: generate_task_id(),
            session_id,
            kind,
            status: TaskStatus::Running,
            started_at: Utc::now(),
            completed_at: None,
            session_path: None,
        }
    }

    pub fn with_session_path(mut self, path: PathBuf) -> Self {
        self.session_path = Some(path);
        self
    }

    pub fn session_link(&self) -> Option<String> {
        self.session_path
            .as_ref()
            .map(|p| format!("[[{}]]", p.display()))
    }

    pub fn summary(&self) -> String {
        format!(
            "[{}] {} - {}",
            self.id,
            self.kind.name(),
            self.kind.summary()
        )
    }

    pub fn duration(&self) -> Option<Duration> {
        self.completed_at.map(|end| end - self.started_at)
    }

    pub fn mark_completed(&mut self) {
        self.status = TaskStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    pub fn mark_failed(&mut self) {
        self.status = TaskStatus::Failed;
        self.completed_at = Some(Utc::now());
    }

    pub fn mark_cancelled(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub info: TaskInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

impl TaskResult {
    pub fn success(info: TaskInfo, output: String) -> Self {
        Self {
            info,
            output: Some(output),
            error: None,
            exit_code: None,
        }
    }

    pub fn success_with_exit_code(info: TaskInfo, output: String, exit_code: i32) -> Self {
        Self {
            info,
            output: Some(output),
            error: None,
            exit_code: Some(exit_code),
        }
    }

    pub fn failure(info: TaskInfo, error: String) -> Self {
        Self {
            info,
            output: None,
            error: Some(error),
            exit_code: None,
        }
    }

    pub fn failure_with_exit_code(info: TaskInfo, error: String, exit_code: i32) -> Self {
        Self {
            info,
            output: None,
            error: Some(error),
            exit_code: Some(exit_code),
        }
    }

    pub fn is_success(&self) -> bool {
        self.info.status == TaskStatus::Completed && self.error.is_none()
    }

    pub fn truncated_output(&self, max_len: usize) -> String {
        match &self.output {
            Some(out) => truncate(out, max_len),
            None => String::new(),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum TaskError {
    #[error("Task not found: {0}")]
    NotFound(TaskId),

    #[error("Task already completed: {0}")]
    AlreadyCompleted(TaskId),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Failed to spawn task: {0}")]
    SpawnFailed(String),

    #[error("Task timed out")]
    Timeout,

    #[error("Task limit exceeded for session")]
    LimitExceeded,
}

pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }

    let target_len = max_len.saturating_sub(3);
    let mut end = target_len.min(s.len());

    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    format!("{}...", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_kind_name_returns_correct_values() {
        let subagent = TaskKind::Subagent {
            prompt: "test".to_string(),
            context: None,
        };
        let bash = TaskKind::Bash {
            command: "ls".to_string(),
            workdir: None,
        };

        assert_eq!(subagent.name(), "subagent");
        assert_eq!(bash.name(), "bash");
    }

    #[test]
    fn task_kind_summary_truncates_long_content() {
        let long_prompt = "a".repeat(200);
        let subagent = TaskKind::Subagent {
            prompt: long_prompt.clone(),
            context: None,
        };

        let summary = subagent.summary();
        assert!(
            summary.len() <= 83,
            "summary should be at most 80 chars + ellipsis"
        );
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn task_status_is_terminal() {
        assert!(!TaskStatus::Running.is_terminal());
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed.is_terminal());
        assert!(TaskStatus::Cancelled.is_terminal());
    }

    #[test]
    fn task_info_new_creates_running_task() {
        let info = TaskInfo::new(
            "session-123".to_string(),
            TaskKind::Bash {
                command: "sleep 10".to_string(),
                workdir: None,
            },
        );

        assert!(info.id.starts_with("task-"));
        assert_eq!(info.session_id, "session-123");
        assert_eq!(info.status, TaskStatus::Running);
        assert!(info.completed_at.is_none());
    }

    #[test]
    fn task_info_mark_completed_sets_timestamp() {
        let mut info = TaskInfo::new(
            "session-123".to_string(),
            TaskKind::Subagent {
                prompt: "test".to_string(),
                context: None,
            },
        );

        assert!(info.completed_at.is_none());
        info.mark_completed();
        assert_eq!(info.status, TaskStatus::Completed);
        assert!(info.completed_at.is_some());
    }

    #[test]
    fn task_info_duration_calculates_correctly() {
        let mut info = TaskInfo::new(
            "session-123".to_string(),
            TaskKind::Subagent {
                prompt: "test".to_string(),
                context: None,
            },
        );

        assert!(info.duration().is_none());
        info.mark_completed();
        let duration = info
            .duration()
            .expect("should have duration after completion");
        assert!(duration.num_milliseconds() >= 0);
    }

    #[test]
    fn task_result_success_is_success() {
        let mut info = TaskInfo::new(
            "session-123".to_string(),
            TaskKind::Bash {
                command: "echo hello".to_string(),
                workdir: None,
            },
        );
        info.mark_completed();

        let result = TaskResult::success(info, "hello\n".to_string());
        assert!(result.is_success());
    }

    #[test]
    fn task_result_failure_is_not_success() {
        let mut info = TaskInfo::new(
            "session-123".to_string(),
            TaskKind::Bash {
                command: "false".to_string(),
                workdir: None,
            },
        );
        info.mark_failed();

        let result = TaskResult::failure(info, "command failed".to_string());
        assert!(!result.is_success());
    }

    #[test]
    fn task_result_truncated_output_respects_max_len() {
        let mut info = TaskInfo::new(
            "session-123".to_string(),
            TaskKind::Subagent {
                prompt: "test".to_string(),
                context: None,
            },
        );
        info.mark_completed();

        let long_output = "a".repeat(1000);
        let result = TaskResult::success(info, long_output);

        let truncated = result.truncated_output(100);
        assert!(truncated.len() <= 100);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn task_kind_serde_roundtrip() {
        let subagent = TaskKind::Subagent {
            prompt: "Research topic X".to_string(),
            context: Some("Additional context".to_string()),
        };

        let json = serde_json::to_string(&subagent).unwrap();
        let parsed: TaskKind = serde_json::from_str(&json).unwrap();
        assert_eq!(subagent, parsed);

        let bash = TaskKind::Bash {
            command: "cargo build".to_string(),
            workdir: Some(PathBuf::from("/home/user/project")),
        };

        let json = serde_json::to_string(&bash).unwrap();
        let parsed: TaskKind = serde_json::from_str(&json).unwrap();
        assert_eq!(bash, parsed);
    }

    #[test]
    fn task_status_serde_roundtrip() {
        for status in [
            TaskStatus::Running,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string_adds_ellipsis() {
        let result = truncate("hello world", 8);
        assert_eq!(result, "hello...");
    }

    #[test]
    fn truncate_handles_multibyte_utf8() {
        let result = truncate("こんにちは世界", 10);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 13);
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }
}
