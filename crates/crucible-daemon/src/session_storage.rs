//! Session persistence to kiln storage.
//!
//! Sessions are stored in their owning kiln at:
//! `{kiln}/.crucible/sessions/{session_id}/`
//!
//! Contents:
//! - `meta.json` - Session metadata
//! - `session.jsonl` - Event log (append-only)
//! - `session.md` - Human-readable markdown conversation log

use crate::session_manager::SessionError;
use async_trait::async_trait;
use chrono::Utc;
use crucible_config::is_crucible_home;
use crucible_core::session::{Session, SessionSummary};
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Trait for session persistence.
///
/// Implementations provide different storage backends for persisting
/// sessions to disk or other storage systems.
#[async_trait]
#[allow(dead_code)] // list and append_markdown are part of API but not yet used in production code paths
pub trait SessionStorage: Send + Sync {
    /// Save a session to storage.
    ///
    /// Creates the session directory if needed and writes session metadata.
    async fn save(&self, session: &Session) -> Result<(), SessionError>;

    /// Load a session from storage.
    ///
    /// Returns `SessionError::NotFound` if the session doesn't exist.
    async fn load(&self, session_id: &str, kiln: &Path) -> Result<Session, SessionError>;

    /// List sessions in a kiln.
    ///
    /// Returns summaries of all sessions found in the kiln's session directory.
    /// Returns an empty vec if the sessions directory doesn't exist.
    async fn list(&self, kiln: &Path) -> Result<Vec<SessionSummary>, SessionError>;

    /// Append an event to the session's JSONL log.
    ///
    /// Events are appended as single lines to enable streaming reads.
    async fn append_event(&self, session: &Session, event: &str) -> Result<(), SessionError>;

    /// Append a human-readable entry to the session's markdown log.
    ///
    /// Creates the markdown file with frontmatter on first call.
    /// Subsequent calls append timestamped entries.
    async fn append_markdown(
        &self,
        session: &Session,
        role: &str,
        content: &str,
    ) -> Result<(), SessionError>;

    /// Load events from the session's JSONL log with pagination.
    ///
    /// Returns events in chronological order (oldest first).
    /// Use `offset` to skip events and `limit` to cap the number returned.
    async fn load_events(
        &self,
        session_id: &str,
        kiln: &Path,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError>;

    /// Count total events in the session's JSONL log.
    async fn count_events(&self, session_id: &str, kiln: &Path) -> Result<usize, SessionError>;
}

/// File-based session storage.
///
/// Stores sessions as files in the kiln's `.crucible/sessions/` directory.
#[derive(Debug, Clone, Default)]
pub struct FileSessionStorage;

impl FileSessionStorage {
    /// Create a new file-based session storage.
    pub fn new() -> Self {
        Self
    }

    /// Get the storage directory for a session.
    ///
    /// When the kiln is the crucible home (`~/.crucible/`), sessions go directly
    /// to `~/.crucible/sessions/{id}` to avoid double-nesting `.crucible/.crucible/`.
    /// Otherwise returns `{kiln}/.crucible/sessions/{session_id}/`.
    fn session_dir(session: &Session) -> std::path::PathBuf {
        Self::sessions_base(&session.kiln).join(&session.id)
    }

    /// Get the storage directory for a session by ID and kiln.
    fn session_dir_by_id(session_id: &str, kiln: &Path) -> std::path::PathBuf {
        Self::sessions_base(kiln).join(session_id)
    }

    /// Get the base sessions directory for a kiln.
    ///
    /// For crucible home: `~/.crucible/sessions/`
    /// For other kilns: `{kiln}/.crucible/sessions/`
    fn sessions_base(kiln: &Path) -> std::path::PathBuf {
        if is_crucible_home(kiln) {
            kiln.join("sessions")
        } else {
            kiln.join(".crucible").join("sessions")
        }
    }
}

#[async_trait]
impl SessionStorage for FileSessionStorage {
    async fn save(&self, session: &Session) -> Result<(), SessionError> {
        let dir = Self::session_dir(session);
        fs::create_dir_all(&dir)
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;

        // Save session metadata as JSON
        let meta_path = dir.join("meta.json");
        let json = serde_json::to_string_pretty(session)
            .map_err(|e| SessionError::IoError(e.to_string()))?;
        fs::write(&meta_path, json)
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;

        Ok(())
    }

    async fn load(&self, session_id: &str, kiln: &Path) -> Result<Session, SessionError> {
        let dir = Self::session_dir_by_id(session_id, kiln);
        // Try meta.json first, fall back to legacy session.json for backward compatibility
        let meta_path = dir.join("meta.json");
        let legacy_path = dir.join("session.json");
        let path = if meta_path.exists() {
            meta_path
        } else {
            legacy_path
        };

        let json = fs::read_to_string(&path).await.map_err(|e| {
            // Distinguish between "not found" and other IO errors
            if e.kind() == std::io::ErrorKind::NotFound {
                SessionError::NotFound(session_id.to_string())
            } else {
                SessionError::IoError(format!(
                    "Failed to load session '{}' from {}: {}",
                    session_id,
                    path.display(),
                    e
                ))
            }
        })?;

        serde_json::from_str(&json).map_err(|e| {
            SessionError::IoError(format!(
                "Failed to parse session '{}' JSON: {}",
                session_id, e
            ))
        })
    }

    async fn list(&self, kiln: &Path) -> Result<Vec<SessionSummary>, SessionError> {
        let sessions_dir = Self::sessions_base(kiln);

        if !sessions_dir.exists() {
            return Ok(vec![]);
        }

        let mut summaries = vec![];
        let mut entries = fs::read_dir(&sessions_dir)
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?
        {
            if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                let session_id = entry.file_name().to_string_lossy().to_string();
                if let Ok(session) = self.load(&session_id, kiln).await {
                    summaries.push(SessionSummary::from(&session));
                }
            }
        }

        Ok(summaries)
    }

    async fn append_event(&self, session: &Session, event: &str) -> Result<(), SessionError> {
        let dir = Self::session_dir(session);

        // Ensure directory exists
        fs::create_dir_all(&dir)
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;

        let jsonl_path = dir.join("session.jsonl");

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&jsonl_path)
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;

        file.write_all(event.as_bytes())
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;
        file.write_all(b"\n")
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;

        Ok(())
    }

    async fn append_markdown(
        &self,
        session: &Session,
        role: &str,
        content: &str,
    ) -> Result<(), SessionError> {
        let dir = Self::session_dir(session);

        // Ensure directory exists
        fs::create_dir_all(&dir)
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;

        let md_path = dir.join("session.md");

        // Create file with frontmatter if it doesn't exist
        if !md_path.exists() {
            let session_type_name = match session.session_type {
                crucible_core::session::SessionType::Chat => "Chat",
                crucible_core::session::SessionType::Agent => "Agent",
                crucible_core::session::SessionType::Workflow => "Workflow",
            };

            let frontmatter = format!(
                "---\nsession_id: {}\ntype: {}\nkiln: {}\nworkspace: {}\nstarted: {}\n---\n\n# {} Session\n\n",
                session.id,
                session.session_type.as_prefix(),
                session.kiln.display(),
                session.workspace.display(),
                session.started_at.to_rfc3339(),
                session_type_name,
            );
            fs::write(&md_path, frontmatter)
                .await
                .map_err(|e| SessionError::IoError(e.to_string()))?;
        }

        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let entry = format!("\n## {} - {}\n\n{}\n", role, timestamp, content);

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&md_path)
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;

        file.write_all(entry.as_bytes())
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;

        Ok(())
    }

    async fn load_events(
        &self,
        session_id: &str,
        kiln: &Path,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError> {
        let dir = Self::session_dir_by_id(session_id, kiln);
        let jsonl_path = dir.join("session.jsonl");

        if !jsonl_path.exists() {
            return Ok(vec![]);
        }

        let content = fs::read_to_string(&jsonl_path)
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;

        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(usize::MAX);

        let events: Vec<serde_json::Value> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .skip(offset)
            .take(limit)
            .filter_map(|line| match serde_json::from_str(line) {
                Ok(val) => Some(val),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        line_preview = %line.chars().take(100).collect::<String>(),
                        "Failed to parse session event, skipping"
                    );
                    None
                }
            })
            .collect();

        Ok(events)
    }

    async fn count_events(&self, session_id: &str, kiln: &Path) -> Result<usize, SessionError> {
        let dir = Self::session_dir_by_id(session_id, kiln);
        let jsonl_path = dir.join("session.jsonl");

        if !jsonl_path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(&jsonl_path)
            .await
            .map_err(|e| SessionError::IoError(e.to_string()))?;

        let count = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::session::SessionType;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_session_storage_save_load() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        let session = Session::new(SessionType::Chat, tmp.path().to_path_buf());
        let session_id = session.id.clone();

        storage.save(&session).await.unwrap();

        let loaded = storage.load(&session_id, tmp.path()).await.unwrap();
        assert_eq!(loaded.id, session_id);
        assert_eq!(loaded.session_type, SessionType::Chat);
    }

    #[tokio::test]
    async fn test_session_storage_list() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        // Create two sessions
        let session1 = Session::new(SessionType::Chat, tmp.path().to_path_buf());
        let session2 = Session::new(SessionType::Agent, tmp.path().to_path_buf());

        storage.save(&session1).await.unwrap();
        storage.save(&session2).await.unwrap();

        let summaries = storage.list(tmp.path()).await.unwrap();
        assert_eq!(summaries.len(), 2);
    }

    #[tokio::test]
    async fn test_session_storage_append_event() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        let session = Session::new(SessionType::Chat, tmp.path().to_path_buf());
        storage.save(&session).await.unwrap();

        storage
            .append_event(&session, r#"{"type":"text","content":"hello"}"#)
            .await
            .unwrap();
        storage
            .append_event(&session, r#"{"type":"text","content":"world"}"#)
            .await
            .unwrap();

        // Verify events were appended
        let jsonl_path = tmp
            .path()
            .join(".crucible")
            .join("sessions")
            .join(&session.id)
            .join("session.jsonl");
        let content = tokio::fs::read_to_string(&jsonl_path).await.unwrap();
        assert!(content.contains("hello"));
        assert!(content.contains("world"));
        assert_eq!(content.lines().count(), 2);
    }

    #[tokio::test]
    async fn test_session_storage_load_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        let result = storage.load("nonexistent-session", tmp.path()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_session_storage_list_empty() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        let summaries = storage.list(tmp.path()).await.unwrap();
        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn test_session_storage_append_event_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        // Create session but don't save it first
        let session = Session::new(SessionType::Chat, tmp.path().to_path_buf());

        // append_event should create the directory if needed
        storage
            .append_event(&session, r#"{"type":"text","content":"test"}"#)
            .await
            .unwrap();

        // Verify the directory and file were created
        let jsonl_path = tmp
            .path()
            .join(".crucible")
            .join("sessions")
            .join(&session.id)
            .join("session.jsonl");
        assert!(jsonl_path.exists());
    }

    #[tokio::test]
    async fn test_session_storage_preserves_all_fields() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        let connected_kiln = tmp.path().join("other-kiln");
        let workspace = tmp.path().join("workspace");

        let session = Session::new(SessionType::Agent, tmp.path().to_path_buf())
            .with_workspace(workspace.clone())
            .with_connected_kiln(connected_kiln.clone())
            .with_title("Test Session");
        let session_id = session.id.clone();

        storage.save(&session).await.unwrap();

        let loaded = storage.load(&session_id, tmp.path()).await.unwrap();
        assert_eq!(loaded.session_type, SessionType::Agent);
        assert_eq!(loaded.workspace, workspace);
        assert_eq!(loaded.connected_kilns, vec![connected_kiln]);
        assert_eq!(loaded.title, Some("Test Session".to_string()));
    }

    #[tokio::test]
    async fn test_session_storage_append_markdown() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        let session = Session::new(SessionType::Chat, tmp.path().to_path_buf());
        storage.save(&session).await.unwrap();

        storage
            .append_markdown(&session, "User", "Hello!")
            .await
            .unwrap();
        storage
            .append_markdown(&session, "Assistant", "Hi there!")
            .await
            .unwrap();

        // Verify markdown was created
        let md_path = tmp
            .path()
            .join(".crucible")
            .join("sessions")
            .join(&session.id)
            .join("session.md");

        let content = tokio::fs::read_to_string(&md_path).await.unwrap();

        // Check frontmatter
        assert!(content.starts_with("---\n"));
        assert!(content.contains(&format!("session_id: {}", session.id)));
        assert!(content.contains("type: chat"));

        // Check entries
        assert!(content.contains("## User -"));
        assert!(content.contains("Hello!"));
        assert!(content.contains("## Assistant -"));
        assert!(content.contains("Hi there!"));
    }

    #[tokio::test]
    async fn test_session_storage_markdown_creates_frontmatter_once() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        let session = Session::new(SessionType::Agent, tmp.path().to_path_buf());
        storage.save(&session).await.unwrap();

        storage
            .append_markdown(&session, "User", "First")
            .await
            .unwrap();
        storage
            .append_markdown(&session, "Agent", "Second")
            .await
            .unwrap();
        storage
            .append_markdown(&session, "User", "Third")
            .await
            .unwrap();

        let md_path = tmp
            .path()
            .join(".crucible")
            .join("sessions")
            .join(&session.id)
            .join("session.md");

        let content = tokio::fs::read_to_string(&md_path).await.unwrap();

        // Should only have one frontmatter block
        let frontmatter_count = content.matches("---\n").count();
        assert_eq!(frontmatter_count, 2); // Opening and closing ---

        // Should have all entries
        assert!(content.contains("First"));
        assert!(content.contains("Second"));
        assert!(content.contains("Third"));
    }

    #[tokio::test]
    async fn test_session_storage_append_markdown_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        // Create session but don't save it first
        let session = Session::new(SessionType::Workflow, tmp.path().to_path_buf());

        // append_markdown should create the directory if needed
        storage
            .append_markdown(&session, "System", "Starting workflow")
            .await
            .unwrap();

        // Verify the directory and file were created
        let md_path = tmp
            .path()
            .join(".crucible")
            .join("sessions")
            .join(&session.id)
            .join("session.md");
        assert!(md_path.exists());

        let content = tokio::fs::read_to_string(&md_path).await.unwrap();
        assert!(content.contains("type: workflow"));
        assert!(content.contains("# Workflow Session"));
        assert!(content.contains("Starting workflow"));
    }

    #[tokio::test]
    async fn test_session_storage_load_events() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        let session = Session::new(SessionType::Chat, tmp.path().to_path_buf());
        storage.save(&session).await.unwrap();

        // Append some events
        storage
            .append_event(&session, r#"{"type":"text","content":"first"}"#)
            .await
            .unwrap();
        storage
            .append_event(&session, r#"{"type":"text","content":"second"}"#)
            .await
            .unwrap();
        storage
            .append_event(&session, r#"{"type":"text","content":"third"}"#)
            .await
            .unwrap();

        // Load all events
        let events = storage
            .load_events(&session.id, tmp.path(), None, None)
            .await
            .unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["content"], "first");
        assert_eq!(events[2]["content"], "third");

        // Load with pagination
        let events = storage
            .load_events(&session.id, tmp.path(), Some(2), Some(1))
            .await
            .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["content"], "second");
        assert_eq!(events[1]["content"], "third");
    }

    #[tokio::test]
    async fn test_session_storage_count_events() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        let session = Session::new(SessionType::Chat, tmp.path().to_path_buf());
        storage.save(&session).await.unwrap();

        // Empty initially
        let count = storage.count_events(&session.id, tmp.path()).await.unwrap();
        assert_eq!(count, 0);

        // Append events
        storage
            .append_event(&session, r#"{"type":"text"}"#)
            .await
            .unwrap();
        storage
            .append_event(&session, r#"{"type":"text"}"#)
            .await
            .unwrap();

        let count = storage.count_events(&session.id, tmp.path()).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_session_storage_load_events_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        // Load events for session with no JSONL file
        let events = storage
            .load_events("nonexistent", tmp.path(), None, None)
            .await
            .unwrap();
        assert!(events.is_empty());

        let count = storage
            .count_events("nonexistent", tmp.path())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_session_storage_load_events_with_malformed_json() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        let session = Session::new(SessionType::Chat, tmp.path().to_path_buf());
        storage.save(&session).await.unwrap();

        // Get the JSONL path
        let jsonl_path = tmp
            .path()
            .join(".crucible")
            .join("sessions")
            .join(&session.id)
            .join("session.jsonl");

        // Write a mix of valid and malformed JSON lines directly
        let content = r#"{"type":"text","content":"valid1"}
{invalid json here
{"type":"text","content":"valid2"}
not json at all
{"type":"text","content":"valid3"}
{"unclosed": "brace"
"#;
        tokio::fs::write(&jsonl_path, content).await.unwrap();

        // Load events - should skip malformed lines and return only valid ones
        let events = storage
            .load_events(&session.id, tmp.path(), None, None)
            .await
            .unwrap();

        // Should have 3 valid events (the malformed lines are skipped with warning)
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["content"], "valid1");
        assert_eq!(events[1]["content"], "valid2");
        assert_eq!(events[2]["content"], "valid3");
    }

    #[tokio::test]
    async fn test_crucible_home_avoids_double_nesting() {
        // When kiln IS crucible_home, sessions go to {kiln}/sessions/ (no .crucible prefix)
        let home = crucible_config::crucible_home();
        let base = FileSessionStorage::sessions_base(&home);
        // Should be {home}/sessions, NOT {home}/.crucible/sessions
        assert_eq!(base, home.join("sessions"));
        assert!(
            !base.to_string_lossy().contains(".crucible/.crucible"),
            "Should not double-nest .crucible: {:?}",
            base
        );
    }

    #[tokio::test]
    async fn test_regular_kiln_uses_crucible_prefix() {
        // When kiln is NOT crucible_home, sessions go to {kiln}/.crucible/sessions/
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("my-notes");
        std::fs::create_dir_all(&kiln).unwrap();

        let base = FileSessionStorage::sessions_base(&kiln);
        assert_eq!(base, kiln.join(".crucible").join("sessions"));
    }

    #[tokio::test]
    async fn test_session_storage_save_load_with_crucible_home() {
        // Use the real crucible_home path for this test
        let home = crucible_config::crucible_home();
        let storage = FileSessionStorage::new();

        let session = Session::new(SessionType::Chat, home.clone());
        let session_id = session.id.clone();

        storage.save(&session).await.unwrap();

        // Verify file is at {home}/sessions/{id}/meta.json (no .crucible prefix)
        let meta_path = home
            .join("sessions")
            .join(&session_id)
            .join("meta.json");
        assert!(meta_path.exists(), "meta.json should be at {:?}", meta_path);

        // Verify the double-nested path does NOT exist
        let bad_path = home
            .join(".crucible")
            .join("sessions")
            .join(&session_id);
        assert!(!bad_path.exists(), "should NOT have double .crucible nesting");

        // Load should work
        let loaded = storage.load(&session_id, &home).await.unwrap();
        assert_eq!(loaded.id, session_id);

        // Cleanup: remove the test session dir
        let _ = tokio::fs::remove_dir_all(home.join("sessions").join(&session_id)).await;
    }

    #[tokio::test]
    async fn test_session_storage_load_events_all_malformed() {
        let tmp = TempDir::new().unwrap();
        let storage = FileSessionStorage::new();

        let session = Session::new(SessionType::Chat, tmp.path().to_path_buf());
        storage.save(&session).await.unwrap();

        // Get the JSONL path
        let jsonl_path = tmp
            .path()
            .join(".crucible")
            .join("sessions")
            .join(&session.id)
            .join("session.jsonl");

        // Write only malformed JSON
        let content = r#"{invalid json
not json at all
{"unclosed": "brace"
"#;
        tokio::fs::write(&jsonl_path, content).await.unwrap();

        // Load events - should return empty vec when all lines are malformed
        let events = storage
            .load_events(&session.id, tmp.path(), None, None)
            .await
            .unwrap();

        assert!(events.is_empty());
    }
}
