//! Background task types for session-scoped async work (subagents, long-running bash).
//!
//! TODO: Rename "task" -> "job" throughout this module for Unix-familiar terminology
//! (matches bg/fg/jobs). Affects: TaskId->JobId, TaskInfo->JobInfo, TaskResult->JobResult,
//! TaskStatus->JobStatus, TaskError->JobError, spawn_*->spawn_*, list_tasks->list_jobs, etc.

mod types;

pub use types::{
    generate_task_id, truncate, TaskError, TaskId, TaskInfo, TaskKind, TaskResult, TaskStatus,
};

use async_trait::async_trait;
use std::path::PathBuf;
use std::time::Duration;

/// Trait for spawning and managing background tasks.
///
/// Implementations handle the actual execution of tasks in the background,
/// tracking their status and storing results for later retrieval.
#[async_trait]
pub trait BackgroundSpawner: Send + Sync {
    /// Spawn a bash command in the background.
    ///
    /// Returns the task ID immediately. The command runs asynchronously.
    async fn spawn_bash(
        &self,
        session_id: &str,
        command: String,
        workdir: Option<PathBuf>,
        timeout: Option<Duration>,
    ) -> Result<TaskId, TaskError>;

    /// Spawn a subagent in the background.
    ///
    /// The subagent runs with inherited tools (minus spawn_subagent to prevent recursion)
    /// and executes up to `max_turns` conversation turns.
    ///
    /// Returns the task ID immediately. The subagent runs asynchronously.
    async fn spawn_subagent(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
    ) -> Result<TaskId, TaskError>;

    /// List all tasks (running + completed) for a session.
    ///
    /// Returns tasks sorted by start time (newest first).
    fn list_tasks(&self, session_id: &str) -> Vec<TaskInfo>;

    /// Get the result of a specific task.
    ///
    /// Returns `None` if the task doesn't exist.
    /// For running tasks, returns a result with `Running` status and no output.
    fn get_task_result(&self, task_id: &TaskId) -> Option<TaskResult>;

    /// Cancel a running task.
    ///
    /// Returns `true` if the task was found and cancellation was requested.
    /// Returns `false` if the task was not found or already completed.
    async fn cancel_task(&self, task_id: &TaskId) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_id_format() {
        let id = generate_task_id();
        assert!(
            id.starts_with("task-"),
            "ID should start with 'task-': {}",
            id
        );

        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 4, "ID should have 4 parts: {}", id);
        assert_eq!(parts[0], "task");
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
    fn test_task_id_uniqueness() {
        let ids: Vec<TaskId> = (0..100).map(|_| generate_task_id()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "All IDs should be unique");
    }
}
