//! CrucibleAgent - Rig agent wrapper with session logging
//!
//! This module provides `CrucibleAgent`, which wraps a Rig Agent and
//! automatically logs all conversations to a session.

use crate::session::{LoggerError, SessionLogger, SessionState, Task};
use rig::completion::{CompletionModel, Prompt, PromptError};
use std::path::Path;
use thiserror::Error;

/// Errors from CrucibleAgent operations
#[derive(Debug, Error)]
pub enum CrucibleAgentError {
    /// Session logging error
    #[error("Session error: {0}")]
    Session(#[from] LoggerError),

    /// Prompt error from the underlying agent
    #[error("Prompt error: {0}")]
    Prompt(#[from] PromptError),
}

/// Result type for CrucibleAgent operations
pub type CrucibleAgentResult<T> = Result<T, CrucibleAgentError>;

/// A Rig agent wrapped with session logging
///
/// CrucibleAgent automatically logs all conversations to a session,
/// creating both human-readable markdown and machine-readable JSON.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_rig::CrucibleAgent;
/// use rig::providers::ollama;
///
/// let client = ollama::Client::new();
/// let agent = client.agent("llama3.2")
///     .preamble("You are a helpful assistant.")
///     .build();
///
/// let mut crucible_agent = CrucibleAgent::new(
///     agent,
///     "my-workspace",
///     &kiln_path,
///     &state_path,
/// ).await?;
///
/// let response = crucible_agent.prompt("Hello!").await?;
/// println!("{}", response);
///
/// crucible_agent.end_session().await?;
/// ```
pub struct CrucibleAgent<A> {
    /// The inner Rig agent
    agent: A,

    /// Session logger
    logger: SessionLogger,
}

impl<A> CrucibleAgent<A> {
    /// Create a new CrucibleAgent with a fresh session
    ///
    /// # Arguments
    ///
    /// * `agent` - The Rig agent to wrap
    /// * `workspace` - Workspace name (usually directory name)
    /// * `kiln_path` - Path to the kiln directory for markdown logs
    /// * `state_path` - Path for JSON state files (e.g., ~/.crucible)
    pub async fn new(
        agent: A,
        workspace: &str,
        kiln_path: &Path,
        state_path: &Path,
    ) -> CrucibleAgentResult<Self> {
        let logger = SessionLogger::create(workspace, kiln_path, state_path).await?;

        Ok(Self { agent, logger })
    }

    /// Resume an existing session with a new agent
    ///
    /// This allows continuing a previous conversation. The agent should
    /// be configured the same way as the original session.
    ///
    /// # Arguments
    ///
    /// * `session_id` - Session ID (format: workspace/YYYY-MM-DD_HHMM)
    /// * `agent` - The Rig agent to use
    /// * `state_path` - Path for state files
    pub async fn resume(
        session_id: &str,
        agent: A,
        state_path: &Path,
    ) -> CrucibleAgentResult<Self> {
        let logger = SessionLogger::resume(session_id, state_path).await?;

        Ok(Self { agent, logger })
    }

    /// Get the session ID
    pub fn session_id(&self) -> &str {
        self.logger.id()
    }

    /// Get the current session state
    pub fn session_state(&self) -> &SessionState {
        self.logger.state()
    }

    /// Get a reference to the inner agent
    pub fn inner(&self) -> &A {
        &self.agent
    }

    /// Get a mutable reference to the inner agent
    pub fn inner_mut(&mut self) -> &mut A {
        &mut self.agent
    }

    /// Update the task list
    pub async fn update_tasks(&mut self, tasks: Vec<Task>) -> CrucibleAgentResult<()> {
        self.logger.update_tasks(tasks).await?;
        Ok(())
    }

    /// End the session
    ///
    /// This closes the session and prevents further logging.
    pub async fn end_session(&mut self) -> CrucibleAgentResult<()> {
        self.logger.close().await?;
        Ok(())
    }

    /// Get the path to the markdown log
    pub fn md_path(&self) -> &Path {
        self.logger.md_path()
    }
}

impl<M> CrucibleAgent<rig::agent::Agent<M>>
where
    M: CompletionModel,
{
    /// Send a prompt to the agent and log the conversation
    ///
    /// This method:
    /// 1. Logs the user message
    /// 2. Sends the prompt to the underlying agent
    /// 3. Logs the agent's response
    /// 4. Returns the response
    pub async fn prompt(&mut self, message: &str) -> CrucibleAgentResult<String> {
        // Log user message
        self.logger.log_user_message(message).await?;

        // Get response from agent
        let response = self.agent.prompt(message).await?;

        // Log agent response
        self.logger.log_agent_response(&response).await?;

        Ok(response)
    }

    /// Log a tool call that occurred during agent execution
    ///
    /// This is useful when you're handling tool calls manually or
    /// want to record tool usage from agent execution.
    pub async fn log_tool_call(
        &mut self,
        name: &str,
        args: &serde_json::Value,
        result: &serde_json::Value,
    ) -> CrucibleAgentResult<()> {
        self.logger.log_tool_call(name, args, result).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::client::{CompletionClient, Nothing};
    use rig::providers::ollama;
    use tempfile::TempDir;

    fn create_test_agent() -> rig::agent::Agent<ollama::CompletionModel> {
        let client = ollama::Client::builder().api_key(Nothing).build().unwrap();

        client
            .agent("llama3.2")
            .preamble("You are a test assistant.")
            .build()
    }

    #[tokio::test]
    async fn test_crucible_agent_creates_session() {
        let kiln_dir = TempDir::new().unwrap();
        let state_dir = TempDir::new().unwrap();

        let agent = create_test_agent();
        let crucible_agent =
            CrucibleAgent::new(agent, "test-workspace", kiln_dir.path(), state_dir.path())
                .await
                .unwrap();

        // Should create session directory
        assert!(kiln_dir.path().join("sessions/test-workspace").exists());

        // Session ID should be in correct format
        assert!(crucible_agent.session_id().starts_with("test-workspace/"));
    }

    #[tokio::test]
    async fn test_crucible_agent_session_state() {
        let kiln_dir = TempDir::new().unwrap();
        let state_dir = TempDir::new().unwrap();

        let agent = create_test_agent();
        let crucible_agent = CrucibleAgent::new(agent, "test", kiln_dir.path(), state_dir.path())
            .await
            .unwrap();

        // Initially no messages
        assert_eq!(crucible_agent.session_state().messages.len(), 0);
    }

    #[tokio::test]
    async fn test_crucible_agent_resume() {
        let kiln_dir = TempDir::new().unwrap();
        let state_dir = TempDir::new().unwrap();

        // Create initial session
        let agent = create_test_agent();
        let crucible_agent = CrucibleAgent::new(agent, "test", kiln_dir.path(), state_dir.path())
            .await
            .unwrap();

        let session_id = crucible_agent.session_id().to_string();
        drop(crucible_agent);

        // Resume with new agent
        let new_agent = create_test_agent();
        let resumed = CrucibleAgent::resume(&session_id, new_agent, state_dir.path())
            .await
            .unwrap();

        assert_eq!(resumed.session_id(), session_id);
    }

    #[tokio::test]
    async fn test_crucible_agent_end_session() {
        let kiln_dir = TempDir::new().unwrap();
        let state_dir = TempDir::new().unwrap();

        let agent = create_test_agent();
        let mut crucible_agent =
            CrucibleAgent::new(agent, "test", kiln_dir.path(), state_dir.path())
                .await
                .unwrap();

        crucible_agent.end_session().await.unwrap();

        // Session should be marked as ended
        assert!(crucible_agent.session_state().metadata.ended.is_some());
    }

    #[tokio::test]
    async fn test_crucible_agent_update_tasks() {
        let kiln_dir = TempDir::new().unwrap();
        let state_dir = TempDir::new().unwrap();

        let agent = create_test_agent();
        let mut crucible_agent =
            CrucibleAgent::new(agent, "test", kiln_dir.path(), state_dir.path())
                .await
                .unwrap();

        let tasks = vec![Task {
            content: "Task 1".into(),
            status: crate::session::TaskStatus::InProgress,
        }];

        crucible_agent.update_tasks(tasks).await.unwrap();

        assert_eq!(crucible_agent.session_state().tasks.len(), 1);
    }

    #[tokio::test]
    async fn test_crucible_agent_inner_access() {
        let kiln_dir = TempDir::new().unwrap();
        let state_dir = TempDir::new().unwrap();

        let agent = create_test_agent();
        let crucible_agent = CrucibleAgent::new(agent, "test", kiln_dir.path(), state_dir.path())
            .await
            .unwrap();

        // Can access inner agent
        let _inner = crucible_agent.inner();
    }

    // Note: Testing actual prompt() requires a running Ollama instance
    // These tests verify the wrapper structure without network calls

    #[tokio::test]
    #[ignore = "requires running Ollama"]
    async fn test_crucible_agent_prompt_logs_conversation() {
        let kiln_dir = TempDir::new().unwrap();
        let state_dir = TempDir::new().unwrap();

        // This would need a real Ollama instance
        let agent = create_test_agent();

        let mut crucible_agent =
            CrucibleAgent::new(agent, "test", kiln_dir.path(), state_dir.path())
                .await
                .unwrap();

        let response = crucible_agent.prompt("Say hello").await.unwrap();

        // Should have 2 messages (user + assistant)
        assert_eq!(crucible_agent.session_state().messages.len(), 2);
        assert!(!response.is_empty());

        // Check markdown was written
        let md = tokio::fs::read_to_string(crucible_agent.md_path())
            .await
            .unwrap();
        assert!(md.contains("Say hello"));
        assert!(md.contains("### Agent"));
    }
}
