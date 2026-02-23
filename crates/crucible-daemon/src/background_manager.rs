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
//! ```ignore
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
use crucible_observe::events::LogEvent;
use crucible_observe::session::SessionWriter;
use dashmap::DashMap;
use futures::StreamExt;
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

const DEFAULT_BASH_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_HISTORY_PER_SESSION: usize = 50;
const DEFAULT_SUBAGENT_MAX_TURNS: usize = 10;
/// Maximum output size for subagent accumulated output (10 MB)
const MAX_SUBAGENT_OUTPUT: usize = 10 * 1024 * 1024;

mod events {
    pub const BASH_SPAWNED: &str = "bash_job_spawned";
    pub const BASH_COMPLETED: &str = "bash_job_completed";
    pub const BASH_FAILED: &str = "bash_job_failed";
    pub const SUBAGENT_SPAWNED: &str = "subagent_spawned";
    pub const SUBAGENT_COMPLETED: &str = "subagent_completed";
    pub const SUBAGENT_FAILED: &str = "subagent_failed";
    pub const BACKGROUND_COMPLETED: &str = "background_job_completed";
    pub const DELEGATION_SPAWNED: &str = "delegation_spawned";
    pub const DELEGATION_COMPLETED: &str = "delegation_completed";
    pub const DELEGATION_FAILED: &str = "delegation_failed";
}

#[derive(Error, Debug)]
pub enum BackgroundError {
    #[error("Job error: {0}")]
    Job(#[from] JobError),

    #[error("Failed to spawn job: {0}")]
    SpawnFailed(String),

    #[error("No subagent factory configured")]
    NoSubagentFactory,
}

pub type SubagentFactory = Box<
    dyn Fn(
            &SessionAgent,
            &Path,
        ) -> Pin<
            Box<dyn Future<Output = Result<Box<dyn AgentHandle + Send + Sync>, String>> + Send>,
        > + Send
        + Sync,
>;

struct RunningJob {
    info: JobInfo,
    is_delegation: bool,
    parent_session_id: Option<String>,
    cancel_tx: oneshot::Sender<()>,
    #[allow(dead_code)]
    task_handle: JoinHandle<()>,
}

pub struct SubagentContext {
    pub agent: SessionAgent,
    pub available_agents: HashMap<String, AgentProfile>,
    pub workspace: PathBuf,
    pub parent_session_id: Option<String>,
    /// Parent session directory for creating subagent session files
    pub parent_session_dir: Option<PathBuf>,
    pub delegator_agent_name: Option<String>,
    pub target_agent_name: Option<String>,
    /// Delegation depth counter (0 = root, 1 = first delegation, etc.)
    pub delegation_depth: u32,
}

struct PreparedSubagentExecution {
    info: JobInfo,
    prompt: String,
    context: Option<String>,
    session_id: String,
    session_link: String,
    agent: Box<dyn AgentHandle + Send + Sync>,
    subagent_writer: Option<Arc<Mutex<SessionWriter>>>,
    is_delegation: bool,
    parent_session_id: Option<String>,
}

struct SubagentExecutionOptions {
    max_turns: usize,
    max_output_bytes: usize,
    timeout: Option<Duration>,
}

fn parse_target_agent_name(context: Option<&str>) -> Option<String> {
    const TARGET_PREFIX: &str = "Target agent: ";
    context.and_then(|ctx| {
        ctx.lines().find_map(|line| {
            line.strip_prefix(TARGET_PREFIX)
                .map(str::trim)
                .filter(|target| !target.is_empty())
                .map(ToString::to_string)
        })
    })
}

fn target_profile_to_session_agent(
    target_name: &str,
    available_agents: &HashMap<String, AgentProfile>,
) -> Result<SessionAgent, BackgroundError> {
    let profile = available_agents.get(target_name).ok_or_else(|| {
        let mut available: Vec<_> = available_agents.keys().cloned().collect();
        available.sort();
        let available_list = if available.is_empty() {
            "(none)".to_string()
        } else {
            available.join(", ")
        };
        BackgroundError::SpawnFailed(format!(
            "Delegation target '{target_name}' not found. Available agents: {available_list}"
        ))
    })?;

    Ok(SessionAgent::from_profile(profile, target_name))
}

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

    #[allow(dead_code)]
    pub fn with_subagent_factory(mut self, factory: SubagentFactory) -> Self {
        self.subagent_factory = Some(Arc::new(factory));
        self
    }

    pub fn register_subagent_context(&self, session_id: &str, config: SubagentContext) {
        self.subagent_contexts
            .insert(session_id.to_string(), config);
    }

    #[allow(dead_code)]
    pub fn unregister_subagent_context(&self, session_id: &str) {
        self.subagent_contexts.remove(session_id);
    }

    pub async fn spawn_bash(
        &self,
        session_id: &str,
        command: String,
        workdir: Option<PathBuf>,
        timeout: Option<Duration>,
    ) -> Result<JobId, BackgroundError> {
        let kind = JobKind::Bash {
            command: command.clone(),
            workdir: workdir.clone(),
        };
        let info = JobInfo::new(session_id.to_string(), kind);
        let job_id = info.id.clone();
        let timeout = timeout.unwrap_or(DEFAULT_BASH_TIMEOUT);
        let (cancel_tx, cancel_rx) = oneshot::channel();

        let _ = emit_event(
            &self.event_tx,
            SessionEventMessage::new(
                session_id,
                events::BASH_SPAWNED,
                serde_json::json!({
                    "job_id": job_id,
                    "command": command,
                }),
            ),
        );

        info!(
            job_id = %job_id,
            session_id = %session_id,
            command = %command,
            "Spawning background bash job"
        );

        let task_handle = {
            let running = self.running.clone();
            let history = self.history.clone();
            let event_tx = self.event_tx.clone();
            let job_id = job_id.clone();
            let session_id = session_id.to_string();
            let max_history = self.max_history;
            let command = command.clone();

            tokio::spawn(async move {
                let result = Self::execute_bash_with_cancellation(
                    command.clone(),
                    workdir,
                    timeout,
                    cancel_rx,
                )
                .await;

                // Extract original JobInfo to preserve started_at timestamp
                let info = running
                    .remove(&job_id)
                    .map(|(_, rt)| rt.info)
                    .unwrap_or_else(|| {
                        // Fallback: job was already removed (shouldn't happen)
                        JobInfo::new(
                            session_id.clone(),
                            JobKind::Bash {
                                command: command.clone(),
                                workdir: None,
                            },
                        )
                    });

                let job_result = Self::build_job_result(info, result);
                Self::emit_completion_events(
                    &event_tx,
                    &session_id,
                    &job_result.info.id.clone(),
                    &job_result,
                );
                Self::add_to_history(&history, &session_id, job_result, max_history);

                debug!(job_id = %job_id, "Background bash job completed");
            })
        };

        self.running.insert(
            job_id.clone(),
            RunningJob {
                info,
                is_delegation: false,
                parent_session_id: None,
                cancel_tx,
                task_handle,
            },
        );

        Ok(job_id)
    }

    fn build_job_result(mut info: JobInfo, result: Result<(String, i32), BashError>) -> JobResult {
        match result {
            Ok((output, exit_code)) => {
                info.mark_completed();
                JobResult::success_with_exit_code(info, output, exit_code)
            }
            Err(BashError::Cancelled) => {
                info.mark_cancelled();
                JobResult::failure(info, "Job cancelled".to_string())
            }
            Err(BashError::Timeout) => {
                info.mark_failed();
                JobResult::failure(info, "Job timed out".to_string())
            }
            Err(BashError::Failed { message, exit_code }) => {
                info.mark_failed();
                match exit_code {
                    Some(code) => JobResult::failure_with_exit_code(info, message, code),
                    None => JobResult::failure(info, message),
                }
            }
        }
    }

    fn emit_completion_events(
        event_tx: &broadcast::Sender<SessionEventMessage>,
        session_id: &str,
        job_id: &JobId,
        result: &JobResult,
    ) {
        let (event_type, event_data) = if result.is_success() {
            let output = result.output.as_deref().unwrap_or("");
            (
                events::BASH_COMPLETED,
                serde_json::json!({
                    "job_id": job_id,
                    "output": truncate(output, 1000),
                    "exit_code": result.exit_code,
                }),
            )
        } else {
            let error = result.error.as_deref().unwrap_or("Unknown error");
            (
                events::BASH_FAILED,
                serde_json::json!({
                    "job_id": job_id,
                    "error": error,
                    "exit_code": result.exit_code,
                }),
            )
        };

        if !emit_event(
            event_tx,
            SessionEventMessage::new(session_id, event_type, event_data),
        ) {
            warn!(job_id = %job_id, "No subscribers for bash completion event");
        }
        Self::emit_background_completed(event_tx, session_id, job_id, result, "bash");
    }

    fn emit_background_completed(
        event_tx: &broadcast::Sender<SessionEventMessage>,
        session_id: &str,
        job_id: &JobId,
        result: &JobResult,
        kind: &str,
    ) {
        let summary = result.truncated_output(500);
        let summary = if summary.is_empty() {
            result
                .error
                .clone()
                .unwrap_or_else(|| "completed".to_string())
        } else {
            summary
        };

        if !emit_event(
            event_tx,
            SessionEventMessage::new(
                session_id,
                events::BACKGROUND_COMPLETED,
                serde_json::json!({
                    "job_id": job_id,
                    "kind": kind,
                    "summary": summary,
                }),
            ),
        ) {
            warn!(job_id = %job_id, kind = %kind, "No subscribers for background completion event");
        }
    }

    async fn execute_bash_with_cancellation(
        command: String,
        workdir: Option<PathBuf>,
        timeout: Duration,
        cancel_rx: oneshot::Receiver<()>,
    ) -> Result<(String, i32), BashError> {
        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(&command);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }

        let mut child = cmd.spawn().map_err(|e| BashError::Failed {
            message: format!("Spawn error: {e}"),
            exit_code: None,
        })?;

        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();

        let wait_and_collect = async {
            let status = child.wait().await?;

            let stdout = if let Some(mut h) = stdout_handle {
                use tokio::io::AsyncReadExt;
                let mut buf = Vec::new();
                h.read_to_end(&mut buf).await?;
                String::from_utf8_lossy(&buf).to_string()
            } else {
                String::new()
            };

            let stderr = if let Some(mut h) = stderr_handle {
                use tokio::io::AsyncReadExt;
                let mut buf = Vec::new();
                h.read_to_end(&mut buf).await?;
                String::from_utf8_lossy(&buf).to_string()
            } else {
                String::new()
            };

            Ok::<_, std::io::Error>((status, stdout, stderr))
        };

        tokio::select! {
            _ = cancel_rx => {
                let _ = child.kill().await;
                Err(BashError::Cancelled)
            }
            result = tokio::time::timeout(timeout, wait_and_collect) => {
                match result {
                    Ok(Ok((status, stdout, stderr))) => {
                        let exit_code = status.code().unwrap_or(-1);

                        if status.success() {
                            Ok((stdout, exit_code))
                        } else {
                            Err(BashError::Failed {
                                message: format!("Exit code: {exit_code}\nStdout:\n{stdout}\nStderr:\n{stderr}"),
                                exit_code: Some(exit_code),
                            })
                        }
                    }
                    Ok(Err(e)) => {
                        Err(BashError::Failed {
                            message: format!("Exec error: {e}"),
                            exit_code: None,
                        })
                    }
                    Err(_) => {
                        let _ = child.kill().await;
                        Err(BashError::Timeout)
                    }
                }
            }
        }
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn running_count(&self, session_id: &str) -> usize {
        self.running
            .iter()
            .filter(|entry| entry.value().info.session_id == session_id)
            .count()
    }

    #[allow(dead_code)]
    pub fn total_running_count(&self) -> usize {
        self.running.len()
    }

    pub async fn spawn_subagent(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
    ) -> Result<JobId, BackgroundError> {
        let prepared = self
            .prepare_subagent_execution(session_id, prompt, context)
            .await?;
        let job_id = prepared.info.id.clone();
        let (cancel_tx, cancel_rx) = oneshot::channel();

        let task_handle = {
            let running = self.running.clone();
            let history = self.history.clone();
            let event_tx = self.event_tx.clone();
            let job_id = job_id.clone();
            let session_id = prepared.session_id.clone();
            let max_history = self.max_history;
            let session_link = prepared.session_link.clone();
            let fallback_prompt = prepared.prompt.clone();
            let fallback_context = prepared.context.clone();
            let agent = prepared.agent;
            let prompt = prepared.prompt;
            let context = prepared.context;
            let subagent_writer = prepared.subagent_writer;

            tokio::spawn(async move {
                let result = Self::execute_subagent_with_options(
                    agent,
                    prompt.clone(),
                    context,
                    cancel_rx,
                    subagent_writer,
                    SubagentExecutionOptions {
                        max_turns: DEFAULT_SUBAGENT_MAX_TURNS,
                        max_output_bytes: MAX_SUBAGENT_OUTPUT,
                        timeout: None,
                    },
                )
                .await;

                // Extract original JobInfo and delegation metadata to preserve started_at timestamp
                let (info, job_is_delegation, job_parent_session_id) = running
                    .remove(&job_id)
                    .map(|(_, rt)| (rt.info, rt.is_delegation, rt.parent_session_id))
                    .unwrap_or_else(|| {
                        (
                            JobInfo::new(
                                session_id.clone(),
                                JobKind::Subagent {
                                    prompt: fallback_prompt,
                                    context: fallback_context,
                                },
                            ),
                            false,
                            None,
                        )
                    });

                let job_result = Self::build_subagent_result(info, result);
                Self::emit_subagent_completion_events(
                    &event_tx,
                    &session_id,
                    &job_result.info.id.clone(),
                    &job_result,
                    &session_link,
                    job_is_delegation,
                    job_parent_session_id.as_deref(),
                );
                Self::add_to_history(&history, &session_id, job_result, max_history);

                debug!(job_id = %job_id, "Background subagent completed");
            })
        };

        self.running.insert(
            job_id.clone(),
            RunningJob {
                info: prepared.info,
                is_delegation: prepared.is_delegation,
                parent_session_id: prepared.parent_session_id,
                cancel_tx,
                task_handle,
            },
        );

        Ok(job_id)
    }

    pub async fn spawn_subagent_blocking(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
        config: SubagentBlockingConfig,
        cancel_rx: Option<oneshot::Receiver<()>>,
    ) -> Result<JobResult, BackgroundError> {
        // KNOWN LIMITATION: Blocking delegation does not support streaming responses.
        // The subagent's output is collected entirely before returning to the caller.
        // Streaming delegation is a future enhancement that would require async streaming
        // channels and client-side buffering. For now, blocking mode is synchronous only.
        let prepared = self
            .prepare_subagent_execution(session_id, prompt, context)
            .await?;

        let mut cancel_tx_keepalive = None;
        let cancel_rx = match cancel_rx {
            Some(rx) => rx,
            None => {
                let (cancel_tx, cancel_rx) = oneshot::channel();
                cancel_tx_keepalive = Some(cancel_tx);
                cancel_rx
            }
        };

        let result = Self::execute_subagent_with_options(
            prepared.agent,
            prepared.prompt,
            prepared.context,
            cancel_rx,
            prepared.subagent_writer,
            SubagentExecutionOptions {
                max_turns: DEFAULT_SUBAGENT_MAX_TURNS,
                max_output_bytes: config.result_max_bytes,
                timeout: Some(config.timeout),
            },
        )
        .await;
        drop(cancel_tx_keepalive);

        let job_result = Self::build_subagent_result(prepared.info, result);
        Self::emit_subagent_completion_events(
            &self.event_tx,
            &prepared.session_id,
            &job_result.info.id.clone(),
            &job_result,
            &prepared.session_link,
            prepared.is_delegation,
            prepared.parent_session_id.as_deref(),
        );
        Self::add_to_history(
            &self.history,
            &prepared.session_id,
            job_result.clone(),
            self.max_history,
        );

        Ok(job_result)
    }

    async fn prepare_subagent_execution(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
    ) -> Result<PreparedSubagentExecution, BackgroundError> {
        let factory = self
            .subagent_factory
            .as_ref()
            .ok_or(BackgroundError::NoSubagentFactory)?;

        let (
            parent_agent_config,
            available_agents,
            workspace,
            parent_session_dir,
            parent_session_id,
            delegator_name,
            default_target_name,
            delegation_depth,
        ) = {
            let ctx = self.subagent_contexts.get(session_id).ok_or_else(|| {
                BackgroundError::SpawnFailed("Subagent context not registered".into())
            })?;
            (
                ctx.agent.clone(),
                ctx.available_agents.clone(),
                ctx.workspace.clone(),
                ctx.parent_session_dir.clone(),
                ctx.parent_session_id.clone(),
                ctx.delegator_agent_name.clone(),
                ctx.target_agent_name.clone(),
                ctx.delegation_depth,
            )
        };
        let mut agent_config = parent_agent_config.clone();

        let requested_target_name = parse_target_agent_name(context.as_deref());
        let effective_target_name = requested_target_name
            .clone()
            .or_else(|| default_target_name.clone());

        if let Some(target_name) = requested_target_name {
            agent_config = target_profile_to_session_agent(&target_name, &available_agents)?;
        }

        let is_delegation = parent_session_id.is_some();
        let child_delegation_depth = delegation_depth.saturating_add(1);
        let child_parent_session_id = parent_session_id.clone();

        self.enforce_delegation_capabilities(
            &parent_agent_config,
            delegator_name.as_deref(),
            effective_target_name.as_deref(),
            child_delegation_depth,
            session_id,
        )?;

        // KNOWN LIMITATION: No nested delegation (depth=1 only).
        // Subagents cannot spawn their own subagents. This is enforced by clearing
        // the delegation_config before passing the agent to the subagent factory.
        // Future versions could support configurable nesting depth with proper
        // authorization checks at each level.
        agent_config.delegation_config = None;

        let kind = JobKind::Subagent {
            prompt: prompt.clone(),
            context: context.clone(),
        };
        let mut info = JobInfo::new(session_id.to_string(), kind);
        let job_id = info.id.clone();

        let (subagent_writer, session_link, child_session_id) = if let Some(ref parent_dir) =
            parent_session_dir
        {
            match SessionWriter::create_subagent(parent_dir).await {
                Ok((mut writer, link)) => {
                    let subagent_session_id = writer.id().as_str().to_string();
                    if let Some(ref parent_id) = child_parent_session_id {
                        let metadata = serde_json::json!({
                            "delegation_metadata": {
                                "parent_session_id": parent_id,
                                "delegation_depth": child_delegation_depth,
                            }
                        })
                        .to_string();
                        if let Err(e) = writer.append(LogEvent::system(metadata)).await {
                            warn!(error = %e, "Failed to write delegation metadata to child session");
                        }
                    }

                    info.session_path = Some(writer.session_dir().to_path_buf());
                    (
                        Some(Arc::new(Mutex::new(writer))),
                        link,
                        Some(subagent_session_id),
                    )
                }
                Err(e) => {
                    warn!(error = %e, "Failed to create subagent session, continuing without persistence");
                    (None, format!("[[subagent:{}]]", job_id), None)
                }
            }
        } else {
            (None, format!("[[subagent:{}]]", job_id), None)
        };

        if is_delegation {
            let child_context_key = child_session_id.unwrap_or_else(|| job_id.clone());
            self.subagent_contexts.insert(
                child_context_key,
                SubagentContext {
                    agent: agent_config.clone(),
                    available_agents: available_agents.clone(),
                    workspace: workspace.clone(),
                    parent_session_id: child_parent_session_id,
                    parent_session_dir: info.session_path.clone(),
                    delegator_agent_name: effective_target_name.clone(),
                    target_agent_name: None,
                    delegation_depth: child_delegation_depth,
                },
            );
        }

        let _ = emit_event(
            &self.event_tx,
            SessionEventMessage::new(
                session_id,
                events::SUBAGENT_SPAWNED,
                serde_json::json!({
                    "job_id": job_id,
                    "session_link": session_link,
                    "prompt": truncate(&prompt, 100),
                }),
            ),
        );

        if is_delegation {
            if let Some(ref parent_id) = parent_session_id {
                let _ = emit_event(
                    &self.event_tx,
                    SessionEventMessage::new(
                        parent_id,
                        events::DELEGATION_SPAWNED,
                        serde_json::json!({
                            "delegation_id": job_id,
                            "prompt": truncate(&prompt, 100),
                            "parent_session_id": parent_id,
                            "target_agent": effective_target_name,
                        }),
                    ),
                );
            }
        }

        info!(
            job_id = %job_id,
            session_id = %session_id,
            session_link = %session_link,
            prompt_len = prompt.len(),
            "Spawning background subagent"
        );

        let agent = factory(&agent_config, &workspace)
            .await
            .map_err(BackgroundError::SpawnFailed)?;

        Ok(PreparedSubagentExecution {
            info,
            prompt,
            context,
            session_id: session_id.to_string(),
            session_link,
            agent,
            subagent_writer,
            is_delegation,
            parent_session_id,
        })
    }

    fn enforce_delegation_capabilities(
        &self,
        session_agent: &SessionAgent,
        delegator_name: Option<&str>,
        target_name: Option<&str>,
        delegation_depth: u32,
        parent_session_id: &str,
    ) -> Result<(), BackgroundError> {
        if delegation_depth >= 3 {
            return Err(BackgroundError::SpawnFailed(
                "Delegation depth limit exceeded (hard cap at 3)".to_string(),
            ));
        }

        let delegation = session_agent
            .delegation_config
            .as_ref()
            .filter(|cfg| cfg.enabled)
            .ok_or_else(|| {
                BackgroundError::SpawnFailed("Delegation is disabled for this agent".to_string())
            })?;

        let active_delegations = self
            .running
            .iter()
            .filter(|entry| {
                entry.value().is_delegation && entry.value().info.session_id == parent_session_id
            })
            .count();

        if active_delegations >= delegation.max_concurrent_delegations as usize {
            return Err(BackgroundError::SpawnFailed(format!(
                "Maximum concurrent delegations ({}) exceeded",
                delegation.max_concurrent_delegations
            )));
        }

        if let Some(allowed_targets) = &delegation.allowed_targets {
            let target = target_name.ok_or_else(|| {
                BackgroundError::SpawnFailed(
                    "Delegation target could not be determined".to_string(),
                )
            })?;

            if !allowed_targets.iter().any(|allowed| allowed == target) {
                return Err(BackgroundError::SpawnFailed(format!(
                    "Delegation target '{target}' is not allowed"
                )));
            }
        }

        if let (Some(delegator), Some(target)) = (delegator_name, target_name) {
            if delegator == target {
                return Err(BackgroundError::SpawnFailed(
                    "Delegation rejected by self-delegation guard".to_string(),
                ));
            }
        }

        Ok(())
    }

    async fn execute_subagent_with_options(
        mut agent: Box<dyn AgentHandle + Send + Sync>,
        prompt: String,
        context: Option<String>,
        mut cancel_rx: oneshot::Receiver<()>,
        session_writer: Option<Arc<Mutex<SessionWriter>>>,
        options: SubagentExecutionOptions,
    ) -> Result<String, SubagentError> {
        let SubagentExecutionOptions {
            max_turns,
            max_output_bytes,
            timeout,
        } = options;
        let execute = async {
            let full_prompt = match context {
                Some(ctx) => format!("{}\n\n{}", ctx, prompt),
                None => prompt.clone(),
            };

            if let Some(ref writer) = session_writer {
                let mut w = writer.lock().await;
                if let Err(e) = w.append(LogEvent::user(&full_prompt)).await {
                    error!(error = %e, "Failed to write user event to subagent session");
                }
            }

            let mut accumulated_output = String::new();
            let mut turns = 0;

            while turns < max_turns {
                turns += 1;
                let input = if turns == 1 {
                    full_prompt.clone()
                } else {
                    "Continue with the task.".to_string()
                };

                let mut stream = agent.send_message_stream(input);
                let mut turn_output = String::new();
                let mut has_tool_calls = false;

                loop {
                    tokio::select! {
                        _ = &mut cancel_rx => {
                            return Err(SubagentError::Cancelled);
                        }
                        chunk = stream.next() => {
                            match chunk {
                                Some(Ok(c)) => {
                                    turn_output.push_str(&c.delta);
                                    if c.tool_calls.is_some() {
                                        has_tool_calls = true;
                                    }
                                    if c.done {
                                        break;
                                    }
                                }
                                Some(Err(e)) => {
                                    return Err(SubagentError::Failed(e.to_string()));
                                }
                                None => break,
                            }
                        }
                    }
                }

                if let Some(ref writer) = session_writer {
                    let mut w = writer.lock().await;
                    if let Err(e) = w.append(LogEvent::assistant(&turn_output)).await {
                        error!(error = %e, "Failed to write assistant event to subagent session");
                    }
                }

                accumulated_output.push_str(&turn_output);
                accumulated_output.push('\n');

                if accumulated_output.len() > max_output_bytes {
                    accumulated_output.truncate(max_output_bytes);
                    accumulated_output.push_str("\n\n[Output truncated due to size limit]");
                    break;
                }

                if !has_tool_calls {
                    break;
                }
            }

            Ok(accumulated_output.trim().to_string())
        };

        let mut output = if let Some(timeout_duration) = timeout {
            match tokio::time::timeout(timeout_duration, execute).await {
                Ok(inner) => inner?,
                Err(_) => return Err(SubagentError::Timeout),
            }
        } else {
            execute.await?
        };

        if output.len() > max_output_bytes {
            output = truncate(&output, max_output_bytes);
        }

        Ok(output)
    }

    fn build_subagent_result(
        mut info: JobInfo,
        result: Result<String, SubagentError>,
    ) -> JobResult {
        match result {
            Ok(output) => {
                info.mark_completed();
                JobResult::success(info, output)
            }
            Err(SubagentError::Cancelled) => {
                info.mark_cancelled();
                JobResult::failure(info, "Subagent cancelled".to_string())
            }
            Err(SubagentError::Failed(msg)) => {
                info.mark_failed();
                JobResult::failure(info, msg)
            }
            Err(SubagentError::Timeout) => {
                info.mark_failed();
                JobResult::failure(info, "Subagent timed out".to_string())
            }
        }
    }

    fn emit_subagent_completion_events(
        event_tx: &broadcast::Sender<SessionEventMessage>,
        session_id: &str,
        job_id: &JobId,
        result: &JobResult,
        session_link: &str,
        is_delegation: bool,
        parent_session_id: Option<&str>,
    ) {
        let (is_success, output_or_error) = if result.is_success() {
            (true, result.output.as_deref().unwrap_or(""))
        } else {
            (false, result.error.as_deref().unwrap_or("Unknown error"))
        };

        let (event_type, event_data) = if is_success {
            (
                events::SUBAGENT_COMPLETED,
                serde_json::json!({
                    "job_id": job_id,
                    "session_link": session_link,
                    "summary": truncate(output_or_error, 500),
                }),
            )
        } else {
            (
                events::SUBAGENT_FAILED,
                serde_json::json!({
                    "job_id": job_id,
                    "session_link": session_link,
                    "error": output_or_error,
                }),
            )
        };

        if !emit_event(
            event_tx,
            SessionEventMessage::new(session_id, event_type, event_data),
        ) {
            warn!(job_id = %job_id, "No subscribers for subagent completion event");
        }

        if let Some(parent_id) = parent_session_id.filter(|_| is_delegation) {
            let (deleg_type, deleg_data) = if is_success {
                (
                    events::DELEGATION_COMPLETED,
                    serde_json::json!({
                        "delegation_id": job_id,
                        "result_summary": truncate(output_or_error, 500),
                        "parent_session_id": parent_id,
                    }),
                )
            } else {
                (
                    events::DELEGATION_FAILED,
                    serde_json::json!({
                        "delegation_id": job_id,
                        "error": output_or_error,
                        "parent_session_id": parent_id,
                    }),
                )
            };

            let _ = emit_event(
                event_tx,
                SessionEventMessage::new(parent_id, deleg_type, deleg_data),
            );
        }

        Self::emit_background_completed(event_tx, session_id, job_id, result, "subagent");
    }
}

enum SubagentError {
    Cancelled,
    Failed(String),
    Timeout,
}

#[async_trait]
impl BackgroundSpawner for BackgroundJobManager {
    async fn spawn_bash(
        &self,
        session_id: &str,
        command: String,
        workdir: Option<PathBuf>,
        timeout: Option<Duration>,
    ) -> Result<JobId, JobError> {
        self.spawn_bash(session_id, command, workdir, timeout)
            .await
            .map_err(|e| JobError::SpawnFailed(e.to_string()))
    }

    fn list_jobs(&self, session_id: &str) -> Vec<JobInfo> {
        BackgroundJobManager::list_jobs(self, session_id)
    }

    fn get_job_result(&self, job_id: &JobId) -> Option<JobResult> {
        BackgroundJobManager::get_job_result(self, job_id)
    }

    async fn cancel_job(&self, job_id: &JobId) -> bool {
        BackgroundJobManager::cancel_job(self, job_id).await
    }

    async fn spawn_subagent(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
    ) -> Result<JobId, JobError> {
        BackgroundJobManager::spawn_subagent(self, session_id, prompt, context)
            .await
            .map_err(|e| JobError::SpawnFailed(e.to_string()))
    }

    async fn spawn_subagent_blocking(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
        config: SubagentBlockingConfig,
        cancel_rx: Option<oneshot::Receiver<()>>,
    ) -> Result<JobResult, JobError> {
        BackgroundJobManager::spawn_subagent_blocking(
            self, session_id, prompt, context, config, cancel_rx,
        )
        .await
        .map_err(|e| JobError::SpawnFailed(e.to_string()))
    }
}

enum BashError {
    Cancelled,
    Timeout,
    Failed {
        message: String,
        exit_code: Option<i32>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_config::{AgentProfile, BackendType, DelegationConfig};
    use crucible_core::background::JobStatus;
    use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatError, ChatResult};
    use futures::stream::{self, BoxStream};
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;
    use std::time::Instant;
    use tokio::sync::broadcast;

    fn create_manager() -> BackgroundJobManager {
        let (tx, _) = broadcast::channel(16);
        BackgroundJobManager::new(tx)
    }

    #[derive(Clone)]
    enum MockSubagentBehavior {
        ImmediateSuccess(String),
        DelayedSuccess { output: String, delay: Duration },
        DelayedFailure { error: String, delay: Duration },
        Pending,
        StreamFailure(String),
    }

    struct MockSubagentHandle {
        behavior: MockSubagentBehavior,
    }

    impl MockSubagentHandle {
        fn new(behavior: MockSubagentBehavior) -> Self {
            Self { behavior }
        }
    }

    fn chunk(delta: String, done: bool) -> ChatChunk {
        ChatChunk {
            delta,
            done,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
        }
    }

    #[async_trait]
    impl AgentHandle for MockSubagentHandle {
        fn send_message_stream(
            &mut self,
            _message: String,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            match self.behavior.clone() {
                MockSubagentBehavior::ImmediateSuccess(output) => {
                    Box::pin(stream::iter(vec![Ok(chunk(output, true))]))
                }
                MockSubagentBehavior::DelayedSuccess { output, delay } => {
                    Box::pin(stream::once(async move {
                        tokio::time::sleep(delay).await;
                        Ok(chunk(output, true))
                    }))
                }
                MockSubagentBehavior::DelayedFailure { error, delay } => {
                    Box::pin(stream::once(async move {
                        tokio::time::sleep(delay).await;
                        Err(ChatError::Internal(error))
                    }))
                }
                MockSubagentBehavior::Pending => Box::pin(stream::pending()),
                MockSubagentBehavior::StreamFailure(message) => {
                    Box::pin(stream::iter(vec![Err(ChatError::Internal(message))]))
                }
            }
        }

        async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }
    }

    fn test_session_agent(delegation_config: Option<DelegationConfig>) -> SessionAgent {
        SessionAgent {
            agent_type: "acp".to_string(),
            agent_name: Some("test-agent".to_string()),
            provider_key: None,
            provider: BackendType::Custom,
            model: "test-agent".to_string(),
            system_prompt: String::new(),
            temperature: None,
            max_tokens: None,
            max_context_tokens: None,
            thinking_budget: None,
            endpoint: None,
            env_overrides: HashMap::new(),
            mcp_servers: vec![],
            agent_card_name: None,
            capabilities: None,
            agent_description: None,
            delegation_config,
            precognition_enabled: false,
        }
    }

    fn default_enabled_delegation_config() -> DelegationConfig {
        DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }
    }

    fn test_agent_profile(command: &str, args: &[&str]) -> AgentProfile {
        AgentProfile {
            extends: None,
            command: Some(command.to_string()),
            args: Some(args.iter().map(|arg| arg.to_string()).collect()),
            env: HashMap::new(),
            description: Some("delegation test target".to_string()),
            capabilities: None,
            delegation: None,
        }
    }

    fn make_subagent_manager_with_factory_and_identity(
        factory: SubagentFactory,
        delegation_config: Option<DelegationConfig>,
        delegator_agent_name: Option<&str>,
        target_agent_name: Option<&str>,
    ) -> BackgroundJobManager {
        let delegation_config =
            delegation_config.or_else(|| Some(default_enabled_delegation_config()));
        let (tx, _) = broadcast::channel(16);
        let manager = BackgroundJobManager::new(tx).with_subagent_factory(factory);
        manager.register_subagent_context(
            "session-1",
            SubagentContext {
                agent: test_session_agent(delegation_config),
                available_agents: HashMap::new(),
                workspace: std::env::temp_dir(),
                parent_session_id: Some("session-1".to_string()),
                parent_session_dir: None,
                delegator_agent_name: delegator_agent_name.map(str::to_string),
                target_agent_name: target_agent_name.map(str::to_string),
                delegation_depth: 0,
            },
        );
        manager
    }

    fn make_subagent_manager_with_factory(
        factory: SubagentFactory,
        delegation_config: Option<DelegationConfig>,
    ) -> BackgroundJobManager {
        make_subagent_manager_with_factory_and_identity(
            factory,
            delegation_config,
            Some("parent-agent"),
            Some("worker-agent"),
        )
    }

    fn make_subagent_manager_with_factory_and_events(
        factory: SubagentFactory,
        delegation_config: Option<DelegationConfig>,
    ) -> (
        BackgroundJobManager,
        broadcast::Receiver<SessionEventMessage>,
    ) {
        let delegation_config =
            delegation_config.or_else(|| Some(default_enabled_delegation_config()));
        let (tx, rx) = broadcast::channel(32);
        let manager = BackgroundJobManager::new(tx).with_subagent_factory(factory);
        manager.register_subagent_context(
            "session-1",
            SubagentContext {
                agent: test_session_agent(delegation_config),
                available_agents: HashMap::new(),
                workspace: std::env::temp_dir(),
                parent_session_id: Some("session-1".to_string()),
                parent_session_dir: None,
                delegator_agent_name: Some("parent-agent".to_string()),
                target_agent_name: Some("worker-agent".to_string()),
                delegation_depth: 0,
            },
        );
        (manager, rx)
    }

    fn behavior_factory(behavior: MockSubagentBehavior) -> SubagentFactory {
        Box::new(move |_agent, _workspace| {
            let behavior = behavior.clone();
            Box::pin(async move {
                Ok(Box::new(MockSubagentHandle::new(behavior))
                    as Box<dyn AgentHandle + Send + Sync>)
            })
        })
    }

    #[tokio::test]
    async fn spawn_subagent_blocking_returns_job_result_with_output() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::DelayedSuccess {
                output: "blocking-complete".to_string(),
                delay: Duration::from_millis(75),
            }),
            None,
        );
        let start = Instant::now();

        let result: Result<JobResult, BackgroundError> = manager
            .spawn_subagent_blocking(
                "session-1",
                "do it".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await;

        let result = result.expect("blocking subagent should complete");
        assert!(start.elapsed() >= Duration::from_millis(70));
        assert_eq!(result.info.status, JobStatus::Completed);
        assert_eq!(result.output.as_deref(), Some("blocking-complete"));
    }

    #[tokio::test]
    async fn spawn_subagent_blocking_timeout_returns_failed_job_result() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::Pending),
            None,
        );

        let result = manager
            .spawn_subagent_blocking(
                "session-1",
                "do it".to_string(),
                None,
                SubagentBlockingConfig {
                    timeout: Duration::from_millis(50),
                    result_max_bytes: 51200,
                },
                None,
            )
            .await
            .expect("timeout should return JobResult");

        assert_eq!(result.info.status, JobStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("timed out"));
    }

    #[tokio::test]
    async fn spawn_subagent_blocking_cancellation_marks_job_cancelled() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::Pending),
            None,
        );
        let (cancel_tx, cancel_rx) = oneshot::channel();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            let _ = cancel_tx.send(());
        });

        let result = manager
            .spawn_subagent_blocking(
                "session-1",
                "do it".to_string(),
                None,
                SubagentBlockingConfig::default(),
                Some(cancel_rx),
            )
            .await
            .expect("cancelled execution should return JobResult");

        assert_eq!(result.info.status, JobStatus::Cancelled);
        assert!(result.error.as_deref().unwrap_or("").contains("cancelled"));
    }

    #[tokio::test]
    async fn spawn_subagent_blocking_factory_failure_returns_background_error() {
        let manager = make_subagent_manager_with_factory(
            Box::new(move |_agent, _workspace| {
                Box::pin(async move { Err("factory failed".to_string()) })
            }),
            None,
        );

        let err = manager
            .spawn_subagent_blocking(
                "session-1",
                "do it".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect_err("factory failure should return BackgroundError");

        assert!(matches!(err, BackgroundError::SpawnFailed(_)));
    }

    #[tokio::test]
    async fn spawn_subagent_blocking_execution_failure_returns_failed_job_result() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::StreamFailure(
                "agent-stream-broke".to_string(),
            )),
            None,
        );

        let result = manager
            .spawn_subagent_blocking(
                "session-1",
                "do it".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect("execution failure should still return JobResult");

        assert_eq!(result.info.status, JobStatus::Failed);
        assert!(result
            .error
            .as_deref()
            .unwrap_or("")
            .contains("agent-stream-broke"));
    }

    #[tokio::test]
    async fn spawn_subagent_blocking_truncates_output_to_configured_max_bytes() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::ImmediateSuccess("x".repeat(512))),
            None,
        );

        let result = manager
            .spawn_subagent_blocking(
                "session-1",
                "do it".to_string(),
                None,
                SubagentBlockingConfig {
                    timeout: Duration::from_secs(1),
                    result_max_bytes: 32,
                },
                None,
            )
            .await
            .expect("subagent should complete");

        let output = result.output.unwrap_or_default();
        assert!(output.len() <= 32, "output length was {}", output.len());
    }

    #[tokio::test]
    async fn spawn_subagent_blocking_disables_nested_delegation_before_factory() {
        let observed = Arc::new(StdMutex::new(None));
        let observed_for_factory = observed.clone();
        let manager = make_subagent_manager_with_factory_and_identity(
            Box::new(move |agent, _workspace| {
                let mut lock = observed_for_factory
                    .lock()
                    .expect("observation mutex should be available");
                *lock = Some(agent.delegation_config.clone());
                Box::pin(async move {
                    Ok(Box::new(MockSubagentHandle::new(
                        MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
                    )) as Box<dyn AgentHandle + Send + Sync>)
                })
            }),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 2,
                allowed_targets: Some(vec!["worker-agent".to_string()]),
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
            Some("parent-agent"),
            Some("worker-agent"),
        );

        let _ = manager
            .spawn_subagent_blocking(
                "session-1",
                "do it".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect("blocking run should succeed");

        let observed = observed
            .lock()
            .expect("observation mutex should be available")
            .clone();
        assert_eq!(observed, Some(None));
    }

    #[tokio::test]
    async fn delegation_happy_path_returns_result_to_parent() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::ImmediateSuccess(
                "delegation-result".to_string(),
            )),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
        );

        let result = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                Some("delegation-context".to_string()),
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect("delegation should succeed");

        assert_eq!(result.info.status, JobStatus::Completed);
        assert_eq!(result.output.as_deref(), Some("delegation-result"));
    }

    #[tokio::test]
    async fn delegation_rejected_when_disabled() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
            Some(DelegationConfig {
                enabled: false,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
        );

        let err = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect_err("disabled delegation should be rejected");

        assert!(matches!(err, BackgroundError::SpawnFailed(_)));
        assert!(err.to_string().contains("Delegation is disabled"));
    }

    #[tokio::test]
    async fn delegation_rejected_when_target_not_allowed() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: Some(vec!["allowed-agent".to_string()]),
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
        );

        let err = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect_err("unauthorized delegation target should be rejected");

        assert!(matches!(err, BackgroundError::SpawnFailed(_)));
        assert!(err.to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_delegation_with_target_uses_different_session_agent() {
        let observed = Arc::new(StdMutex::new(None));
        let observed_for_factory = observed.clone();
        let manager = make_subagent_manager_with_factory_and_identity(
            Box::new(move |agent, _workspace| {
                let mut lock = observed_for_factory
                    .lock()
                    .expect("observation mutex should be available");
                *lock = Some(agent.clone());
                Box::pin(async move {
                    Ok(Box::new(MockSubagentHandle::new(
                        MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
                    )) as Box<dyn AgentHandle + Send + Sync>)
                })
            }),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: Some(vec!["cursor".to_string()]),
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
            Some("parent-agent"),
            Some("worker-agent"),
        );
        let mut agent_profiles = HashMap::new();
        agent_profiles.insert(
            "cursor".to_string(),
            test_agent_profile("cursor-acp", &["acp"]),
        );
        manager.register_subagent_context(
            "session-1",
            SubagentContext {
                agent: test_session_agent(Some(DelegationConfig {
                    enabled: true,
                    max_depth: 1,
                    allowed_targets: Some(vec!["cursor".to_string()]),
                    result_max_bytes: 51200,
                    max_concurrent_delegations: 3,
                })),
                available_agents: agent_profiles,
                workspace: std::env::temp_dir(),
                parent_session_id: Some("session-1".to_string()),
                parent_session_dir: None,
                delegator_agent_name: Some("parent-agent".to_string()),
                target_agent_name: Some("worker-agent".to_string()),
                delegation_depth: 0,
            },
        );

        let _ = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                Some("Target agent: cursor".to_string()),
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect("delegation with explicit target should succeed");

        let observed = observed
            .lock()
            .expect("observation mutex should be available")
            .clone()
            .expect("factory should have observed target agent config");
        assert_eq!(observed.agent_name.as_deref(), Some("cursor"));
        assert_eq!(observed.model, "cursor");
    }

    #[tokio::test]
    async fn test_delegation_with_target_validates_allowed() {
        let manager = make_subagent_manager_with_factory_and_identity(
            behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: Some(vec!["allowed-agent".to_string()]),
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
            Some("parent-agent"),
            None,
        );
        let mut agent_profiles = HashMap::new();
        agent_profiles.insert(
            "cursor".to_string(),
            test_agent_profile("cursor-acp", &["acp"]),
        );
        manager.register_subagent_context(
            "session-1",
            SubagentContext {
                agent: test_session_agent(Some(DelegationConfig {
                    enabled: true,
                    max_depth: 1,
                    allowed_targets: Some(vec!["allowed-agent".to_string()]),
                    result_max_bytes: 51200,
                    max_concurrent_delegations: 3,
                })),
                available_agents: agent_profiles,
                workspace: std::env::temp_dir(),
                parent_session_id: Some("session-1".to_string()),
                parent_session_dir: None,
                delegator_agent_name: Some("parent-agent".to_string()),
                target_agent_name: None,
                delegation_depth: 0,
            },
        );

        let err = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                Some("Target agent: cursor".to_string()),
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect_err("unauthorized explicit target should be rejected");

        assert!(matches!(err, BackgroundError::SpawnFailed(_)));
        assert!(err.to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_delegation_with_unknown_target_returns_available_agents() {
        let manager = make_subagent_manager_with_factory_and_identity(
            behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: Some(vec!["ghost".to_string()]),
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
            Some("parent-agent"),
            None,
        );

        let err = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                Some("Target agent: ghost".to_string()),
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect_err("unknown explicit target should fail with available list");

        let msg = err.to_string();
        assert!(msg.contains("Delegation target 'ghost' not found"));
        assert!(msg.contains("Available agents:"));
    }

    #[tokio::test]
    async fn test_delegation_without_target_uses_parent_agent() {
        let observed = Arc::new(StdMutex::new(None));
        let observed_for_factory = observed.clone();
        let manager = make_subagent_manager_with_factory_and_identity(
            Box::new(move |agent, _workspace| {
                let mut lock = observed_for_factory
                    .lock()
                    .expect("observation mutex should be available");
                *lock = Some(agent.clone());
                Box::pin(async move {
                    Ok(Box::new(MockSubagentHandle::new(
                        MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
                    )) as Box<dyn AgentHandle + Send + Sync>)
                })
            }),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
            Some("parent-agent"),
            None,
        );

        let _ = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                Some("Delegation ID: deleg-1\nDescription: no explicit target".to_string()),
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect("delegation without explicit target should succeed");

        let observed = observed
            .lock()
            .expect("observation mutex should be available")
            .clone()
            .expect("factory should have observed parent agent config");
        assert_eq!(observed.agent_name.as_deref(), Some("test-agent"));
        assert_eq!(observed.model, "test-agent");
    }

    #[tokio::test]
    async fn delegation_timeout_returns_failed_job_result() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::Pending),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
        );

        let result = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                None,
                SubagentBlockingConfig {
                    timeout: Duration::from_millis(30),
                    result_max_bytes: 51200,
                },
                None,
            )
            .await
            .expect("timeout should return a failed JobResult");

        assert_eq!(result.info.status, JobStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("timed out"));
    }

    #[tokio::test]
    async fn delegation_unavailable_agent_returns_error() {
        let manager = make_subagent_manager_with_factory(
            Box::new(move |_agent, _workspace| {
                Box::pin(async move { Err("command not found: mock-subagent".to_string()) })
            }),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
        );

        let err = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect_err("unavailable target agent should return error");

        assert!(matches!(err, BackgroundError::SpawnFailed(_)));
        assert!(err.to_string().contains("command not found"));
    }

    #[tokio::test]
    async fn delegation_self_delegation_guard_rejects_same_agent() {
        let manager = make_subagent_manager_with_factory_and_identity(
            behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: Some(vec!["parent-agent".to_string()]),
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
            Some("parent-agent"),
            Some("parent-agent"),
        );

        let err = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect_err("self delegation must be rejected");

        assert!(matches!(err, BackgroundError::SpawnFailed(_)));
        assert!(err.to_string().contains("self-delegation"));
    }

    #[tokio::test]
    async fn delegation_result_truncation_respects_config_limit() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::ImmediateSuccess("y".repeat(200))),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 16,
                max_concurrent_delegations: 3,
            }),
        );

        let result = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                None,
                SubagentBlockingConfig {
                    timeout: Duration::from_secs(1),
                    result_max_bytes: 16,
                },
                None,
            )
            .await
            .expect("delegation should complete");

        let output = result.output.unwrap_or_default();
        assert!(output.len() <= 16, "output length was {}", output.len());
    }

    #[tokio::test]
    async fn delegation_blocking_emits_spawned_and_completed_events() {
        let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
            behavior_factory(MockSubagentBehavior::ImmediateSuccess(
                "eventful-result".to_string(),
            )),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            }),
        );

        let result = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect("blocking delegation should succeed");

        assert_eq!(result.info.status, JobStatus::Completed);

        let mut saw_spawned = false;
        let mut saw_completed = false;
        for _ in 0..5 {
            let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
                .await
                .expect("timeout waiting for delegation event")
                .expect("failed to receive delegation event");

            if event.event == events::SUBAGENT_SPAWNED {
                saw_spawned = true;
            }
            if event.event == events::SUBAGENT_COMPLETED {
                saw_completed = true;
            }

            if saw_spawned && saw_completed {
                break;
            }
        }

        assert!(saw_spawned, "expected {} event", events::SUBAGENT_SPAWNED);
        assert!(
            saw_completed,
            "expected {} event",
            events::SUBAGENT_COMPLETED
        );
    }

    #[tokio::test]
    async fn delegation_rejected_when_max_concurrent_delegations_reached() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::Pending),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 1,
            }),
        );

        let first_job_id = manager
            .spawn_subagent("session-1", "delegate first".to_string(), None)
            .await
            .expect("first delegation should spawn");

        tokio::time::sleep(Duration::from_millis(25)).await;

        let err = manager
            .spawn_subagent("session-1", "delegate second".to_string(), None)
            .await
            .expect_err("second delegation should be rejected at concurrency limit");

        assert!(err
            .to_string()
            .contains("Maximum concurrent delegations (1) exceeded"));

        manager.cancel_job(&first_job_id).await;
    }

    #[tokio::test]
    async fn delegation_under_max_concurrent_delegations_is_allowed() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::Pending),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 2,
            }),
        );

        let first_job_id = manager
            .spawn_subagent("session-1", "delegate first".to_string(), None)
            .await
            .expect("first delegation should spawn");
        let second_job_id = manager
            .spawn_subagent("session-1", "delegate second".to_string(), None)
            .await
            .expect("second delegation should still be allowed under limit");

        manager.cancel_job(&first_job_id).await;
        manager.cancel_job(&second_job_id).await;
    }

    #[tokio::test]
    async fn completed_delegation_frees_concurrency_slot() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::DelayedSuccess {
                output: "done".to_string(),
                delay: Duration::from_millis(80),
            }),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 1,
            }),
        );

        let _first = manager
            .spawn_subagent("session-1", "delegate first".to_string(), None)
            .await
            .expect("first delegation should spawn");

        tokio::time::sleep(Duration::from_millis(10)).await;

        let blocked = manager
            .spawn_subagent("session-1", "delegate blocked".to_string(), None)
            .await;
        assert!(
            blocked.is_err(),
            "second delegation should be blocked while first is running"
        );

        tokio::time::sleep(Duration::from_millis(120)).await;

        let second = manager
            .spawn_subagent("session-1", "delegate second".to_string(), None)
            .await
            .expect("delegation slot should be freed after completion");

        manager.cancel_job(&second).await;
    }

    #[tokio::test]
    async fn failed_delegation_frees_concurrency_slot() {
        let manager = make_subagent_manager_with_factory(
            behavior_factory(MockSubagentBehavior::DelayedFailure {
                error: "boom".to_string(),
                delay: Duration::from_millis(80),
            }),
            Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 1,
            }),
        );

        let _first = manager
            .spawn_subagent("session-1", "delegate first".to_string(), None)
            .await
            .expect("first delegation should spawn");

        tokio::time::sleep(Duration::from_millis(10)).await;

        let blocked = manager
            .spawn_subagent("session-1", "delegate blocked".to_string(), None)
            .await;
        assert!(
            blocked.is_err(),
            "second delegation should be blocked while first is running"
        );

        tokio::time::sleep(Duration::from_millis(120)).await;

        let second = manager
            .spawn_subagent("session-1", "delegate second".to_string(), None)
            .await
            .expect("delegation slot should be freed after failure");

        manager.cancel_job(&second).await;
    }

    #[tokio::test]
    async fn delegation_writes_parent_session_id_and_incremented_depth_to_child_session() {
        let parent_dir = tempfile::TempDir::new().expect("temp dir should be created");
        let (tx, _) = broadcast::channel(16);
        let manager = BackgroundJobManager::new(tx).with_subagent_factory(behavior_factory(
            MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
        ));
        manager.register_subagent_context(
            "session-1",
            SubagentContext {
                agent: test_session_agent(Some(DelegationConfig {
                    enabled: true,
                    max_depth: 1,
                    allowed_targets: None,
                    result_max_bytes: 51200,
                    max_concurrent_delegations: 3,
                })),
                available_agents: HashMap::new(),
                workspace: std::env::temp_dir(),
                parent_session_id: Some("session-1".to_string()),
                parent_session_dir: Some(parent_dir.path().to_path_buf()),
                delegator_agent_name: Some("parent-agent".to_string()),
                target_agent_name: Some("worker-agent".to_string()),
                delegation_depth: 0,
            },
        );

        let result = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate this".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect("delegation should complete");

        let session_path = result
            .info
            .session_path
            .expect("subagent session path should exist");
        let jsonl_path = session_path.join("session.jsonl");
        let contents = tokio::fs::read_to_string(&jsonl_path)
            .await
            .expect("subagent session jsonl should be readable");

        let metadata_line = contents
            .lines()
            .find_map(|line| {
                let event: serde_json::Value = serde_json::from_str(line).ok()?;
                if event.get("type")?.as_str()? != "system" {
                    return None;
                }
                let content = event.get("content")?.as_str()?;
                serde_json::from_str::<serde_json::Value>(content).ok()
            })
            .expect("delegation metadata system event should exist");

        assert_eq!(
            metadata_line["delegation_metadata"]["parent_session_id"]
                .as_str()
                .expect("parent_session_id should be present"),
            "session-1"
        );
        assert_eq!(
            metadata_line["delegation_metadata"]["delegation_depth"]
                .as_u64()
                .expect("delegation_depth should be present"),
            1
        );
    }

    #[tokio::test]
    async fn spawn_bash_returns_job_id_immediately() {
        let manager = create_manager();

        let job_id = manager
            .spawn_bash("session-1", "echo hello".to_string(), None, None)
            .await
            .unwrap();

        assert!(job_id.starts_with("job-"));
    }

    #[tokio::test]
    async fn job_appears_in_list_while_running() {
        let manager = create_manager();

        let job_id = manager
            .spawn_bash("session-1", "sleep 5".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        let jobs = manager.list_jobs("session-1");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, job_id);
        assert_eq!(jobs[0].status, JobStatus::Running);

        manager.cancel_job(&job_id).await;
    }

    #[tokio::test]
    async fn completed_job_moves_to_history() {
        let manager = create_manager();

        let job_id = manager
            .spawn_bash("session-1", "echo done".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let result = manager.get_job_result(&job_id);
        assert!(result.is_some());

        let result = result.unwrap();
        assert!(result.info.status.is_terminal());
    }

    #[tokio::test]
    async fn cancel_job_stops_running_job() {
        let manager = create_manager();

        let job_id = manager
            .spawn_bash("session-1", "sleep 60".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(manager.running.contains_key(&job_id));

        let cancelled = manager.cancel_job(&job_id).await;
        assert!(cancelled);

        assert!(!manager.running.contains_key(&job_id));
    }

    #[tokio::test]
    async fn history_eviction_at_limit() {
        let (tx, _) = broadcast::channel(16);
        let mut manager = BackgroundJobManager::new(tx);
        manager.max_history = 3;

        for i in 0..5 {
            let _ = manager
                .spawn_bash("session-1", format!("echo job-{i}"), None, None)
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        tokio::time::sleep(Duration::from_millis(500)).await;

        let jobs = manager.list_jobs("session-1");
        assert!(
            jobs.len() <= 3,
            "Should have at most 3 jobs, got {}",
            jobs.len()
        );
    }

    #[tokio::test]
    async fn get_job_result_for_running_job() {
        let manager = create_manager();

        let job_id = manager
            .spawn_bash("session-1", "sleep 5".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        let result = manager.get_job_result(&job_id);
        assert!(result.is_some());
        assert_eq!(result.unwrap().info.status, JobStatus::Running);

        manager.cancel_job(&job_id).await;
    }

    #[tokio::test]
    async fn cleanup_session_cancels_all_jobs() {
        let manager = create_manager();

        for i in 0..3 {
            let _ = manager
                .spawn_bash("session-1", format!("sleep {}", 10 + i), None, None)
                .await
                .unwrap();
        }

        let _ = manager
            .spawn_bash("session-2", "sleep 10".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(manager.running_count("session-1"), 3);
        assert_eq!(manager.running_count("session-2"), 1);

        manager.cleanup_session("session-1", true).await;

        assert_eq!(manager.running_count("session-1"), 0);
        assert_eq!(manager.running_count("session-2"), 1);

        manager.cleanup_session("session-2", false).await;
    }

    #[tokio::test]
    async fn job_timeout() {
        let manager = create_manager();

        let job_id = manager
            .spawn_bash(
                "session-1",
                "sleep 10".to_string(),
                None,
                Some(Duration::from_millis(100)),
            )
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(300)).await;

        let result = manager.get_job_result(&job_id);
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(result.info.status, JobStatus::Failed);
        assert!(result
            .error
            .as_ref()
            .is_some_and(|e| e.contains("timed out")));
    }

    #[tokio::test]
    async fn different_sessions_have_separate_histories() {
        let manager = create_manager();

        let _ = manager
            .spawn_bash("session-1", "echo one".to_string(), None, None)
            .await
            .unwrap();
        let _ = manager
            .spawn_bash("session-2", "echo two".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let jobs_1 = manager.list_jobs("session-1");
        let jobs_2 = manager.list_jobs("session-2");

        assert_eq!(jobs_1.len(), 1);
        assert_eq!(jobs_2.len(), 1);
        assert_ne!(jobs_1[0].id, jobs_2[0].id);
    }

    #[tokio::test]
    async fn completed_job_preserves_started_at_for_duration() {
        let manager = create_manager();

        let job_id = manager
            .spawn_bash("session-1", "sleep 0.1".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let result = manager.get_job_result(&job_id).unwrap();
        let duration = result
            .info
            .duration()
            .expect("completed job should have duration");
        let millis = duration.num_milliseconds();

        assert!(
            millis >= 100,
            "Duration {}ms should be >= 100ms (job ran sleep 0.1)",
            millis
        );
        assert!(
            millis < 5000,
            "Duration {}ms should be < 5000ms (sanity check)",
            millis
        );
    }

    #[tokio::test]
    async fn failed_bash_command_has_error_output() {
        let manager = create_manager();

        let job_id = manager
            .spawn_bash("session-1", "false".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let result = manager.get_job_result(&job_id);
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(result.info.status, JobStatus::Failed);
        assert!(result.error.is_some());
        let error = result.error.unwrap();
        assert!(error.contains("Exit code") || error.contains("1"));
    }

    #[tokio::test]
    async fn bash_with_workdir_executes_in_directory() {
        let manager = create_manager();

        let job_id = manager
            .spawn_bash(
                "session-1",
                "pwd".to_string(),
                Some(PathBuf::from("/tmp")),
                None,
            )
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let result = manager.get_job_result(&job_id);
        assert!(result.is_some());

        let result = result.unwrap();
        assert!(result.info.status.is_terminal());
        let output = result.output.unwrap_or_default();
        assert!(output.contains("/tmp"));
    }

    #[tokio::test]
    async fn cancel_job_for_wrong_session_is_denied() {
        let manager = create_manager();

        let job_id = manager
            .spawn_bash("session-1", "sleep 60".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        let cancelled = manager
            .cancel_job_for_session(&job_id, Some("session-2"))
            .await;
        assert!(!cancelled);

        assert!(manager.running.contains_key(&job_id));

        manager.cancel_job(&job_id).await;
    }

    #[tokio::test]
    async fn cancel_nonexistent_job_returns_false() {
        let manager = create_manager();

        let fake_job_id = JobId::from("job-nonexistent");
        let cancelled = manager.cancel_job(&fake_job_id).await;

        assert!(!cancelled);
    }

    #[tokio::test]
    async fn bash_events_are_broadcast() {
        let (tx, mut rx) = broadcast::channel(16);
        let manager = BackgroundJobManager::new(tx);

        let _job_id = manager
            .spawn_bash("session-1", "echo test".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("failed to receive event");

        assert_eq!(event.session_id, "session-1");
        assert_eq!(event.event, events::BASH_SPAWNED);

        tokio::time::sleep(Duration::from_millis(200)).await;

        let completion_event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout waiting for completion event")
            .expect("failed to receive completion event");

        assert_eq!(completion_event.session_id, "session-1");
        assert!(
            completion_event.event == events::BASH_COMPLETED
                || completion_event.event == events::BASH_FAILED
        );
    }

    #[tokio::test]
    async fn total_running_count_across_sessions() {
        let manager = create_manager();

        let job_id_1 = manager
            .spawn_bash("session-1", "sleep 10".to_string(), None, None)
            .await
            .unwrap();

        let job_id_2 = manager
            .spawn_bash("session-2", "sleep 10".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(manager.total_running_count(), 2);

        manager.cancel_job(&job_id_1).await;

        assert_eq!(manager.total_running_count(), 1);

        manager.cancel_job(&job_id_2).await;

        assert_eq!(manager.total_running_count(), 0);
    }

    #[tokio::test]
    async fn cleanup_session_with_clear_history_removes_history() {
        let manager = create_manager();

        let _job_id = manager
            .spawn_bash("session-1", "echo done".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let jobs_before = manager.list_jobs("session-1");
        assert_eq!(jobs_before.len(), 1);

        manager.cleanup_session("session-1", true).await;

        let jobs_after = manager.list_jobs("session-1");
        assert_eq!(jobs_after.len(), 0);
    }

    #[tokio::test]
    async fn cleanup_session_preserves_history_when_clear_history_false() {
        let manager = create_manager();

        let _job_id = manager
            .spawn_bash("session-1", "echo done".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let jobs_before = manager.list_jobs("session-1");
        assert_eq!(jobs_before.len(), 1);

        manager.cleanup_session("session-1", false).await;

        let jobs_after = manager.list_jobs("session-1");
        assert_eq!(jobs_after.len(), 1);
    }

    #[tokio::test]
    async fn background_spawner_trait_spawn_bash() {
        let manager = create_manager();
        let spawner: &dyn BackgroundSpawner = &manager;

        let job_id = spawner
            .spawn_bash("session-1", "echo trait".to_string(), None, None)
            .await
            .unwrap();

        assert!(job_id.starts_with("job-"));

        tokio::time::sleep(Duration::from_millis(200)).await;

        let result = spawner.get_job_result(&job_id);
        assert!(result.is_some());
        assert!(result.unwrap().info.status.is_terminal());
    }

    #[test]
    fn subagent_context_default_delegation_depth_is_zero() {
        let ctx = SubagentContext {
            agent: test_session_agent(None),
            available_agents: HashMap::new(),
            workspace: std::env::temp_dir(),
            parent_session_id: Some("session-1".to_string()),
            parent_session_dir: None,
            delegator_agent_name: None,
            target_agent_name: None,
            delegation_depth: 0,
        };

        assert_eq!(ctx.delegation_depth, 0);
    }

    #[test]
    fn enforce_delegation_capabilities_rejects_depth_at_hard_cap() {
        let manager = create_manager();
        let agent = test_session_agent(Some(DelegationConfig {
            enabled: true,
            max_depth: 10,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }));

        let result = manager.enforce_delegation_capabilities(
            &agent,
            Some("parent"),
            Some("child"),
            3,
            "session-1",
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Delegation depth limit exceeded"));
    }

    #[test]
    fn enforce_delegation_capabilities_rejects_depth_above_hard_cap() {
        let manager = create_manager();
        let agent = test_session_agent(Some(DelegationConfig {
            enabled: true,
            max_depth: 10,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }));

        let result = manager.enforce_delegation_capabilities(
            &agent,
            Some("parent"),
            Some("child"),
            5,
            "session-1",
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Delegation depth limit exceeded"));
    }

    #[test]
    fn enforce_delegation_capabilities_allows_depth_below_hard_cap() {
        let manager = create_manager();
        let agent = test_session_agent(Some(DelegationConfig {
            enabled: true,
            max_depth: 10,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }));

        let result = manager.enforce_delegation_capabilities(
            &agent,
            Some("parent"),
            Some("child"),
            0,
            "session-1",
        );

        assert!(result.is_ok());
    }

    #[test]
    fn enforce_delegation_capabilities_allows_depth_one() {
        let manager = create_manager();
        let agent = test_session_agent(Some(DelegationConfig {
            enabled: true,
            max_depth: 10,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }));

        let result = manager.enforce_delegation_capabilities(
            &agent,
            Some("parent"),
            Some("child"),
            1,
            "session-1",
        );

        assert!(result.is_ok());
    }

    #[test]
    fn enforce_delegation_capabilities_allows_depth_two() {
        let manager = create_manager();
        let agent = test_session_agent(Some(DelegationConfig {
            enabled: true,
            max_depth: 10,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }));

        let result = manager.enforce_delegation_capabilities(
            &agent,
            Some("parent"),
            Some("child"),
            2,
            "session-1",
        );

        assert!(result.is_ok());
    }

    #[test]
    fn enforce_delegation_capabilities_hard_cap_checked_before_enabled_check() {
        let manager = create_manager();
        let agent = test_session_agent(Some(DelegationConfig {
            enabled: false,
            max_depth: 10,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }));

        let result = manager.enforce_delegation_capabilities(
            &agent,
            Some("parent"),
            Some("child"),
            3,
            "session-1",
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Delegation depth limit exceeded"));
    }

    #[tokio::test]
    async fn delegation_spawned_event_emitted_on_parent_channel() {
        let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
            behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
            None,
        );

        let _ = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate task".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect("delegation should succeed");

        let mut saw_delegation_spawned = false;
        for _ in 0..10 {
            match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
                Ok(Ok(event)) => {
                    if event.event == events::DELEGATION_SPAWNED {
                        saw_delegation_spawned = true;
                        assert_eq!(event.session_id, "session-1");
                        assert!(event.data["delegation_id"].as_str().is_some());
                        assert_eq!(event.data["parent_session_id"].as_str(), Some("session-1"));
                        assert!(event.data["prompt"]
                            .as_str()
                            .unwrap_or("")
                            .contains("delegate"));
                        break;
                    }
                }
                _ => break,
            }
        }
        assert!(
            saw_delegation_spawned,
            "expected delegation_spawned event on parent channel"
        );
    }

    #[tokio::test]
    async fn delegation_completed_event_emitted_on_parent_channel() {
        let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
            behavior_factory(MockSubagentBehavior::ImmediateSuccess(
                "result-data".to_string(),
            )),
            None,
        );

        let _ = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate task".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect("delegation should succeed");

        let mut saw_delegation_completed = false;
        for _ in 0..10 {
            match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
                Ok(Ok(event)) => {
                    if event.event == events::DELEGATION_COMPLETED {
                        saw_delegation_completed = true;
                        assert_eq!(event.session_id, "session-1");
                        assert!(event.data["delegation_id"].as_str().is_some());
                        assert_eq!(event.data["parent_session_id"].as_str(), Some("session-1"));
                        assert!(event.data["result_summary"]
                            .as_str()
                            .unwrap_or("")
                            .contains("result-data"));
                        break;
                    }
                }
                _ => break,
            }
        }
        assert!(
            saw_delegation_completed,
            "expected delegation_completed event on parent channel"
        );
    }

    #[tokio::test]
    async fn delegation_failed_event_emitted_on_parent_channel() {
        let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
            behavior_factory(MockSubagentBehavior::StreamFailure(
                "agent-crashed".to_string(),
            )),
            None,
        );

        let _ = manager
            .spawn_subagent_blocking(
                "session-1",
                "delegate task".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect("failed delegation still returns a JobResult");

        let mut saw_delegation_failed = false;
        for _ in 0..10 {
            match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
                Ok(Ok(event)) => {
                    if event.event == events::DELEGATION_FAILED {
                        saw_delegation_failed = true;
                        assert_eq!(event.session_id, "session-1");
                        assert!(event.data["delegation_id"].as_str().is_some());
                        assert_eq!(event.data["parent_session_id"].as_str(), Some("session-1"));
                        assert!(event.data["error"]
                            .as_str()
                            .unwrap_or("")
                            .contains("agent-crashed"));
                        break;
                    }
                }
                _ => break,
            }
        }
        assert!(
            saw_delegation_failed,
            "expected delegation_failed event on parent channel"
        );
    }

    #[tokio::test]
    async fn non_delegation_subagent_does_not_emit_delegation_events() {
        let (tx, mut rx) = broadcast::channel(32);
        let manager = BackgroundJobManager::new(tx).with_subagent_factory(behavior_factory(
            MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
        ));
        manager.register_subagent_context(
            "session-1",
            SubagentContext {
                agent: test_session_agent(Some(default_enabled_delegation_config())),
                available_agents: HashMap::new(),
                workspace: std::env::temp_dir(),
                parent_session_id: None,
                parent_session_dir: None,
                delegator_agent_name: Some("parent".to_string()),
                target_agent_name: Some("child".to_string()),
                delegation_depth: 0,
            },
        );

        let _ = manager
            .spawn_subagent_blocking(
                "session-1",
                "do task".to_string(),
                None,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .expect("subagent should succeed");

        let mut delegation_events = vec![];
        loop {
            match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
                Ok(Ok(event)) => {
                    if event.event == events::DELEGATION_SPAWNED
                        || event.event == events::DELEGATION_COMPLETED
                        || event.event == events::DELEGATION_FAILED
                    {
                        delegation_events.push(event.event.clone());
                    }
                }
                _ => break,
            }
        }
        assert!(
            delegation_events.is_empty(),
            "non-delegation subagent should not emit delegation events, got: {:?}",
            delegation_events
        );
    }
}
