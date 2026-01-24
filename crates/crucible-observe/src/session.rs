//! Session management and JSONL writing
//!
//! Sessions are stored as append-only JSONL files in `.crucible/sessions/<id>/session.jsonl`

use crate::events::LogEvent;
use crate::id::{SessionId, SessionType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, warn};

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: SessionId,
    pub session_type: SessionType,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub title: Option<String>,
    pub message_count: u32,
    pub kiln_path: PathBuf,
}

impl SessionMetadata {
    /// Create metadata for a new session
    pub fn new(id: SessionId, kiln_path: impl Into<PathBuf>) -> Self {
        Self {
            session_type: id.session_type(),
            id,
            started_at: Utc::now(),
            ended_at: None,
            title: None,
            message_count: 0,
            kiln_path: kiln_path.into(),
        }
    }
}

/// Errors that can occur during session operations
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("session not found: {0}")]
    NotFound(SessionId),

    #[error("session already exists: {0}")]
    AlreadyExists(SessionId),
}

/// A session writer for appending events to a session log
pub struct SessionWriter {
    id: SessionId,
    session_dir: PathBuf,
    file: Option<File>,
    event_count: u32,
}

impl SessionWriter {
    /// Create a new session writer, creating the session directory
    pub async fn create(
        sessions_dir: impl AsRef<Path>,
        session_type: SessionType,
    ) -> Result<Self, SessionError> {
        let id = SessionId::new(session_type, Utc::now());
        let session_dir = sessions_dir.as_ref().join(id.as_str());

        // Create session directory and workspace
        fs::create_dir_all(&session_dir).await?;
        fs::create_dir_all(session_dir.join("workspace")).await?;

        debug!("created session directory: {}", session_dir.display());

        Ok(Self {
            id,
            session_dir,
            file: None,
            event_count: 0,
        })
    }

    /// Create a subagent session under a parent session
    ///
    /// The subagent session is created in `{parent_session_dir}/subagents/{subagent_id}/`
    /// Returns the writer and the relative wikilink path.
    pub async fn create_subagent(
        parent_session_dir: impl AsRef<Path>,
    ) -> Result<(Self, String), SessionError> {
        let id = SessionId::new(SessionType::Subagent, Utc::now());
        let subagents_dir = parent_session_dir.as_ref().join("subagents");
        let session_dir = subagents_dir.join(id.as_str());

        fs::create_dir_all(&session_dir).await?;

        debug!(
            "created subagent session directory: {}",
            session_dir.display()
        );

        let wikilink = format!("[[.subagents/{}/session]]", id.as_str());

        Ok((
            Self {
                id,
                session_dir,
                file: None,
                event_count: 0,
            },
            wikilink,
        ))
    }

    /// Open an existing session for appending
    pub async fn open(sessions_dir: impl AsRef<Path>, id: SessionId) -> Result<Self, SessionError> {
        let session_dir = sessions_dir.as_ref().join(id.as_str());

        if !session_dir.exists() {
            return Err(SessionError::NotFound(id));
        }

        // Count existing events
        let jsonl_path = session_dir.join("session.jsonl");
        let event_count = if jsonl_path.exists() {
            let file = File::open(&jsonl_path).await?;
            let reader = BufReader::new(file);
            let mut lines = reader.lines();
            let mut count = 0u32;
            while lines.next_line().await?.is_some() {
                count += 1;
            }
            count
        } else {
            0
        };

        debug!("opened session {} with {} events", id, event_count);

        Ok(Self {
            id,
            session_dir,
            file: None,
            event_count,
        })
    }

    /// Get the session ID
    pub fn id(&self) -> &SessionId {
        &self.id
    }

    /// Get the session directory path
    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    /// Get the workspace directory path
    pub fn workspace_dir(&self) -> PathBuf {
        self.session_dir.join("workspace")
    }

    /// Get the JSONL file path
    pub fn jsonl_path(&self) -> PathBuf {
        self.session_dir.join("session.jsonl")
    }

    /// Get the markdown file path
    pub fn markdown_path(&self) -> PathBuf {
        self.session_dir.join("session.md")
    }

    /// Get the current event count
    pub fn event_count(&self) -> u32 {
        self.event_count
    }

    /// Append an event to the session log
    pub async fn append(&mut self, event: LogEvent) -> Result<(), SessionError> {
        // Lazy-open the file with append mode
        if self.file.is_none() {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.jsonl_path())
                .await?;
            self.file = Some(file);
        }

        let file = self.file.as_mut().unwrap();

        // Serialize and write
        let mut line = event.to_jsonl()?;
        line.push('\n');

        file.write_all(line.as_bytes()).await?;
        file.flush().await?;

        self.event_count += 1;
        Ok(())
    }

    /// Flush any buffered writes
    pub async fn flush(&mut self) -> Result<(), SessionError> {
        if let Some(file) = &mut self.file {
            file.flush().await?;
        }
        Ok(())
    }
}

/// Load all events from a session log
pub async fn load_events(session_dir: impl AsRef<Path>) -> Result<Vec<LogEvent>, SessionError> {
    let jsonl_path = session_dir.as_ref().join("session.jsonl");

    if !jsonl_path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(&jsonl_path).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut events = Vec::new();
    let mut line_num = 0;

    while let Some(line) = lines.next_line().await? {
        line_num += 1;
        if line.trim().is_empty() {
            continue;
        }

        match LogEvent::from_jsonl(&line) {
            Ok(event) => events.push(event),
            Err(e) => {
                warn!("failed to parse line {line_num} in session log: {e}");
                // Continue loading other events
            }
        }
    }

    Ok(events)
}

/// List all session IDs in a sessions directory
pub async fn list_sessions(sessions_dir: impl AsRef<Path>) -> Result<Vec<SessionId>, SessionError> {
    let sessions_dir = sessions_dir.as_ref();

    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(sessions_dir).await?;
    let mut ids = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            let name = entry.file_name();
            if let Some(name_str) = name.to_str() {
                if let Ok(id) = SessionId::parse(name_str) {
                    ids.push(id);
                }
            }
        }
    }

    // Sort by ID (which includes timestamp, so newest last)
    ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup() -> TempDir {
        TempDir::new().unwrap()
    }

    #[tokio::test]
    async fn test_create_session() {
        let dir = setup().await;
        let sessions_dir = dir.path().join("sessions");

        let writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
            .await
            .unwrap();

        assert!(writer.id().as_str().starts_with("chat-"));
        assert!(writer.session_dir().exists());
        assert!(writer.workspace_dir().exists());
    }

    #[tokio::test]
    async fn test_append_events() {
        let dir = setup().await;
        let sessions_dir = dir.path().join("sessions");

        let mut writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
            .await
            .unwrap();

        writer
            .append(LogEvent::system("System prompt"))
            .await
            .unwrap();
        writer.append(LogEvent::user("Hello")).await.unwrap();
        writer.append(LogEvent::assistant("Hi!")).await.unwrap();

        assert_eq!(writer.event_count(), 3);
        assert!(writer.jsonl_path().exists());
    }

    #[tokio::test]
    async fn test_load_events_roundtrip() {
        let dir = setup().await;
        let sessions_dir = dir.path().join("sessions");

        let mut writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
            .await
            .unwrap();

        let id = writer.id().clone();

        writer.append(LogEvent::system("System")).await.unwrap();
        writer.append(LogEvent::user("Hello")).await.unwrap();
        writer.append(LogEvent::assistant("Hi!")).await.unwrap();
        writer.flush().await.unwrap();

        // Load events back
        let events = load_events(sessions_dir.join(id.as_str())).await.unwrap();

        assert_eq!(events.len(), 3);

        match &events[0] {
            LogEvent::System { content, .. } => assert_eq!(content, "System"),
            _ => panic!("wrong event type"),
        }

        match &events[1] {
            LogEvent::User { content, .. } => assert_eq!(content, "Hello"),
            _ => panic!("wrong event type"),
        }

        match &events[2] {
            LogEvent::Assistant { content, .. } => assert_eq!(content, "Hi!"),
            _ => panic!("wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_reopen_session() {
        let dir = setup().await;
        let sessions_dir = dir.path().join("sessions");

        let id = {
            let mut writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
                .await
                .unwrap();
            writer.append(LogEvent::user("First")).await.unwrap();
            writer.append(LogEvent::user("Second")).await.unwrap();
            writer.id().clone()
        };

        // Reopen and continue
        let mut writer = SessionWriter::open(&sessions_dir, id.clone())
            .await
            .unwrap();
        assert_eq!(writer.event_count(), 2);

        writer.append(LogEvent::user("Third")).await.unwrap();
        assert_eq!(writer.event_count(), 3);

        // Verify all events
        let events = load_events(sessions_dir.join(id.as_str())).await.unwrap();
        assert_eq!(events.len(), 3);
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let dir = setup().await;
        let sessions_dir = dir.path().join("sessions");

        // Create multiple sessions
        let mut ids = Vec::new();
        for _ in 0..3 {
            let writer = SessionWriter::create(&sessions_dir, SessionType::Chat)
                .await
                .unwrap();
            ids.push(writer.id().clone());
        }

        let listed = list_sessions(&sessions_dir).await.unwrap();
        assert_eq!(listed.len(), 3);

        // All created sessions should be listed
        for id in &ids {
            assert!(listed.contains(id));
        }
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        let dir = setup().await;
        let sessions_dir = dir.path().join("nonexistent");

        let listed = list_sessions(&sessions_dir).await.unwrap();
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn test_open_nonexistent_session() {
        let dir = setup().await;
        let sessions_dir = dir.path().join("sessions");
        fs::create_dir_all(&sessions_dir).await.unwrap();

        let id = SessionId::parse("chat-20260104-1530-a1b2").unwrap();
        let result = SessionWriter::open(&sessions_dir, id).await;

        assert!(matches!(result, Err(SessionError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_create_subagent_session() {
        let dir = setup().await;
        let sessions_dir = dir.path().join("sessions");

        let parent = SessionWriter::create(&sessions_dir, SessionType::Chat)
            .await
            .unwrap();
        let parent_dir = parent.session_dir().to_path_buf();

        let (mut subagent, wikilink) = SessionWriter::create_subagent(&parent_dir).await.unwrap();

        assert!(subagent.id().as_str().starts_with("sub-"));
        assert!(subagent.session_dir().exists());
        assert!(wikilink.starts_with("[[.subagents/sub-"));
        assert!(wikilink.ends_with("/session]]"));

        subagent
            .append(LogEvent::user("Subagent prompt"))
            .await
            .unwrap();
        subagent
            .append(LogEvent::assistant("Subagent response"))
            .await
            .unwrap();

        assert_eq!(subagent.event_count(), 2);
        assert!(subagent.jsonl_path().exists());

        let events = load_events(subagent.session_dir()).await.unwrap();
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn test_subagent_nested_under_parent() {
        let dir = setup().await;
        let sessions_dir = dir.path().join("sessions");

        let parent = SessionWriter::create(&sessions_dir, SessionType::Chat)
            .await
            .unwrap();
        let parent_dir = parent.session_dir().to_path_buf();

        let (subagent, _) = SessionWriter::create_subagent(&parent_dir).await.unwrap();

        let subagent_path = subagent.session_dir();
        assert!(subagent_path.starts_with(&parent_dir));
        assert!(subagent_path.to_string_lossy().contains("subagents"));
    }
}
