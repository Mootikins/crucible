//! Background task management for the daemon.
//!
//! Provides session-scoped, ephemeral task management (tasks don't survive daemon restart).
//!
//! # Supported Task Types
//!
//! - **Bash**: Background shell command execution with timeout
//! - **Subagent**: Multi-turn LLM execution with inherited tools
//!
//! # Example
//!
//! ```ignore
//! let manager = BackgroundTaskManager::new(event_tx);
//!
//! let task_id = manager.spawn_bash(
//!     "session-123",
//!     "cargo build --release",
//!     None,
//!     None,
//! ).await?;
//!
//! let tasks = manager.list_tasks("session-123");
//!
//! if let Some(result) = manager.get_task_result(&task_id) {
//!     println!("Task completed: {:?}", result);
//! }
//! ```

use crate::protocol::SessionEventMessage;
use async_trait::async_trait;
use crucible_core::background::{
    truncate, BackgroundSpawner, TaskError, TaskId, TaskInfo, TaskKind, TaskResult,
};
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::AgentHandle;
use crucible_observe::events::LogEvent;
use crucible_observe::session::SessionWriter;
use dashmap::DashMap;
use futures::StreamExt;
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
    pub const BASH_SPAWNED: &str = "bash_task_spawned";
    pub const BASH_COMPLETED: &str = "bash_task_completed";
    pub const BASH_FAILED: &str = "bash_task_failed";
    pub const SUBAGENT_SPAWNED: &str = "subagent_spawned";
    pub const SUBAGENT_COMPLETED: &str = "subagent_completed";
    pub const SUBAGENT_FAILED: &str = "subagent_failed";
    pub const BACKGROUND_COMPLETED: &str = "background_task_completed";
}

#[derive(Error, Debug)]
pub enum BackgroundError {
    #[error("Task error: {0}")]
    Task(#[from] TaskError),

    #[error("Failed to spawn task: {0}")]
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

struct RunningTask {
    info: TaskInfo,
    cancel_tx: oneshot::Sender<()>,
    #[allow(dead_code)]
    task_handle: JoinHandle<()>,
}

pub struct SubagentContext {
    pub agent: SessionAgent,
    pub workspace: PathBuf,
    /// Parent session directory for creating subagent session files
    pub parent_session_dir: Option<PathBuf>,
}

pub struct BackgroundTaskManager {
    running: Arc<DashMap<TaskId, RunningTask>>,
    history: Arc<DashMap<String, std::collections::VecDeque<TaskResult>>>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    max_history: usize,
    subagent_factory: Option<Arc<SubagentFactory>>,
    subagent_contexts: Arc<DashMap<String, SubagentContext>>,
}

impl BackgroundTaskManager {
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

    pub fn with_subagent_factory(mut self, factory: SubagentFactory) -> Self {
        self.subagent_factory = Some(Arc::new(factory));
        self
    }

    pub fn register_subagent_context(&self, session_id: &str, config: SubagentContext) {
        self.subagent_contexts.insert(session_id.to_string(), config);
    }

    pub fn unregister_subagent_context(&self, session_id: &str) {
        self.subagent_contexts.remove(session_id);
    }

    pub async fn spawn_bash(
        &self,
        session_id: &str,
        command: String,
        workdir: Option<PathBuf>,
        timeout: Option<Duration>,
    ) -> Result<TaskId, BackgroundError> {
        let kind = TaskKind::Bash {
            command: command.clone(),
            workdir: workdir.clone(),
        };
        let info = TaskInfo::new(session_id.to_string(), kind);
        let task_id = info.id.clone();
        let timeout = timeout.unwrap_or(DEFAULT_BASH_TIMEOUT);
        let (cancel_tx, cancel_rx) = oneshot::channel();

        let _ = self.event_tx.send(SessionEventMessage::new(
            session_id,
            events::BASH_SPAWNED,
            serde_json::json!({
                "task_id": task_id,
                "command": command,
            }),
        ));

        info!(
            task_id = %task_id,
            session_id = %session_id,
            command = %command,
            "Spawning background bash task"
        );

        let task_handle = {
            let running = self.running.clone();
            let history = self.history.clone();
            let event_tx = self.event_tx.clone();
            let task_id = task_id.clone();
            let session_id = session_id.to_string();
            let max_history = self.max_history;
            let command = command.clone();

            tokio::spawn(async move {
                let result =
                    Self::execute_bash_with_cancellation(command.clone(), workdir, timeout, cancel_rx)
                        .await;

                // Extract original TaskInfo to preserve started_at timestamp
                let info = running
                    .remove(&task_id)
                    .map(|(_, rt)| rt.info)
                    .unwrap_or_else(|| {
                        // Fallback: task was already removed (shouldn't happen)
                        TaskInfo::new(
                            session_id.clone(),
                            TaskKind::Bash {
                                command: command.clone(),
                                workdir: None,
                            },
                        )
                    });

                let task_result = Self::build_task_result(info, result);
                Self::emit_completion_events(&event_tx, &session_id, &task_result.info.id.clone(), &task_result);
                Self::add_to_history(&history, &session_id, task_result, max_history);

                debug!(task_id = %task_id, "Background bash task completed");
            })
        };

        self.running.insert(
            task_id.clone(),
            RunningTask {
                info,
                cancel_tx,
                task_handle,
            },
        );

        Ok(task_id)
    }

    fn build_task_result(mut info: TaskInfo, result: Result<(String, i32), BashError>) -> TaskResult {
        match result {
            Ok((output, exit_code)) => {
                info.mark_completed();
                TaskResult::success_with_exit_code(info, output, exit_code)
            }
            Err(BashError::Cancelled) => {
                info.mark_cancelled();
                TaskResult::failure(info, "Task cancelled".to_string())
            }
            Err(BashError::Timeout) => {
                info.mark_failed();
                TaskResult::failure(info, "Task timed out".to_string())
            }
            Err(BashError::Failed { message, exit_code }) => {
                info.mark_failed();
                match exit_code {
                    Some(code) => TaskResult::failure_with_exit_code(info, message, code),
                    None => TaskResult::failure(info, message),
                }
            }
        }
    }

    fn emit_completion_events(
        event_tx: &broadcast::Sender<SessionEventMessage>,
        session_id: &str,
        task_id: &TaskId,
        result: &TaskResult,
    ) {
        let (event_type, event_data) = if result.is_success() {
            let output = result.output.as_deref().unwrap_or("");
            (
                events::BASH_COMPLETED,
                serde_json::json!({
                    "task_id": task_id,
                    "output": truncate(output, 1000),
                    "exit_code": result.exit_code,
                }),
            )
        } else {
            let error = result.error.as_deref().unwrap_or("Unknown error");
            (
                events::BASH_FAILED,
                serde_json::json!({
                    "task_id": task_id,
                    "error": error,
                    "exit_code": result.exit_code,
                }),
            )
        };

        if event_tx
            .send(SessionEventMessage::new(session_id, event_type, event_data))
            .is_err()
        {
            warn!(task_id = %task_id, "No subscribers for bash completion event");
        }
        Self::emit_background_completed(event_tx, session_id, task_id, result, "bash");
    }

    fn emit_background_completed(
        event_tx: &broadcast::Sender<SessionEventMessage>,
        session_id: &str,
        task_id: &TaskId,
        result: &TaskResult,
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

        if event_tx
            .send(SessionEventMessage::new(
                session_id,
                events::BACKGROUND_COMPLETED,
                serde_json::json!({
                    "task_id": task_id,
                    "kind": kind,
                    "summary": summary,
                }),
            ))
            .is_err()
        {
            warn!(task_id = %task_id, kind = %kind, "No subscribers for background completion event");
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
        history: &DashMap<String, std::collections::VecDeque<TaskResult>>,
        session_id: &str,
        result: TaskResult,
        max_history: usize,
    ) {
        let mut entry = history.entry(session_id.to_string()).or_default();
        entry.push_back(result);

        while entry.len() > max_history {
            entry.pop_front();
        }
    }

    pub fn list_tasks(&self, session_id: &str) -> Vec<TaskInfo> {
        let mut tasks = Vec::new();

        // Add running tasks
        for entry in self.running.iter() {
            if entry.value().info.session_id == session_id {
                tasks.push(entry.value().info.clone());
            }
        }

        // Add completed tasks from history
        if let Some(history) = self.history.get(session_id) {
            for result in history.iter() {
                tasks.push(result.info.clone());
            }
        }

        // Sort by started_at descending (newest first)
        tasks.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        tasks
    }

    pub fn get_task_result(&self, task_id: &TaskId) -> Option<TaskResult> {
        self.get_task_result_for_session(task_id, None)
    }

    pub fn get_task_result_for_session(
        &self,
        task_id: &TaskId,
        session_id: Option<&str>,
    ) -> Option<TaskResult> {
        if let Some(entry) = self.running.get(task_id) {
            if let Some(sid) = session_id {
                if entry.info.session_id != sid {
                    return None;
                }
            }
            return Some(TaskResult {
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
                if result.info.id == *task_id {
                    return Some(result.clone());
                }
            }
        }

        None
    }

    pub async fn cancel_task(&self, task_id: &TaskId) -> bool {
        self.cancel_task_for_session(task_id, None).await
    }

    pub async fn cancel_task_for_session(
        &self,
        task_id: &TaskId,
        caller_session_id: Option<&str>,
    ) -> bool {
        if let Some(sid) = caller_session_id {
            if let Some(entry) = self.running.get(task_id) {
                if entry.info.session_id != sid {
                    warn!(
                        task_id = %task_id,
                        owner = %entry.info.session_id,
                        caller = %sid,
                        "Session tried to cancel task owned by another session"
                    );
                    return false;
                }
            }
        }

        let Some((_, running_task)) = self.running.remove(task_id) else {
            warn!(task_id = %task_id, "Task not found for cancellation");
            return false;
        };

        let _ = running_task.cancel_tx.send(());

        let mut info = running_task.info;
        info.mark_cancelled();
        let task_session_id = info.session_id.clone();
        let task_result = TaskResult::failure(info, "Task cancelled".to_string());

        let kind = match &task_result.info.kind {
            TaskKind::Bash { .. } => "bash",
            TaskKind::Subagent { .. } => "subagent",
        };
        Self::emit_background_completed(&self.event_tx, &task_session_id, task_id, &task_result, kind);
        Self::add_to_history(&self.history, &task_session_id, task_result, self.max_history);

        info!(task_id = %task_id, "Task cancelled");
        true
    }

    pub async fn cleanup_session(&self, session_id: &str, clear_history: bool) {
        let task_ids: Vec<TaskId> = self
            .running
            .iter()
            .filter(|entry| entry.value().info.session_id == session_id)
            .map(|entry| entry.key().clone())
            .collect();

        for task_id in task_ids {
            self.cancel_task(&task_id).await;
        }

        if clear_history {
            self.history.remove(session_id);
        }

        debug!(session_id = %session_id, "Session cleanup completed");
    }

    pub fn running_count(&self, session_id: &str) -> usize {
        self.running
            .iter()
            .filter(|entry| entry.value().info.session_id == session_id)
            .count()
    }

    pub fn total_running_count(&self) -> usize {
        self.running.len()
    }

    pub async fn spawn_subagent(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
    ) -> Result<TaskId, BackgroundError> {
        let factory = self
            .subagent_factory
            .as_ref()
            .ok_or(BackgroundError::NoSubagentFactory)?;

        let (agent_config, workspace, parent_session_dir) = {
            let ctx = self
                .subagent_contexts
                .get(session_id)
                .ok_or_else(|| BackgroundError::SpawnFailed("Subagent context not registered".into()))?;
            (ctx.agent.clone(), ctx.workspace.clone(), ctx.parent_session_dir.clone())
        };

        let kind = TaskKind::Subagent {
            prompt: prompt.clone(),
            context: context.clone(),
        };
        let mut info = TaskInfo::new(session_id.to_string(), kind);
        let task_id = info.id.clone();
        let (cancel_tx, cancel_rx) = oneshot::channel();

        let (subagent_writer, session_link) = if let Some(ref parent_dir) = parent_session_dir {
            match SessionWriter::create_subagent(parent_dir).await {
                Ok((writer, link)) => {
                    info.session_path = Some(writer.session_dir().to_path_buf());
                    (Some(Arc::new(Mutex::new(writer))), link)
                }
                Err(e) => {
                    warn!(error = %e, "Failed to create subagent session, continuing without persistence");
                    (None, format!("[[subagent:{}]]", task_id))
                }
            }
        } else {
            (None, format!("[[subagent:{}]]", task_id))
        };

        let _ = self.event_tx.send(SessionEventMessage::new(
            session_id,
            events::SUBAGENT_SPAWNED,
            serde_json::json!({
                "task_id": task_id,
                "session_link": session_link,
                "prompt": truncate(&prompt, 100),
            }),
        ));

        info!(
            task_id = %task_id,
            session_id = %session_id,
            session_link = %session_link,
            prompt_len = prompt.len(),
            "Spawning background subagent"
        );

        let agent = factory(&agent_config, &workspace)
            .await
            .map_err(BackgroundError::SpawnFailed)?;

        let task_handle = {
            let running = self.running.clone();
            let history = self.history.clone();
            let event_tx = self.event_tx.clone();
            let task_id = task_id.clone();
            let session_id = session_id.to_string();
            let max_history = self.max_history;
            let session_link = session_link.clone();

            tokio::spawn(async move {
                let result = Self::execute_subagent(
                    agent,
                    prompt.clone(),
                    context,
                    cancel_rx,
                    DEFAULT_SUBAGENT_MAX_TURNS,
                    subagent_writer,
                )
                .await;

                // Extract original TaskInfo to preserve started_at timestamp
                let info = running
                    .remove(&task_id)
                    .map(|(_, rt)| rt.info)
                    .unwrap_or_else(|| {
                        TaskInfo::new(
                            session_id.clone(),
                            TaskKind::Subagent {
                                prompt,
                                context: None,
                            },
                        )
                    });

                let task_result = Self::build_subagent_result(info, result);
                Self::emit_subagent_completion_events(
                    &event_tx,
                    &session_id,
                    &task_result.info.id.clone(),
                    &task_result,
                    &session_link,
                );
                Self::add_to_history(&history, &session_id, task_result, max_history);

                debug!(task_id = %task_id, "Background subagent completed");
            })
        };

        self.running.insert(
            task_id.clone(),
            RunningTask {
                info,
                cancel_tx,
                task_handle,
            },
        );

        Ok(task_id)
    }

    async fn execute_subagent(
        mut agent: Box<dyn AgentHandle + Send + Sync>,
        prompt: String,
        context: Option<String>,
        mut cancel_rx: oneshot::Receiver<()>,
        max_turns: usize,
        session_writer: Option<Arc<Mutex<SessionWriter>>>,
    ) -> Result<String, SubagentError> {
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

            if accumulated_output.len() > MAX_SUBAGENT_OUTPUT {
                accumulated_output.truncate(MAX_SUBAGENT_OUTPUT);
                accumulated_output.push_str("\n\n[Output truncated due to size limit]");
                break;
            }

            if !has_tool_calls {
                break;
            }
        }

        Ok(accumulated_output.trim().to_string())
    }

    fn build_subagent_result(mut info: TaskInfo, result: Result<String, SubagentError>) -> TaskResult {
        match result {
            Ok(output) => {
                info.mark_completed();
                TaskResult::success(info, output)
            }
            Err(SubagentError::Cancelled) => {
                info.mark_cancelled();
                TaskResult::failure(info, "Subagent cancelled".to_string())
            }
            Err(SubagentError::Failed(msg)) => {
                info.mark_failed();
                TaskResult::failure(info, msg)
            }
        }
    }

    fn emit_subagent_completion_events(
        event_tx: &broadcast::Sender<SessionEventMessage>,
        session_id: &str,
        task_id: &TaskId,
        result: &TaskResult,
        session_link: &str,
    ) {
        let (event_type, event_data) = if result.is_success() {
            let output = result.output.as_deref().unwrap_or("");
            (
                events::SUBAGENT_COMPLETED,
                serde_json::json!({
                    "task_id": task_id,
                    "session_link": session_link,
                    "summary": truncate(output, 500),
                }),
            )
        } else {
            let error = result.error.as_deref().unwrap_or("Unknown error");
            (
                events::SUBAGENT_FAILED,
                serde_json::json!({
                    "task_id": task_id,
                    "session_link": session_link,
                    "error": error,
                }),
            )
        };

        if event_tx
            .send(SessionEventMessage::new(session_id, event_type, event_data))
            .is_err()
        {
            warn!(task_id = %task_id, "No subscribers for subagent completion event");
        }
        Self::emit_background_completed(event_tx, session_id, task_id, result, "subagent");
    }
}

enum SubagentError {
    Cancelled,
    Failed(String),
}

#[async_trait]
impl BackgroundSpawner for BackgroundTaskManager {
    async fn spawn_bash(
        &self,
        session_id: &str,
        command: String,
        workdir: Option<PathBuf>,
        timeout: Option<Duration>,
    ) -> Result<TaskId, TaskError> {
        self.spawn_bash(session_id, command, workdir, timeout)
            .await
            .map_err(|e| TaskError::SpawnFailed(e.to_string()))
    }

    fn list_tasks(&self, session_id: &str) -> Vec<TaskInfo> {
        BackgroundTaskManager::list_tasks(self, session_id)
    }

    fn get_task_result(&self, task_id: &TaskId) -> Option<TaskResult> {
        BackgroundTaskManager::get_task_result(self, task_id)
    }

    async fn cancel_task(&self, task_id: &TaskId) -> bool {
        BackgroundTaskManager::cancel_task(self, task_id).await
    }

    async fn spawn_subagent(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
    ) -> Result<TaskId, TaskError> {
        BackgroundTaskManager::spawn_subagent(self, session_id, prompt, context)
            .await
            .map_err(|e| TaskError::SpawnFailed(e.to_string()))
    }
}

enum BashError {
    Cancelled,
    Timeout,
    Failed { message: String, exit_code: Option<i32> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::background::TaskStatus;
    use tokio::sync::broadcast;

    fn create_manager() -> BackgroundTaskManager {
        let (tx, _) = broadcast::channel(16);
        BackgroundTaskManager::new(tx)
    }

    #[tokio::test]
    async fn spawn_bash_returns_task_id_immediately() {
        let manager = create_manager();

        let task_id = manager
            .spawn_bash("session-1", "echo hello".to_string(), None, None)
            .await
            .unwrap();

        assert!(task_id.starts_with("task-"));
    }

    #[tokio::test]
    async fn task_appears_in_list_while_running() {
        let manager = create_manager();

        let task_id = manager
            .spawn_bash("session-1", "sleep 5".to_string(), None, None)
            .await
            .unwrap();

        // Give task time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        let tasks = manager.list_tasks("session-1");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task_id);
        assert_eq!(tasks[0].status, TaskStatus::Running);

        // Cancel to clean up
        manager.cancel_task(&task_id).await;
    }

    #[tokio::test]
    async fn completed_task_moves_to_history() {
        let manager = create_manager();

        let task_id = manager
            .spawn_bash("session-1", "echo done".to_string(), None, None)
            .await
            .unwrap();

        // Wait for completion
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should be in history now
        let result = manager.get_task_result(&task_id);
        assert!(result.is_some());

        let result = result.unwrap();
        assert!(result.info.status.is_terminal());
    }

    #[tokio::test]
    async fn cancel_task_stops_running_task() {
        let manager = create_manager();

        let task_id = manager
            .spawn_bash("session-1", "sleep 60".to_string(), None, None)
            .await
            .unwrap();

        // Give task time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify it's running
        assert!(manager.running.contains_key(&task_id));

        // Cancel
        let cancelled = manager.cancel_task(&task_id).await;
        assert!(cancelled);

        // Should no longer be running
        assert!(!manager.running.contains_key(&task_id));
    }

    #[tokio::test]
    async fn history_eviction_at_limit() {
        let (tx, _) = broadcast::channel(16);
        let mut manager = BackgroundTaskManager::new(tx);
        manager.max_history = 3; // Small limit for testing

        // Spawn 5 quick tasks
        for i in 0..5 {
            let _ = manager
                .spawn_bash("session-1", format!("echo task-{i}"), None, None)
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Wait for all to complete
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Should only have 3 in history (the newest 3)
        let tasks = manager.list_tasks("session-1");
        assert!(tasks.len() <= 3, "Should have at most 3 tasks, got {}", tasks.len());
    }

    #[tokio::test]
    async fn get_task_result_for_running_task() {
        let manager = create_manager();

        let task_id = manager
            .spawn_bash("session-1", "sleep 5".to_string(), None, None)
            .await
            .unwrap();

        // Give task time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        let result = manager.get_task_result(&task_id);
        assert!(result.is_some());
        assert_eq!(result.unwrap().info.status, TaskStatus::Running);

        // Clean up
        manager.cancel_task(&task_id).await;
    }

    #[tokio::test]
    async fn cleanup_session_cancels_all_tasks() {
        let manager = create_manager();

        // Spawn multiple tasks
        for i in 0..3 {
            let _ = manager
                .spawn_bash("session-1", format!("sleep {}", 10 + i), None, None)
                .await
                .unwrap();
        }

        // Also spawn for different session
        let _ = manager
            .spawn_bash("session-2", "sleep 10".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(manager.running_count("session-1"), 3);
        assert_eq!(manager.running_count("session-2"), 1);

        // Cleanup session-1
        manager.cleanup_session("session-1", true).await;

        assert_eq!(manager.running_count("session-1"), 0);
        assert_eq!(manager.running_count("session-2"), 1);

        // Clean up session-2
        manager.cleanup_session("session-2", false).await;
    }

    #[tokio::test]
    async fn task_timeout() {
        let manager = create_manager();

        let task_id = manager
            .spawn_bash(
                "session-1",
                "sleep 10".to_string(),
                None,
                Some(Duration::from_millis(100)), // Very short timeout
            )
            .await
            .unwrap();

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(300)).await;

        let result = manager.get_task_result(&task_id);
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(result.info.status, TaskStatus::Failed);
        assert!(result.error.as_ref().map_or(false, |e| e.contains("timed out")));
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

        let tasks_1 = manager.list_tasks("session-1");
        let tasks_2 = manager.list_tasks("session-2");

        assert_eq!(tasks_1.len(), 1);
        assert_eq!(tasks_2.len(), 1);
        assert_ne!(tasks_1[0].id, tasks_2[0].id);
    }

    #[tokio::test]
    async fn completed_task_preserves_started_at_for_duration() {
        let manager = create_manager();

        let task_id = manager
            .spawn_bash("session-1", "sleep 0.1".to_string(), None, None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let result = manager.get_task_result(&task_id).unwrap();
        let duration = result.info.duration().expect("completed task should have duration");
        let millis = duration.num_milliseconds();

        assert!(
            millis >= 100,
            "Duration {}ms should be >= 100ms (task ran sleep 0.1)",
            millis
        );
        assert!(
            millis < 5000,
            "Duration {}ms should be < 5000ms (sanity check)",
            millis
        );
    }
}
