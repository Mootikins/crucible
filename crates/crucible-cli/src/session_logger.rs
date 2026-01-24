//! Session logging integration for chat
//!
//! Provides optional session logging for the TUI chat interface.
//! Sessions are logged as JSONL files in `.crucible/sessions/<id>/`.

use crucible_observe::{
    load_events, truncate_for_log, LogEvent, SessionId, SessionType, SessionWriter,
    DEFAULT_TRUNCATE_THRESHOLD,
};
use std::path::PathBuf;
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Session logger for capturing chat events
pub struct SessionLogger {
    sessions_dir: PathBuf,
    writer: Mutex<Option<SessionWriter>>,
    accumulated_assistant: Mutex<String>,
}

impl SessionLogger {
    /// Create a new session logger for the given kiln path
    pub fn new(kiln_path: PathBuf) -> Self {
        let sessions_dir = kiln_path.join(".crucible").join("sessions");
        Self {
            sessions_dir,
            writer: Mutex::new(None),
            accumulated_assistant: Mutex::new(String::new()),
        }
    }

    /// Resume an existing session by ID
    ///
    /// Returns the loaded events if successful, or None if the session doesn't exist.
    pub async fn resume_session(&self, session_id: &SessionId) -> Option<Vec<LogEvent>> {
        // Try to open the existing session
        match SessionWriter::open(&self.sessions_dir, session_id.clone()).await {
            Ok(writer) => {
                debug!("Resumed session: {}", writer.id());

                // Load existing events before storing the writer
                let session_dir = self.sessions_dir.join(session_id.as_str());
                let events = match load_events(&session_dir).await {
                    Ok(e) => e,
                    Err(e) => {
                        warn!("Failed to load session events: {}", e);
                        Vec::new()
                    }
                };

                // Store the writer for future appends
                let mut writer_guard = self.writer.lock().await;
                *writer_guard = Some(writer);

                Some(events)
            }
            Err(e) => {
                warn!("Failed to resume session {}: {}", session_id, e);
                None
            }
        }
    }

    /// Get or create the session writer
    async fn ensure_writer(&self) -> Option<()> {
        let mut writer_guard = self.writer.lock().await;
        if writer_guard.is_none() {
            match SessionWriter::create(&self.sessions_dir, SessionType::Chat).await {
                Ok(w) => {
                    debug!("Created new session: {}", w.id());
                    *writer_guard = Some(w);
                }
                Err(e) => {
                    warn!("Failed to create session: {}", e);
                    return None;
                }
            }
        }
        Some(())
    }

    /// Get the current session ID (if session started)
    pub async fn session_id(&self) -> Option<SessionId> {
        let guard = self.writer.lock().await;
        guard.as_ref().map(|w| w.id().clone())
    }

    /// Log a user message
    pub async fn log_user_message(&self, content: &str) {
        if self.ensure_writer().await.is_none() {
            return;
        }

        let mut writer_guard = self.writer.lock().await;
        if let Some(writer) = writer_guard.as_mut() {
            if let Err(e) = writer.append(LogEvent::user(content)).await {
                warn!("Failed to log user message: {}", e);
            }
        }
    }

    /// Accumulate streaming assistant content
    pub async fn accumulate_assistant_chunk(&self, chunk: &str) {
        let mut acc = self.accumulated_assistant.lock().await;
        acc.push_str(chunk);
    }

    /// Flush accumulated assistant content as a complete message
    pub async fn flush_assistant_message(&self, model: Option<&str>) {
        let content = {
            let mut acc = self.accumulated_assistant.lock().await;
            std::mem::take(&mut *acc)
        };

        if content.is_empty() {
            return;
        }

        if self.ensure_writer().await.is_none() {
            return;
        }

        let mut writer_guard = self.writer.lock().await;
        if let Some(writer) = writer_guard.as_mut() {
            let event = if let Some(m) = model {
                LogEvent::assistant_with_model(&content, m, None)
            } else {
                LogEvent::assistant(&content)
            };

            if let Err(e) = writer.append(event).await {
                warn!("Failed to log assistant message: {}", e);
            }
        }
    }

    /// Log a tool call
    pub async fn log_tool_call(&self, id: &str, name: &str, args: serde_json::Value) {
        if self.ensure_writer().await.is_none() {
            return;
        }

        let mut writer_guard = self.writer.lock().await;
        if let Some(writer) = writer_guard.as_mut() {
            if let Err(e) = writer.append(LogEvent::tool_call(id, name, args)).await {
                warn!("Failed to log tool call: {}", e);
            }
        }
    }

    /// Log a tool result (automatically truncated if too large)
    pub async fn log_tool_result(&self, id: &str, result: &str) {
        if self.ensure_writer().await.is_none() {
            return;
        }

        let mut writer_guard = self.writer.lock().await;
        if let Some(writer) = writer_guard.as_mut() {
            let truncated = truncate_for_log(result, DEFAULT_TRUNCATE_THRESHOLD);
            let event = if truncated.truncated {
                LogEvent::tool_result_truncated(id, truncated.content, truncated.original_size)
            } else {
                LogEvent::tool_result(id, truncated.content)
            };
            if let Err(e) = writer.append(event).await {
                warn!("Failed to log tool result: {}", e);
            }
        }
    }

    /// Log an error
    pub async fn log_error(&self, message: &str, recoverable: bool) {
        if self.ensure_writer().await.is_none() {
            return;
        }

        let mut writer_guard = self.writer.lock().await;
        if let Some(writer) = writer_guard.as_mut() {
            if let Err(e) = writer.append(LogEvent::error(message, recoverable)).await {
                warn!("Failed to log error: {}", e);
            }
        }
    }

    /// Flush and close the session
    pub async fn finish(&self) {
        let mut writer_guard = self.writer.lock().await;
        if let Some(writer) = writer_guard.as_mut() {
            if let Err(e) = writer.flush().await {
                warn!("Failed to flush session: {}", e);
            }
        }
    }

    /// List all sessions in the kiln
    ///
    /// Returns session IDs sorted by creation time (newest first).
    pub async fn list_sessions(&self) -> Vec<SessionId> {
        match crucible_observe::list_sessions(&self.sessions_dir).await {
            Ok(mut ids) => {
                // Reverse to get newest first (they come sorted oldest first)
                ids.reverse();
                ids
            }
            Err(e) => {
                warn!("Failed to list sessions: {}", e);
                Vec::new()
            }
        }
    }
}

/// Load events from an existing session for resumption
pub async fn load_session_events(
    kiln_path: &std::path::Path,
    session_id: &SessionId,
) -> Result<Vec<LogEvent>, crucible_observe::SessionError> {
    let sessions_dir = kiln_path.join(".crucible").join("sessions");
    let session_dir = sessions_dir.join(session_id.as_str());
    load_events(&session_dir).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_session_logger_creates_session_lazily() {
        let tmp = TempDir::new().unwrap();
        let logger = SessionLogger::new(tmp.path().to_path_buf());

        // No session created yet
        assert!(logger.session_id().await.is_none());

        // Log a message - should create session
        logger.log_user_message("Hello").await;

        // Now session should exist
        let id = logger.session_id().await;
        assert!(id.is_some());
    }

    #[tokio::test]
    async fn test_session_logger_logs_messages() {
        let tmp = TempDir::new().unwrap();
        let logger = SessionLogger::new(tmp.path().to_path_buf());

        logger.log_user_message("Hello").await;
        logger.accumulate_assistant_chunk("Hi ").await;
        logger.accumulate_assistant_chunk("there!").await;
        logger.flush_assistant_message(Some("test-model")).await;
        logger.finish().await;

        let id = logger.session_id().await.unwrap();
        let events = load_session_events(tmp.path(), &id).await.unwrap();

        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], LogEvent::User { .. }));
        assert!(matches!(events[1], LogEvent::Assistant { .. }));
    }

    #[tokio::test]
    async fn test_session_logger_tool_calls() {
        let tmp = TempDir::new().unwrap();
        let logger = SessionLogger::new(tmp.path().to_path_buf());

        logger.log_user_message("Read file").await;
        logger
            .log_tool_call("tc1", "read_file", serde_json::json!({"path": "test.rs"}))
            .await;
        logger.log_tool_result("tc1", "fn main() {}").await;
        logger.finish().await;

        let id = logger.session_id().await.unwrap();
        let events = load_session_events(tmp.path(), &id).await.unwrap();

        assert_eq!(events.len(), 3);
    }
}
