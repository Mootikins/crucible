//! Background job types for session-scoped async work (subagents, long-running bash).
//!
//! Uses Unix-familiar terminology (bg/fg/jobs).

mod types;

pub use types::{
    generate_job_id, truncate, JobError, JobId, JobInfo, JobKind, JobResult, JobStatus,
};

use async_trait::async_trait;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::oneshot;

/// Configuration for blocking subagent delegation.
#[derive(Debug, Clone)]
pub struct SubagentBlockingConfig {
    /// Maximum time to wait for subagent completion.
    pub timeout: Duration,
    /// Maximum bytes returned in `JobResult.output`.
    pub result_max_bytes: usize,
}

impl Default for SubagentBlockingConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            result_max_bytes: 51200,
        }
    }
}

/// Trait for spawning and managing background jobs.
///
/// Implementations handle the actual execution of jobs in the background,
/// tracking their status and storing results for later retrieval.
#[async_trait]
pub trait BackgroundSpawner: Send + Sync {
    /// Spawn a bash command in the background.
    ///
    /// Returns the job ID immediately. The command runs asynchronously.
    async fn spawn_bash(
        &self,
        session_id: &str,
        command: String,
        workdir: Option<PathBuf>,
        timeout: Option<Duration>,
    ) -> Result<JobId, JobError>;

    /// Spawn a subagent in the background.
    ///
    /// The subagent runs with inherited tools (minus spawn_subagent to prevent recursion)
    /// and executes up to `max_turns` conversation turns.
    ///
    /// Returns the job ID immediately. The subagent runs asynchronously.
    async fn spawn_subagent(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
    ) -> Result<JobId, JobError>;

    /// Spawn a subagent and wait for completion.
    ///
    /// Default implementation returns an unsupported error so existing
    /// `BackgroundSpawner` implementations remain source-compatible.
    async fn spawn_subagent_blocking(
        &self,
        _session_id: &str,
        _prompt: String,
        _context: Option<String>,
        _config: SubagentBlockingConfig,
        _cancel_rx: Option<oneshot::Receiver<()>>,
    ) -> Result<JobResult, JobError> {
        Err(JobError::SpawnFailed(
            "spawn_subagent_blocking not supported".to_string(),
        ))
    }

    /// List all jobs (running + completed) for a session.
    ///
    /// Returns jobs sorted by start time (newest first).
    fn list_jobs(&self, session_id: &str) -> Vec<JobInfo>;

    /// Get the result of a specific job.
    ///
    /// Returns `None` if the job doesn't exist.
    /// For running jobs, returns a result with `Running` status and no output.
    fn get_job_result(&self, job_id: &JobId) -> Option<JobResult>;

    /// Cancel a running job.
    ///
    /// Returns `true` if the job was found and cancellation was requested.
    /// Returns `false` if the job was not found or already completed.
    async fn cancel_job(&self, job_id: &JobId) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_id_format() {
        let id = generate_job_id();
        assert!(
            id.starts_with("job-"),
            "ID should start with 'job-': {}",
            id
        );

        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 4, "ID should have 4 parts: {}", id);
        assert_eq!(parts[0], "job");
        assert_eq!(
            parts[1].len(),
            8,
            "Date part should be 8 chars: {}",
            parts[1]
        );
        assert_eq!(
            parts[2].len(),
            4,
            "Time part should be 4 chars: {}",
            parts[2]
        );
        assert_eq!(
            parts[3].len(),
            6,
            "Random part should be 6 chars: {}",
            parts[3]
        );
    }

    #[test]
    fn test_job_id_uniqueness() {
        let ids: Vec<JobId> = (0..100).map(|_| generate_job_id()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "All IDs should be unique");
    }
}
