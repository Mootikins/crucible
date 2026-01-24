use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

pub type JobId = String;

pub fn generate_job_id() -> JobId {
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
    format!("job-{}-{}", timestamp, random)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobKind {
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

impl JobKind {
    pub fn name(&self) -> &'static str {
        match self {
            JobKind::Subagent { .. } => "subagent",
            JobKind::Bash { .. } => "bash",
        }
    }

    pub fn summary(&self) -> String {
        match self {
            JobKind::Subagent { prompt, .. } => truncate(prompt, 80),
            JobKind::Bash { command, .. } => truncate(command, 80),
        }
    }
}

impl fmt::Display for JobKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobKind::Subagent { prompt, .. } => write!(f, "subagent: {}", truncate(prompt, 50)),
            JobKind::Bash { command, .. } => write!(f, "bash: {}", truncate(command, 50)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    #[default]
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        )
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobStatus::Running => write!(f, "running"),
            JobStatus::Completed => write!(f, "completed"),
            JobStatus::Failed => write!(f, "failed"),
            JobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub id: JobId,
    pub session_id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_path: Option<PathBuf>,
}

impl JobInfo {
    pub fn new(session_id: String, kind: JobKind) -> Self {
        Self {
            id: generate_job_id(),
            session_id,
            kind,
            status: JobStatus::Running,
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
        self.status = JobStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    pub fn mark_failed(&mut self) {
        self.status = JobStatus::Failed;
        self.completed_at = Some(Utc::now());
    }

    pub fn mark_cancelled(&mut self) {
        self.status = JobStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub info: JobInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

impl JobResult {
    pub fn success(info: JobInfo, output: String) -> Self {
        Self {
            info,
            output: Some(output),
            error: None,
            exit_code: None,
        }
    }

    pub fn success_with_exit_code(info: JobInfo, output: String, exit_code: i32) -> Self {
        Self {
            info,
            output: Some(output),
            error: None,
            exit_code: Some(exit_code),
        }
    }

    pub fn failure(info: JobInfo, error: String) -> Self {
        Self {
            info,
            output: None,
            error: Some(error),
            exit_code: None,
        }
    }

    pub fn failure_with_exit_code(info: JobInfo, error: String, exit_code: i32) -> Self {
        Self {
            info,
            output: None,
            error: Some(error),
            exit_code: Some(exit_code),
        }
    }

    pub fn is_success(&self) -> bool {
        self.info.status == JobStatus::Completed && self.error.is_none()
    }

    pub fn truncated_output(&self, max_len: usize) -> String {
        match &self.output {
            Some(out) => truncate(out, max_len),
            None => String::new(),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum JobError {
    #[error("Job not found: {0}")]
    NotFound(JobId),

    #[error("Job already completed: {0}")]
    AlreadyCompleted(JobId),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Failed to spawn job: {0}")]
    SpawnFailed(String),

    #[error("Job timed out")]
    Timeout,

    #[error("Job limit exceeded for session")]
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
    fn job_kind_name_returns_correct_values() {
        let subagent = JobKind::Subagent {
            prompt: "test".to_string(),
            context: None,
        };
        let bash = JobKind::Bash {
            command: "ls".to_string(),
            workdir: None,
        };

        assert_eq!(subagent.name(), "subagent");
        assert_eq!(bash.name(), "bash");
    }

    #[test]
    fn job_kind_summary_truncates_long_content() {
        let long_prompt = "a".repeat(200);
        let subagent = JobKind::Subagent {
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
    fn job_status_is_terminal() {
        assert!(!JobStatus::Running.is_terminal());
        assert!(JobStatus::Completed.is_terminal());
        assert!(JobStatus::Failed.is_terminal());
        assert!(JobStatus::Cancelled.is_terminal());
    }

    #[test]
    fn job_info_new_creates_running_job() {
        let info = JobInfo::new(
            "session-123".to_string(),
            JobKind::Bash {
                command: "sleep 10".to_string(),
                workdir: None,
            },
        );

        assert!(info.id.starts_with("job-"));
        assert_eq!(info.session_id, "session-123");
        assert_eq!(info.status, JobStatus::Running);
        assert!(info.completed_at.is_none());
    }

    #[test]
    fn job_info_mark_completed_sets_timestamp() {
        let mut info = JobInfo::new(
            "session-123".to_string(),
            JobKind::Subagent {
                prompt: "test".to_string(),
                context: None,
            },
        );

        assert!(info.completed_at.is_none());
        info.mark_completed();
        assert_eq!(info.status, JobStatus::Completed);
        assert!(info.completed_at.is_some());
    }

    #[test]
    fn job_info_duration_calculates_correctly() {
        let mut info = JobInfo::new(
            "session-123".to_string(),
            JobKind::Subagent {
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
    fn job_result_success_is_success() {
        let mut info = JobInfo::new(
            "session-123".to_string(),
            JobKind::Bash {
                command: "echo hello".to_string(),
                workdir: None,
            },
        );
        info.mark_completed();

        let result = JobResult::success(info, "hello\n".to_string());
        assert!(result.is_success());
    }

    #[test]
    fn job_result_failure_is_not_success() {
        let mut info = JobInfo::new(
            "session-123".to_string(),
            JobKind::Bash {
                command: "false".to_string(),
                workdir: None,
            },
        );
        info.mark_failed();

        let result = JobResult::failure(info, "command failed".to_string());
        assert!(!result.is_success());
    }

    #[test]
    fn job_result_truncated_output_respects_max_len() {
        let mut info = JobInfo::new(
            "session-123".to_string(),
            JobKind::Subagent {
                prompt: "test".to_string(),
                context: None,
            },
        );
        info.mark_completed();

        let long_output = "a".repeat(1000);
        let result = JobResult::success(info, long_output);

        let truncated = result.truncated_output(100);
        assert!(truncated.len() <= 100);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn job_kind_serde_roundtrip() {
        let subagent = JobKind::Subagent {
            prompt: "Research topic X".to_string(),
            context: Some("Additional context".to_string()),
        };

        let json = serde_json::to_string(&subagent).unwrap();
        let parsed: JobKind = serde_json::from_str(&json).unwrap();
        assert_eq!(subagent, parsed);

        let bash = JobKind::Bash {
            command: "cargo build".to_string(),
            workdir: Some(PathBuf::from("/home/user/project")),
        };

        let json = serde_json::to_string(&bash).unwrap();
        let parsed: JobKind = serde_json::from_str(&json).unwrap();
        assert_eq!(bash, parsed);
    }

    #[test]
    fn job_status_serde_roundtrip() {
        for status in [
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: JobStatus = serde_json::from_str(&json).unwrap();
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
