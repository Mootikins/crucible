//! Session persistence to the daemon-owned sessions root.
//!
//! Sessions are stored one directory per id at `{sessions_root}/{session_id}/`,
//! where `sessions_root` is `{data_home}/sessions` — resolved once at daemon
//! bind and threaded in as a value, never read from the process-global
//! `crucible_home()`.
//!
//! Contents:
//! - `meta.json` - Session metadata
//! - `session.jsonl` - Event log (append-only)
//! - `session.md` - Human-readable markdown conversation log
//!
//! Sessions used to live inside their owning kiln
//! (`{kiln}/.crucible/sessions/{id}`), which welded a filing decision to a
//! knowledge decision and shipped every conversation along with a shared kiln.
//! [`crate::session_migration`] relocates those on daemon start.

use crate::session_manager::SessionError;
use async_trait::async_trait;
use chrono::Utc;
use crucible_core::session::{Session, SessionId, SessionSummary};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Trait for session persistence.
///
/// Implementations provide different storage backends for persisting
/// sessions to disk or other storage systems.
#[async_trait]
pub trait SessionStorage: Send + Sync {
    /// Root directory holding one subdirectory per session.
    ///
    /// On the trait rather than on the file backend because callers that need
    /// a session's directory for something this trait does not do (workflow
    /// snapshots, review journals, recording files) used to reach around it to
    /// a `FileSessionStorage` associated function, which is what let the layout
    /// be decided in half a dozen places at once.
    fn sessions_root(&self) -> &Path;

    /// Save a session to storage.
    ///
    /// Creates the session directory if needed and writes session metadata.
    async fn save(&self, session: &Session) -> Result<(), SessionError>;

    /// Load a session from storage.
    ///
    /// Returns `SessionError::NotFound` if the session doesn't exist.
    async fn load(&self, session_id: &SessionId) -> Result<Session, SessionError>;

    /// List every persisted session.
    ///
    /// Returns an empty vec if the sessions root doesn't exist.
    async fn list(&self) -> Result<Vec<SessionSummary>, SessionError>;

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
        session_id: &SessionId,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError>;

    /// Count total events in the session's JSONL log.
    async fn count_events(&self, session_id: &SessionId) -> Result<usize, SessionError>;
}

/// File-based session storage rooted at a single directory.
#[derive(Debug, Clone)]
pub struct FileSessionStorage {
    sessions_root: PathBuf,
}

impl FileSessionStorage {
    /// Create file-based storage rooted at `sessions_root`.
    ///
    /// Use [`FileSessionStorage::root_for`] to derive it from the daemon's
    /// data root.
    pub fn new(sessions_root: PathBuf) -> Self {
        Self { sessions_root }
    }

    /// The sessions root for a daemon data root: `{data_home}/sessions`.
    ///
    /// The single place the layout is spelled out, so relocating it again is a
    /// one-line change rather than a grep.
    pub fn root_for(data_home: &Path) -> PathBuf {
        data_home.join("sessions")
    }

    /// Storage directory for a session id.
    ///
    /// Takes the validated id rather than a `&str`: the join is what turns an
    /// identifier into filesystem reach, and [`SessionId`] is the only type
    /// that has been through the check.
    fn session_dir(&self, session_id: &SessionId) -> PathBuf {
        session_id.dir_under(&self.sessions_root)
    }
}

#[async_trait]
impl SessionStorage for FileSessionStorage {
    fn sessions_root(&self) -> &Path {
        &self.sessions_root
    }

    async fn save(&self, session: &Session) -> Result<(), SessionError> {
        let dir = self.session_dir(&session.id);
        fs::create_dir_all(&dir).await?;

        // Save session metadata as JSON
        let meta_path = dir.join("meta.json");
        let json = serde_json::to_string_pretty(session)?;
        fs::write(&meta_path, json).await?;

        Ok(())
    }

    async fn load(&self, session_id: &SessionId) -> Result<Session, SessionError> {
        let dir = self.session_dir(session_id);
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

        let session: Session = serde_json::from_str(&json).map_err(|e| {
            SessionError::IoError(format!(
                "Failed to parse session '{}' JSON: {}",
                session_id, e
            ))
        })?;

        // Same rule `list` applies, at the other door. A load is keyed on the
        // directory; every subsequent write — `save`, `append_event`,
        // `append_markdown` — is keyed on `session.id`. Handing back a session
        // whose id names a *different* directory therefore turns reading one
        // session into writing another, which is the whole of the attack
        // migration's stamp closes on the way in. Enforced here as well because
        // migration is not the only way a `meta.json` can come to disagree with
        // the directory holding it, and this is the sink rather than one door.
        if session.id != *session_id {
            return Err(SessionError::IoError(format!(
                "Session '{}' holds metadata naming '{}'; refusing to load a session that names another session's directory",
                session_id, session.id
            )));
        }

        Ok(session)
    }

    async fn list(&self) -> Result<Vec<SessionSummary>, SessionError> {
        if !self.sessions_root.exists() {
            return Ok(vec![]);
        }

        let mut summaries = vec![];
        let mut entries = fs::read_dir(&self.sessions_root).await?;

        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            // Anything the daemon did not file is not a session: migration's
            // `.migrating` staging directory, an editor's dotfile, a name that
            // is not valid UTF-8.
            let Some(session_id) = entry
                .file_name()
                .to_str()
                .and_then(|name| SessionId::parse(name).ok())
            else {
                continue;
            };
            let Ok(session) = self.load(&session_id).await else {
                continue;
            };
            // The directory name IS the id. A `meta.json` claiming a different
            // one is either corrupt or planted, and serving it would hand every
            // caller downstream — `session.cleanup` most of all — a path that
            // points at a directory other than the one this summary describes.
            if session.id != session_id {
                tracing::warn!(
                    directory = %session_id,
                    persisted_id = %session.id,
                    "Skipping a session whose meta.json names a different id than its directory"
                );
                continue;
            }
            summaries.push(SessionSummary::from(&session));
        }

        Ok(summaries)
    }

    async fn append_event(&self, session: &Session, event: &str) -> Result<(), SessionError> {
        let dir = self.session_dir(&session.id);

        // Ensure directory exists
        fs::create_dir_all(&dir).await?;

        let jsonl_path = dir.join("session.jsonl");

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&jsonl_path)
            .await?;

        file.write_all(event.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;

        Ok(())
    }

    async fn append_markdown(
        &self,
        session: &Session,
        role: &str,
        content: &str,
    ) -> Result<(), SessionError> {
        let dir = self.session_dir(&session.id);

        // Ensure directory exists
        fs::create_dir_all(&dir).await?;

        let md_path = dir.join("session.md");

        // Create file with frontmatter if it doesn't exist
        if !md_path.exists() {
            let session_type_name = match session.session_type {
                crucible_core::session::SessionType::Chat => "Chat",
                crucible_core::session::SessionType::Agent => "Agent",
                crucible_core::session::SessionType::Workflow => "Workflow",
            };

            let kilns = session
                .kilns
                .iter()
                .map(|k| k.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");

            let frontmatter = format!(
                "---\nsession_id: {}\ntype: {}\nkilns: [{}]\nworkspace: {}\nstarted: {}\n---\n\n# {} Session\n\n",
                session.id,
                session.session_type.as_prefix(),
                kilns,
                session.workspace.display(),
                session.started_at.to_rfc3339(),
                session_type_name,
            );
            fs::write(&md_path, frontmatter).await?;
        }

        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let entry = format!("\n## {} - {}\n\n{}\n", role, timestamp, content);

        let mut file = fs::OpenOptions::new().append(true).open(&md_path).await?;

        file.write_all(entry.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    async fn load_events(
        &self,
        session_id: &SessionId,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError> {
        let jsonl_path = self.session_dir(session_id).join("session.jsonl");

        if !jsonl_path.exists() {
            return Ok(vec![]);
        }

        let content = fs::read_to_string(&jsonl_path).await?;

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

    async fn count_events(&self, session_id: &SessionId) -> Result<usize, SessionError> {
        let jsonl_path = self.session_dir(session_id).join("session.jsonl");

        if !jsonl_path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(&jsonl_path).await?;

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

    /// A storage rooted in a fresh TempDir, plus the kiln path test sessions
    /// are given. The two are deliberately unrelated: storage location no
    /// longer follows from the kiln.
    fn storage_in(tmp: &TempDir) -> FileSessionStorage {
        FileSessionStorage::new(FileSessionStorage::root_for(tmp.path()))
    }

    fn session_in(tmp: &TempDir, session_type: SessionType) -> Session {
        Session::new(session_type, vec![tmp.path().join("kiln")])
    }

    #[tokio::test]
    async fn test_session_storage_save_load() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        let session_id = session.id.clone();

        storage.save(&session).await.unwrap();

        let loaded = storage.load(&session_id).await.unwrap();
        assert_eq!(loaded.id, session_id);
        assert_eq!(loaded.session_type, SessionType::Chat);
    }

    /// A load is keyed on the directory and every write that follows is keyed
    /// on `session.id`, so a `meta.json` naming another session turns reading
    /// one directory into writing another. Migration stamps the id on the way
    /// in; this is the same rule at the sink, for metadata that came to
    /// disagree some other way.
    #[tokio::test]
    async fn loading_a_directory_whose_metadata_names_another_session_fails() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let victim = session_in(&tmp, SessionType::Chat);
        storage.save(&victim).await.unwrap();

        // A directory of its own, whose metadata claims to be the victim.
        let evil_id = SessionId::parse("chat-evil").unwrap();
        let mut evil = session_in(&tmp, SessionType::Chat);
        evil.id = victim.id.clone();
        let dir = evil_id.dir_under(&FileSessionStorage::root_for(tmp.path()));
        fs::create_dir_all(&dir).await.unwrap();
        fs::write(
            dir.join("meta.json"),
            serde_json::to_string_pretty(&evil).unwrap(),
        )
        .await
        .unwrap();

        assert!(
            storage.load(&evil_id).await.is_err(),
            "loading one session's directory handed back another session's id"
        );
    }

    #[tokio::test]
    async fn sessions_land_under_the_injected_root_not_the_kiln() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        let meta = tmp
            .path()
            .join("sessions")
            .join(&*session.id)
            .join("meta.json");
        assert!(meta.exists(), "meta.json should be at {}", meta.display());
        assert!(
            !tmp.path().join("kiln").join(".crucible").exists(),
            "nothing may be written inside the kiln"
        );
    }

    #[tokio::test]
    async fn test_session_storage_list() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        // Two sessions in different kilns still share one storage root.
        let session1 = Session::new(SessionType::Chat, vec![tmp.path().join("kiln-a")]);
        let session2 = Session::new(SessionType::Agent, vec![tmp.path().join("kiln-b")]);

        storage.save(&session1).await.unwrap();
        storage.save(&session2).await.unwrap();

        let summaries = storage.list().await.unwrap();
        assert_eq!(summaries.len(), 2);
    }

    #[tokio::test]
    async fn test_session_storage_append_event() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
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
        let jsonl_path = session.jsonl_path(storage.sessions_root());
        let content = tokio::fs::read_to_string(&jsonl_path).await.unwrap();
        assert!(content.contains("hello"));
        assert!(content.contains("world"));
        assert_eq!(content.lines().count(), 2);
    }

    #[tokio::test]
    async fn test_session_storage_append_event_includes_timestamp() {
        use crucible_core::protocol::SessionEventMessage;
        use serde_json::json;

        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        // Create an event with timestamp
        let event =
            SessionEventMessage::new(&session.id, "user_message", json!({"content": "hello"}))
                .with_timestamp();

        // Serialize and append
        let json_str = serde_json::to_string(&event).unwrap();
        storage.append_event(&session, &json_str).await.unwrap();

        // Read back and verify timestamp is present
        let jsonl_path = session.jsonl_path(storage.sessions_root());
        let content = tokio::fs::read_to_string(&jsonl_path).await.unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(content.lines().next().unwrap()).unwrap();

        // Verify timestamp field exists and is ISO8601 format
        assert!(
            parsed.get("timestamp").is_some(),
            "timestamp field missing from persisted event"
        );
        let timestamp_str = parsed.get("timestamp").unwrap().as_str().unwrap();
        // Basic ISO8601 validation: should contain T and Z
        assert!(
            timestamp_str.contains('T'),
            "timestamp not in ISO8601 format"
        );
        assert!(
            timestamp_str.ends_with('Z'),
            "timestamp should end with Z (UTC)"
        );
    }

    #[tokio::test]
    async fn test_session_storage_load_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let result = storage
            .load(&crate::test_support::sid("nonexistent-session"))
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_session_storage_list_empty() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let summaries = storage.list().await.unwrap();
        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn test_session_storage_append_event_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        // Create session but don't save it first
        let session = session_in(&tmp, SessionType::Chat);

        // append_event should create the directory if needed
        storage
            .append_event(&session, r#"{"type":"text","content":"test"}"#)
            .await
            .unwrap();

        // Verify the directory and file were created
        assert!(session.jsonl_path(storage.sessions_root()).exists());
    }

    #[tokio::test]
    async fn test_session_storage_preserves_all_fields() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let other_kiln = tmp.path().join("other-kiln");
        let workspace = tmp.path().join("workspace");

        let session = session_in(&tmp, SessionType::Agent)
            .with_workspace(workspace.clone())
            .with_kiln(other_kiln.clone())
            .with_title("Test Session");
        let session_id = session.id.clone();
        let kilns = session.kilns.clone();

        storage.save(&session).await.unwrap();

        let loaded = storage.load(&session_id).await.unwrap();
        assert_eq!(loaded.session_type, SessionType::Agent);
        assert_eq!(loaded.workspace, workspace);
        assert_eq!(loaded.kilns, kilns);
        assert!(loaded.kilns.contains(&other_kiln));
        assert_eq!(loaded.title, Some("Test Session".to_string()));
    }

    #[tokio::test]
    async fn test_session_storage_append_markdown() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
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
        let content = tokio::fs::read_to_string(session.log_path(storage.sessions_root()))
            .await
            .unwrap();

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
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Agent);
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

        let content = tokio::fs::read_to_string(session.log_path(storage.sessions_root()))
            .await
            .unwrap();

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
        let storage = storage_in(&tmp);

        // Create session but don't save it first
        let session = session_in(&tmp, SessionType::Workflow);

        // append_markdown should create the directory if needed
        storage
            .append_markdown(&session, "System", "Starting workflow")
            .await
            .unwrap();

        // Verify the directory and file were created
        let md_path = session.log_path(storage.sessions_root());
        assert!(md_path.exists());

        let content = tokio::fs::read_to_string(&md_path).await.unwrap();
        assert!(content.contains("type: workflow"));
        assert!(content.contains("# Workflow Session"));
        assert!(content.contains("Starting workflow"));
    }

    #[tokio::test]
    async fn test_session_storage_load_events() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
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
        let events = storage.load_events(&session.id, None, None).await.unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["content"], "first");
        assert_eq!(events[2]["content"], "third");

        // Load with pagination
        let events = storage
            .load_events(&session.id, Some(2), Some(1))
            .await
            .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["content"], "second");
        assert_eq!(events[1]["content"], "third");
    }

    #[tokio::test]
    async fn test_session_storage_count_events() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        // Empty initially
        let count = storage.count_events(&session.id).await.unwrap();
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

        let count = storage.count_events(&session.id).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_session_storage_load_events_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        // Load events for session with no JSONL file
        let events = storage
            .load_events(&crate::test_support::sid("nonexistent"), None, None)
            .await
            .unwrap();
        assert!(events.is_empty());

        let count = storage
            .count_events(&crate::test_support::sid("nonexistent"))
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_session_storage_load_events_with_malformed_json() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        // Write a mix of valid and malformed JSON lines directly
        let content = r#"{"type":"text","content":"valid1"}
{invalid json here
{"type":"text","content":"valid2"}
not json at all
{"type":"text","content":"valid3"}
{"unclosed": "brace"
"#;
        tokio::fs::write(session.jsonl_path(storage.sessions_root()), content)
            .await
            .unwrap();

        // Load events - should skip malformed lines and return only valid ones
        let events = storage.load_events(&session.id, None, None).await.unwrap();

        // Should have 3 valid events (the malformed lines are skipped with warning)
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["content"], "valid1");
        assert_eq!(events[1]["content"], "valid2");
        assert_eq!(events[2]["content"], "valid3");
    }

    #[tokio::test]
    async fn test_session_storage_load_events_all_malformed() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        // Write only malformed JSON
        let content = r#"{invalid json
not json at all
{"unclosed": "brace"
"#;
        tokio::fs::write(session.jsonl_path(storage.sessions_root()), content)
            .await
            .unwrap();

        // Load events - should return empty vec when all lines are malformed
        let events = storage.load_events(&session.id, None, None).await.unwrap();

        assert!(events.is_empty());
    }
}
