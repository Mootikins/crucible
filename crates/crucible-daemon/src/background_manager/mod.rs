//! Background job management for the daemon.
//!
//! Provides session-scoped, ephemeral job management (jobs don't survive daemon restart).
//!
//! # Supported Job Types
//!
//! - **Bash**: Background shell command execution with timeout
//! - **Subagent**: Multi-turn LLM execution with inherited tools
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
use crucible_config::AgentProfile;

use crucible_core::background::{
    truncate, BackgroundSpawner, JobError, JobId, JobInfo, JobKind, JobResult,
    SubagentBlockingConfig,
};
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::AgentHandle;
use crate::observe::events::LogEvent;
use crate::observe::session::SessionWriter;
use dashmap::DashMap;
use futures::StreamExt;
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

const DEFAULT_BASH_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_HISTORY_PER_SESSION: usize = 50;
const DEFAULT_SUBAGENT_MAX_TURNS: usize = 10;
/// Maximum output size for subagent accumulated output (10 MB)
const MAX_SUBAGENT_OUTPUT: usize = 10 * 1024 * 1024;

mod bash;
mod spawner;
mod subagent;
mod types;

#[cfg(test)]
mod tests;

use types::{
    events, parse_target_agent_name, target_profile_to_session_agent, BashError,
    PreparedSubagentExecution, RunningJob, SubagentError, SubagentExecutionOptions,
};
pub use types::{BackgroundError, SubagentContext, SubagentFactory};

pub struct BackgroundJobManager {
    running: Arc<DashMap<JobId, RunningJob>>,
    history: Arc<DashMap<String, std::collections::VecDeque<JobResult>>>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    max_history: usize,
    subagent_factory: Option<Arc<SubagentFactory>>,
    subagent_contexts: Arc<DashMap<String, SubagentContext>>,
}

impl BackgroundJobManager {
    pub fn new(event_tx: broadcast::Sender<SessionEventMessage>) -> Self {
        Self {
            running: Arc::new(DashMap::new()),
            history: Arc::new(DashMap::new()),
            event_tx,
            max_history: MAX_HISTORY_PER_SESSION,
            subagent_factory: None,
            subagent_contexts: Arc::new(DashMap::new()),
        }
    }
    #[allow(dead_code)] // builder API, exercised by tests
    pub fn with_subagent_factory(mut self, factory: SubagentFactory) -> Self {
        self.subagent_factory = Some(Arc::new(factory));
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

        jobs.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        jobs
    }

    pub fn get_job_result(&self, job_id: &JobId) -> Option<JobResult> {
        self.get_job_result_for_session(job_id, None)
    }

    pub fn get_job_result_for_session(
        &self,
        job_id: &JobId,
        session_id: Option<&str>,
    ) -> Option<JobResult> {
        if let Some(entry) = self.running.get(job_id) {
            if let Some(sid) = session_id {
                if entry.info.session_id != sid {
                    return None;
                }
            }
            return Some(JobResult {
                info: entry.info.clone(),
                output: None,
                error: None,
                exit_code: None,
            });
        }

        for entry in self.history.iter() {
            if let Some(sid) = session_id {
                if entry.key() != sid {
                    continue;
                }
            }
            for result in entry.value().iter() {
                if result.info.id == *job_id {
                    return Some(result.clone());
                }
            }
        }

        None
    }

    pub async fn cancel_job(&self, job_id: &JobId) -> bool {
        self.cancel_job_for_session(job_id, None).await
    }

    pub async fn cancel_job_for_session(
        &self,
        job_id: &JobId,
        caller_session_id: Option<&str>,
    ) -> bool {
        if let Some(sid) = caller_session_id {
            if let Some(entry) = self.running.get(job_id) {
                if entry.info.session_id != sid {
                    warn!(
                        job_id = %job_id,
                        owner = %entry.info.session_id,
                        caller = %sid,
                        "Session tried to cancel job owned by another session"
                    );
                    return false;
                }
            }
        }

        let Some((_, running_job)) = self.running.remove(job_id) else {
            warn!(job_id = %job_id, "Job not found for cancellation");
            return false;
        };

        let _ = running_job.cancel_tx.send(());

        let mut info = running_job.info;
        info.mark_cancelled();
        let job_session_id = info.session_id.clone();
        let job_result = JobResult::failure(info, "Job cancelled".to_string());

        let kind = match &job_result.info.kind {
            JobKind::Bash { .. } => "bash",
            JobKind::Subagent { .. } => "subagent",
        };
        Self::emit_background_completed(&self.event_tx, &job_session_id, job_id, &job_result, kind);
        Self::add_to_history(&self.history, &job_session_id, job_result, self.max_history);

        info!(job_id = %job_id, "Job cancelled");
        true
    }
    #[allow(dead_code)] // lifecycle API, exercised by tests
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
        }

        debug!(session_id = %session_id, "Session cleanup completed");
    }
    #[allow(dead_code)] // diagnostic API, exercised by tests
    pub fn running_count(&self, session_id: &str) -> usize {
        self.running
            .iter()
            .filter(|entry| entry.value().info.session_id == session_id)
            .count()
    }
    #[allow(dead_code)] // diagnostic API, exercised by tests
    pub fn total_running_count(&self) -> usize {
        self.running.len()
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
