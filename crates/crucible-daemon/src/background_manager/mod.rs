//! Background job management for the daemon.
//!
//! Provides session-scoped, ephemeral job management (jobs don't survive
//! daemon restart) for **bash** background commands.
//!
//! Subagent delegation used to run here too; delegated children are now real
//! scheduler-driven sessions managed by [`crate::delegation::DelegationService`].
//!
//! # Example
//!
//! ```text
//! let manager = BackgroundJobManager::new(event_tx);
//!
//! let job_id = manager.spawn_bash(
//!     "session-123",
//!     "cargo build --release",
//!     None,
//!     None,
//! ).await?;
//!
//! let jobs = manager.list_jobs("session-123");
//!
//! if let Some(result) = manager.get_job_result(&job_id) {
//!     println!("Job completed: {:?}", result);
//! }
//! ```

use crate::event_emitter::emit_event;
use crate::protocol::SessionEventMessage;
use async_trait::async_trait;

use crucible_core::background::{BackgroundSpawner, JobError, JobId, JobInfo, JobKind, JobResult};
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

const DEFAULT_BASH_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_HISTORY_PER_SESSION: usize = 50;

mod bash;
mod spawner;
mod types;

#[cfg(test)]
mod tests;

pub use types::BackgroundError;
use types::{events, BashError, RunningJob};

pub struct BackgroundJobManager {
    running: Arc<DashMap<JobId, RunningJob>>,
    history: Arc<DashMap<String, std::collections::VecDeque<JobResult>>>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    max_history: usize,
    pub(crate) completed: Arc<tokio::sync::Notify>,
    /// Where a running job reports itself, so the daemon does not exit in the
    /// middle of one. The server hands its own registry in
    /// ([`Self::with_activity`]); a manager built without one counts into a
    /// registry nothing reads, which is right for a test.
    activity: Arc<crate::activity::DaemonActivity>,
}

impl BackgroundJobManager {
    pub fn new(event_tx: broadcast::Sender<SessionEventMessage>) -> Self {
        Self {
            running: Arc::new(DashMap::new()),
            history: Arc::new(DashMap::new()),
            event_tx,
            max_history: MAX_HISTORY_PER_SESSION,
            completed: Arc::default(),
            activity: crate::activity::DaemonActivity::new(),
        }
    }

    /// Report running jobs into the daemon's own activity registry rather
    /// than this manager's private one. The server calls this at bind.
    pub fn with_activity(mut self, activity: Arc<crate::activity::DaemonActivity>) -> Self {
        self.activity = activity;
        self
    }

    pub fn list_jobs(&self, session_id: &str) -> Vec<JobInfo> {
        let mut jobs = Vec::new();

        for entry in self.running.iter() {
            if entry.value().info.session_id == session_id {
                jobs.push(entry.value().info.clone());
            }
        }

        if let Some(history) = self.history.get(session_id) {
            for result in history.iter() {
                jobs.push(result.info.clone());
            }
        }

        jobs.sort_by_key(|j| std::cmp::Reverse(j.started_at));

        jobs
    }

    /// How many jobs are running right now, across every session.
    ///
    /// The idle timer does NOT read this — every running job holds a
    /// [`crate::activity::WorkKind::BackgroundJob`] guard instead, so the
    /// timer has one question to ask rather than a list of counters to
    /// collect. This stays for the status surfaces that report job counts.
    pub fn running_count(&self) -> usize {
        self.running.len()
    }

    pub fn get_job_result(&self, job_id: &JobId) -> Option<JobResult> {
        if let Some(entry) = self.running.get(job_id) {
            return Some(JobResult {
                info: entry.info.clone(),
                output: None,
                error: None,
                exit_code: None,
            });
        }

        self.history
            .iter()
            .find_map(|entry| entry.value().iter().find(|r| r.info.id == *job_id).cloned())
    }

    pub async fn cancel_job(&self, job_id: &JobId) -> bool {
        let dashmap::mapref::entry::Entry::Occupied(entry) = self.running.entry(job_id.clone())
        else {
            warn!(job_id = %job_id, "Job not found for cancellation");
            return false;
        };

        let mut info = entry.get().info.clone();
        info.mark_cancelled();
        let job_session_id = info.session_id.clone();
        let job_result = JobResult::failure(info, "Job cancelled".to_string());

        let kind = job_result.info.kind.name();
        Self::add_to_history(
            &self.history,
            &job_session_id,
            job_result.clone(),
            self.max_history,
        );
        let running_job = entry.remove();
        let _ = running_job.cancel_tx.send(());
        self.completed.notify_waiters();
        Self::emit_background_completed(&self.event_tx, &job_session_id, job_id, &job_result, kind);

        info!(job_id = %job_id, "Job cancelled");
        true
    }

    /// Cancel all running jobs for a session; optionally drop its history.
    pub async fn cleanup_session(&self, session_id: &str, clear_history: bool) {
        let job_ids: Vec<JobId> = self
            .running
            .iter()
            .filter(|entry| entry.value().info.session_id == session_id)
            .map(|entry| entry.key().clone())
            .collect();

        for job_id in job_ids {
            self.cancel_job(&job_id).await;
        }

        if clear_history {
            self.history.remove(session_id);
            self.completed.notify_waiters();
        }

        debug!(session_id = %session_id, "Session cleanup completed");
    }

    fn add_to_history(
        history: &DashMap<String, std::collections::VecDeque<JobResult>>,
        session_id: &str,
        result: JobResult,
        max_history: usize,
    ) {
        let mut entry = history.entry(session_id.to_string()).or_default();
        entry.push_back(result);

        while entry.len() > max_history {
            entry.pop_front();
        }
    }
}
