//! Session logger for persisting agent conversations
//!
//! The SessionLogger writes session data to two locations:
//! - Markdown file in the kiln: `<kiln>/sessions/<workspace>/<timestamp>/log.md`
//! - JSON state file: `<state_path>/sessions/state/<workspace>/<timestamp>.json`
//! - Session index: `<state_path>/sessions/index.json`

use super::format::{
    format_agent_response, format_frontmatter, format_task_list, format_tool_call,
    format_user_message,
};
use super::types::{
    MessageRole, SessionEntry, SessionIndex, SessionMessage, SessionMetadata, SessionState, Task,
};
use chrono::Utc;
use serde_json::Value;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Errors from session logging operations
#[derive(Debug, Error)]
pub enum LoggerError {
    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Session not found
    #[error("Session not found: {0}")]
    NotFound(String),

    /// Session already closed
    #[error("Session already closed")]
    AlreadyClosed,
}

/// Result type for logger operations
pub type LoggerResult<T> = Result<T, LoggerError>;

/// Session logger that persists conversations to disk
///
/// The logger maintains both a human-readable markdown log and a
/// machine-readable JSON state file.
#[derive(Debug)]
pub struct SessionLogger {
    /// Session ID (format: workspace/YYYY-MM-DD_HHMM)
    id: String,

    /// Current session state
    state: SessionState,

    /// Path to markdown log file
    md_path: PathBuf,

    /// Path to JSON state file
    state_path: PathBuf,

    /// Path to session index
    index_path: PathBuf,

    /// Whether the session is closed
    closed: bool,
}

impl SessionLogger {
    /// Create a new session
    ///
    /// Creates the necessary directories and files:
    /// - `<kiln_path>/sessions/<workspace>/<timestamp>/log.md`
    /// - `<state_base>/sessions/state/<workspace>/<timestamp>.json`
    /// - Updates `<state_base>/sessions/index.json`
    ///
    /// # Arguments
    ///
    /// * `workspace` - Workspace name (usually the directory name)
    /// * `kiln_path` - Path to the kiln directory
    /// * `state_base` - Base path for state files (e.g., ~/.crucible)
    pub async fn create(
        workspace: &str,
        kiln_path: &Path,
        state_base: &Path,
    ) -> LoggerResult<Self> {
        let now = Utc::now();
        let timestamp = now.format("%Y-%m-%d_%H%M").to_string();
        let id = format!("{}/{}", workspace, timestamp);

        // Create directory paths
        let session_dir = kiln_path.join("sessions").join(workspace).join(&timestamp);
        let state_dir = state_base.join("sessions").join("state").join(workspace);

        // Create directories
        fs::create_dir_all(&session_dir).await?;
        fs::create_dir_all(&state_dir).await?;
        fs::create_dir_all(state_base.join("sessions")).await?;

        // File paths
        let md_path = session_dir.join("log.md");
        let state_path = state_dir.join(format!("{}.json", timestamp));
        let index_path = state_base.join("sessions").join("index.json");

        // Create initial state
        let metadata = SessionMetadata {
            workspace: workspace.to_string(),
            started: now,
            ended: None,
            continued_from: None,
        };

        let state = SessionState {
            metadata,
            messages: Vec::new(),
            tasks: Vec::new(),
        };

        let logger = Self {
            id: id.clone(),
            state,
            md_path: md_path.clone(),
            state_path,
            index_path: index_path.clone(),
            closed: false,
        };

        // Write initial markdown with frontmatter
        let frontmatter = format_frontmatter(&logger.state.metadata);
        let initial_content = format!("{}# Session Log\n\n", frontmatter);
        fs::write(&md_path, initial_content).await?;

        // Save initial state
        logger.save_state().await?;

        // Update index
        let entry = SessionEntry {
            id,
            workspace: workspace.to_string(),
            md_path: md_path.to_string_lossy().to_string(),
            started: now,
            ended: None,
            continued_as: None,
        };
        logger.update_index(|index| index.add_entry(entry)).await?;

        Ok(logger)
    }

    /// Resume an existing session
    ///
    /// # Arguments
    ///
    /// * `session_id` - Session ID (format: workspace/YYYY-MM-DD_HHMM)
    /// * `state_base` - Base path for state files
    pub async fn resume(session_id: &str, state_base: &Path) -> LoggerResult<Self> {
        // Parse session ID
        let parts: Vec<&str> = session_id.split('/').collect();
        if parts.len() != 2 {
            return Err(LoggerError::NotFound(session_id.to_string()));
        }
        let workspace = parts[0];
        let timestamp = parts[1];

        // Build paths
        let state_path = state_base
            .join("sessions")
            .join("state")
            .join(workspace)
            .join(format!("{}.json", timestamp));
        let index_path = state_base.join("sessions").join("index.json");

        // Load state
        let state_content = fs::read_to_string(&state_path)
            .await
            .map_err(|_| LoggerError::NotFound(session_id.to_string()))?;
        let state: SessionState = serde_json::from_str(&state_content)?;

        // Get md_path from index
        let index_content = fs::read_to_string(&index_path).await.unwrap_or_default();
        let index: SessionIndex = if index_content.is_empty() {
            SessionIndex::default()
        } else {
            serde_json::from_str(&index_content)?
        };

        let entry = index
            .sessions
            .iter()
            .find(|e| e.id == session_id)
            .ok_or_else(|| LoggerError::NotFound(session_id.to_string()))?;

        let closed = state.metadata.ended.is_some();

        Ok(Self {
            id: session_id.to_string(),
            state,
            md_path: PathBuf::from(&entry.md_path),
            state_path,
            index_path,
            closed,
        })
    }

    /// Get the session ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the current session state
    pub fn state(&self) -> &SessionState {
        &self.state
    }

    /// Get the path to the markdown log
    pub fn md_path(&self) -> &Path {
        &self.md_path
    }

    /// Get the path to the JSON state file
    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    /// Log a user message
    pub async fn log_user_message(&mut self, content: &str) -> LoggerResult<()> {
        if self.closed {
            return Err(LoggerError::AlreadyClosed);
        }

        let now = Utc::now();

        // Add to state
        self.state.messages.push(SessionMessage {
            role: MessageRole::User,
            content: content.to_string(),
            timestamp: now,
            tool_name: None,
            tool_args: None,
            tool_result: None,
        });

        // Append to markdown
        let md = format_user_message(content, now);
        self.append_markdown(&md).await?;

        // Save state
        self.save_state().await?;

        Ok(())
    }

    /// Log an agent response
    pub async fn log_agent_response(&mut self, content: &str) -> LoggerResult<()> {
        if self.closed {
            return Err(LoggerError::AlreadyClosed);
        }

        let now = Utc::now();

        // Add to state
        self.state.messages.push(SessionMessage {
            role: MessageRole::Assistant,
            content: content.to_string(),
            timestamp: now,
            tool_name: None,
            tool_args: None,
            tool_result: None,
        });

        // Append to markdown
        let md = format_agent_response(content, now);
        self.append_markdown(&md).await?;

        // Save state
        self.save_state().await?;

        Ok(())
    }

    /// Log a tool call
    pub async fn log_tool_call(
        &mut self,
        name: &str,
        args: &Value,
        result: &Value,
    ) -> LoggerResult<()> {
        if self.closed {
            return Err(LoggerError::AlreadyClosed);
        }

        let now = Utc::now();

        // Add to state
        self.state.messages.push(SessionMessage {
            role: MessageRole::Tool,
            content: String::new(),
            timestamp: now,
            tool_name: Some(name.to_string()),
            tool_args: Some(args.clone()),
            tool_result: Some(result.clone()),
        });

        // Append to markdown
        let md = format_tool_call(name, args, result, now);
        self.append_markdown(&md).await?;

        // Save state
        self.save_state().await?;

        Ok(())
    }

    /// Update the task list
    pub async fn update_tasks(&mut self, tasks: Vec<Task>) -> LoggerResult<()> {
        if self.closed {
            return Err(LoggerError::AlreadyClosed);
        }

        self.state.tasks = tasks;

        // For tasks, we need to rewrite the markdown file to update the task section
        // For now, we'll just save the state - the markdown can be regenerated
        self.save_state().await?;

        Ok(())
    }

    /// Close the session
    pub async fn close(&mut self) -> LoggerResult<()> {
        if self.closed {
            return Err(LoggerError::AlreadyClosed);
        }

        let now = Utc::now();

        // Update state
        self.state.metadata.ended = Some(now);
        self.closed = true;

        // Save state
        self.save_state().await?;

        // Append task list to markdown if there are tasks
        if !self.state.tasks.is_empty() {
            let md = format_task_list(&self.state.tasks);
            self.append_markdown(&md).await?;
        }

        // Update index
        let session_id = self.id.clone();
        self.update_index(|index| {
            if let Some(entry) = index.sessions.iter_mut().find(|e| e.id == session_id) {
                entry.ended = Some(now);
            }
        })
        .await?;

        Ok(())
    }

    /// Save the current state to JSON
    async fn save_state(&self) -> LoggerResult<()> {
        let json = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.state_path, json).await?;
        Ok(())
    }

    /// Append content to the markdown log
    async fn append_markdown(&self, content: &str) -> LoggerResult<()> {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&self.md_path)
            .await?;
        file.write_all(content.as_bytes()).await?;
        Ok(())
    }

    /// Update the session index
    async fn update_index<F>(&self, update_fn: F) -> LoggerResult<()>
    where
        F: FnOnce(&mut SessionIndex),
    {
        // Load existing index or create new
        let mut index = if self.index_path.exists() {
            let content = fs::read_to_string(&self.index_path).await?;
            if content.is_empty() {
                SessionIndex::default()
            } else {
                serde_json::from_str(&content)?
            }
        } else {
            SessionIndex::default()
        };

        // Apply update
        update_fn(&mut index);

        // Save index
        let json = serde_json::to_string_pretty(&index)?;
        fs::write(&self.index_path, json).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_logger() -> (SessionLogger, TempDir, TempDir) {
        let kiln_dir = TempDir::new().unwrap();
        let state_dir = TempDir::new().unwrap();

        let logger = SessionLogger::create("test-workspace", kiln_dir.path(), state_dir.path())
            .await
            .unwrap();

        (logger, kiln_dir, state_dir)
    }

    #[tokio::test]
    async fn test_session_logger_create() {
        let kiln_dir = TempDir::new().unwrap();
        let state_dir = TempDir::new().unwrap();

        let logger = SessionLogger::create("crucible", kiln_dir.path(), state_dir.path())
            .await
            .unwrap();

        // Should create session folder
        assert!(kiln_dir.path().join("sessions/crucible").exists());

        // Should create markdown file
        assert!(logger.md_path().exists());

        // Should create state file
        assert!(logger.state_path().exists());

        // Should create index
        let index_path = state_dir.path().join("sessions/index.json");
        assert!(index_path.exists());

        // Check markdown content
        let md = fs::read_to_string(logger.md_path()).await.unwrap();
        assert!(md.contains("---"));
        assert!(md.contains("type: session"));
        assert!(md.contains("workspace: crucible"));
        assert!(md.contains("# Session Log"));
    }

    #[tokio::test]
    async fn test_session_logger_log_message() {
        let (mut logger, _kiln, _state) = create_test_logger().await;

        logger.log_user_message("Hello").await.unwrap();
        logger.log_agent_response("Hi!").await.unwrap();

        // Check markdown file
        let md = fs::read_to_string(logger.md_path()).await.unwrap();
        assert!(md.contains("### User"));
        assert!(md.contains("Hello"));
        assert!(md.contains("### Agent"));
        assert!(md.contains("Hi!"));

        // Check state file
        let state: SessionState =
            serde_json::from_str(&fs::read_to_string(logger.state_path()).await.unwrap()).unwrap();
        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[0].role, MessageRole::User);
        assert_eq!(state.messages[0].content, "Hello");
        assert_eq!(state.messages[1].role, MessageRole::Assistant);
        assert_eq!(state.messages[1].content, "Hi!");
    }

    #[tokio::test]
    async fn test_session_logger_log_tool_call() {
        let (mut logger, _kiln, _state) = create_test_logger().await;

        let args = serde_json::json!({"query": "rust"});
        let result = serde_json::json!({"count": 5});

        logger
            .log_tool_call("semantic_search", &args, &result)
            .await
            .unwrap();

        // Check markdown
        let md = fs::read_to_string(logger.md_path()).await.unwrap();
        assert!(md.contains("### Tool: semantic_search"));
        assert!(md.contains("\"query\": \"rust\""));
        assert!(md.contains("\"count\": 5"));

        // Check state
        let state: SessionState =
            serde_json::from_str(&fs::read_to_string(logger.state_path()).await.unwrap()).unwrap();
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, MessageRole::Tool);
        assert_eq!(
            state.messages[0].tool_name,
            Some("semantic_search".to_string())
        );
    }

    #[tokio::test]
    async fn test_session_logger_resume() {
        let kiln_dir = TempDir::new().unwrap();
        let state_dir = TempDir::new().unwrap();

        // Create initial session
        let mut logger = SessionLogger::create("test", kiln_dir.path(), state_dir.path())
            .await
            .unwrap();

        logger.log_user_message("First message").await.unwrap();
        let session_id = logger.id().to_string();

        // Drop the logger (simulating end of process)
        drop(logger);

        // Resume
        let resumed = SessionLogger::resume(&session_id, state_dir.path())
            .await
            .unwrap();

        assert_eq!(resumed.state().messages.len(), 1);
        assert_eq!(resumed.state().messages[0].content, "First message");
    }

    #[tokio::test]
    async fn test_session_logger_resume_and_continue() {
        let kiln_dir = TempDir::new().unwrap();
        let state_dir = TempDir::new().unwrap();

        // Create initial session
        let mut logger = SessionLogger::create("test", kiln_dir.path(), state_dir.path())
            .await
            .unwrap();

        logger.log_user_message("First").await.unwrap();
        let session_id = logger.id().to_string();
        drop(logger);

        // Resume and continue
        let mut resumed = SessionLogger::resume(&session_id, state_dir.path())
            .await
            .unwrap();

        resumed.log_user_message("Second").await.unwrap();
        assert_eq!(resumed.state().messages.len(), 2);

        // Check file was updated
        let md = fs::read_to_string(resumed.md_path()).await.unwrap();
        assert!(md.contains("First"));
        assert!(md.contains("Second"));
    }

    #[tokio::test]
    async fn test_session_logger_close() {
        let (mut logger, _kiln, state_dir) = create_test_logger().await;

        logger.log_user_message("Hello").await.unwrap();
        logger.close().await.unwrap();

        // Check ended timestamp in state
        let state: SessionState =
            serde_json::from_str(&fs::read_to_string(logger.state_path()).await.unwrap()).unwrap();
        assert!(state.metadata.ended.is_some());

        // Check index updated
        let index_path = state_dir.path().join("sessions/index.json");
        let index: SessionIndex =
            serde_json::from_str(&fs::read_to_string(index_path).await.unwrap()).unwrap();
        assert!(index.sessions[0].ended.is_some());

        // Verify can't log after close
        let result = logger.log_user_message("After close").await;
        assert!(matches!(result, Err(LoggerError::AlreadyClosed)));
    }

    #[tokio::test]
    async fn test_session_logger_with_tasks() {
        let (mut logger, _kiln, _state) = create_test_logger().await;

        let tasks = vec![
            Task {
                content: "Task 1".into(),
                status: super::super::types::TaskStatus::Completed,
            },
            Task {
                content: "Task 2".into(),
                status: super::super::types::TaskStatus::InProgress,
            },
        ];

        logger.update_tasks(tasks).await.unwrap();
        logger.close().await.unwrap();

        // Check state has tasks
        let state: SessionState =
            serde_json::from_str(&fs::read_to_string(logger.state_path()).await.unwrap()).unwrap();
        assert_eq!(state.tasks.len(), 2);

        // Check markdown has task list
        let md = fs::read_to_string(logger.md_path()).await.unwrap();
        assert!(md.contains("## Tasks"));
        assert!(md.contains("[x] Task 1"));
        assert!(md.contains("[~] Task 2"));
    }

    #[tokio::test]
    async fn test_session_logger_not_found() {
        let state_dir = TempDir::new().unwrap();

        let result = SessionLogger::resume("nonexistent/2024-01-01_0000", state_dir.path()).await;

        assert!(matches!(result, Err(LoggerError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_session_logger_session_id_format() {
        let (logger, _kiln, _state) = create_test_logger().await;

        // ID should be in format workspace/YYYY-MM-DD_HHMM
        let id = logger.id();
        assert!(id.starts_with("test-workspace/"));
        assert!(id.contains("-"));
        assert!(id.contains("_"));
    }
}
