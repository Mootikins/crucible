//! SQLite session index for fast queries
//!
//! This module provides a SQLite-based index for session metadata.
//! The actual session content lives in JSONL files; this index enables
//! fast listing, searching, and filtering.

#[cfg(feature = "sqlite")]
mod sqlite_impl {
    use crate::id::{SessionId, SessionType};
    use crate::session::{SessionError, SessionMetadata};
    use chrono::DateTime;
    use rusqlite::{params, Connection, OptionalExtension};
    use std::path::Path;

    /// Parse a row tuple into SessionMetadata, returning None if parsing fails
    fn parse_session_row(
        row: (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            u32,
            String,
        ),
    ) -> Option<SessionMetadata> {
        let (id_str, type_str, started_at_str, ended_at_str, title, message_count, kiln_path_str) =
            row;

        let id = SessionId::parse(&id_str).ok()?;
        let session_type: SessionType = type_str.parse().ok()?;
        let started_at = DateTime::parse_from_rfc3339(&started_at_str).ok()?.to_utc();
        let ended_at = ended_at_str
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.to_utc());

        Some(SessionMetadata {
            id,
            session_type,
            started_at,
            ended_at,
            title,
            message_count,
            kiln_path: kiln_path_str.into(),
        })
    }

    /// Session index backed by SQLite
    pub struct SessionIndex {
        conn: Connection,
    }

    impl SessionIndex {
        /// Open or create a session index at the given path
        pub fn open(db_path: impl AsRef<Path>) -> Result<Self, SessionError> {
            let conn = Connection::open(db_path.as_ref())
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

            let index = Self { conn };
            index.init_schema()?;

            Ok(index)
        }

        /// Open an in-memory index (for testing)
        pub fn open_memory() -> Result<Self, SessionError> {
            let conn = Connection::open_in_memory()
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

            let index = Self { conn };
            index.init_schema()?;

            Ok(index)
        }

        fn init_schema(&self) -> Result<(), SessionError> {
            self.conn
                .execute_batch(
                    r#"
                    CREATE TABLE IF NOT EXISTS sessions (
                        id TEXT PRIMARY KEY,
                        type TEXT NOT NULL,
                        started_at TEXT NOT NULL,
                        ended_at TEXT,
                        title TEXT,
                        message_count INTEGER DEFAULT 0,
                        kiln_path TEXT NOT NULL
                    );
                    CREATE INDEX IF NOT EXISTS sessions_started_idx ON sessions(started_at DESC);
                    CREATE INDEX IF NOT EXISTS sessions_kiln_idx ON sessions(kiln_path);
                    CREATE INDEX IF NOT EXISTS sessions_type_idx ON sessions(type);
                "#,
                )
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

            Ok(())
        }

        /// Insert a new session into the index
        pub fn insert(&self, meta: &SessionMetadata) -> Result<(), SessionError> {
            self.conn
                .execute(
                    r#"
                    INSERT INTO sessions (id, type, started_at, ended_at, title, message_count, kiln_path)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                    params![
                        meta.id.as_str(),
                        meta.session_type.to_string(),
                        meta.started_at.to_rfc3339(),
                        meta.ended_at.map(|t| t.to_rfc3339()),
                        meta.title,
                        meta.message_count,
                        meta.kiln_path.display().to_string(),
                    ],
                )
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

            Ok(())
        }

        /// Update session metadata
        pub fn update(&self, meta: &SessionMetadata) -> Result<(), SessionError> {
            self.conn
                .execute(
                    r#"
                    UPDATE sessions
                    SET ended_at = ?1, title = ?2, message_count = ?3
                    WHERE id = ?4
                "#,
                    params![
                        meta.ended_at.map(|t| t.to_rfc3339()),
                        meta.title,
                        meta.message_count,
                        meta.id.as_str(),
                    ],
                )
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

            Ok(())
        }

        /// Get session metadata by ID
        pub fn get(&self, id: &SessionId) -> Result<Option<SessionMetadata>, SessionError> {
            let result = self
                .conn
                .query_row(
                    "SELECT id, type, started_at, ended_at, title, message_count, kiln_path FROM sessions WHERE id = ?1",
                    params![id.as_str()],
                    |row| {
                        let id_str: String = row.get(0)?;
                        let type_str: String = row.get(1)?;
                        let started_at_str: String = row.get(2)?;
                        let ended_at_str: Option<String> = row.get(3)?;
                        let title: Option<String> = row.get(4)?;
                        let message_count: u32 = row.get(5)?;
                        let kiln_path_str: String = row.get(6)?;

                        Ok((id_str, type_str, started_at_str, ended_at_str, title, message_count, kiln_path_str))
                    },
                )
                .optional()
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

            if let Some((
                id_str,
                type_str,
                started_at_str,
                ended_at_str,
                title,
                message_count,
                kiln_path_str,
            )) = result
            {
                let id = SessionId::parse(&id_str)
                    .map_err(|e| SessionError::Io(std::io::Error::other(e.to_string())))?;
                let session_type: SessionType = type_str
                    .parse()
                    .map_err(|e: String| SessionError::Io(std::io::Error::other(e)))?;
                let started_at = DateTime::parse_from_rfc3339(&started_at_str)
                    .map_err(|e| SessionError::Io(std::io::Error::other(e)))?
                    .to_utc();
                let ended_at = ended_at_str
                    .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.to_utc()))
                    .transpose()
                    .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

                Ok(Some(SessionMetadata {
                    id,
                    session_type,
                    started_at,
                    ended_at,
                    title,
                    message_count,
                    kiln_path: kiln_path_str.into(),
                }))
            } else {
                Ok(None)
            }
        }

        /// Delete a session from the index
        pub fn delete(&self, id: &SessionId) -> Result<bool, SessionError> {
            let rows = self
                .conn
                .execute("DELETE FROM sessions WHERE id = ?1", params![id.as_str()])
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

            Ok(rows > 0)
        }

        /// List sessions, newest first
        pub fn list(&self, limit: Option<u32>) -> Result<Vec<SessionMetadata>, SessionError> {
            // Use parameterized query for LIMIT to prevent SQL injection
            let (sql, use_limit) = if limit.is_some() {
                (
                    "SELECT id, type, started_at, ended_at, title, message_count, kiln_path FROM sessions ORDER BY started_at DESC LIMIT ?1",
                    true,
                )
            } else {
                (
                    "SELECT id, type, started_at, ended_at, title, message_count, kiln_path FROM sessions ORDER BY started_at DESC",
                    false,
                )
            };

            let mut stmt = self
                .conn
                .prepare(sql)
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

            let row_mapper = |row: &rusqlite::Row| -> rusqlite::Result<_> {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, String>(6)?,
                ))
            };

            let sessions: Vec<SessionMetadata> = if use_limit {
                stmt.query_map(params![limit.unwrap()], row_mapper)
                    .map_err(|e| SessionError::Io(std::io::Error::other(e)))?
                    .filter_map(|r| r.ok())
                    .filter_map(parse_session_row)
                    .collect()
            } else {
                stmt.query_map([], row_mapper)
                    .map_err(|e| SessionError::Io(std::io::Error::other(e)))?
                    .filter_map(|r| r.ok())
                    .filter_map(parse_session_row)
                    .collect()
            };

            Ok(sessions)
        }

        /// List sessions by type
        pub fn list_by_type(
            &self,
            session_type: SessionType,
            limit: Option<u32>,
        ) -> Result<Vec<SessionMetadata>, SessionError> {
            // Use parameterized query for LIMIT to prevent SQL injection
            let (sql, use_limit) = if limit.is_some() {
                (
                    "SELECT id, type, started_at, ended_at, title, message_count, kiln_path FROM sessions WHERE type = ?1 ORDER BY started_at DESC LIMIT ?2",
                    true,
                )
            } else {
                (
                    "SELECT id, type, started_at, ended_at, title, message_count, kiln_path FROM sessions WHERE type = ?1 ORDER BY started_at DESC",
                    false,
                )
            };

            let mut stmt = self
                .conn
                .prepare(sql)
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

            let row_mapper = |row: &rusqlite::Row| -> rusqlite::Result<_> {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, String>(6)?,
                ))
            };

            let sessions: Vec<SessionMetadata> = if use_limit {
                stmt.query_map(
                    params![session_type.to_string(), limit.unwrap()],
                    row_mapper,
                )
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?
                .filter_map(|r| r.ok())
                .filter_map(parse_session_row)
                .collect()
            } else {
                stmt.query_map(params![session_type.to_string()], row_mapper)
                    .map_err(|e| SessionError::Io(std::io::Error::other(e)))?
                    .filter_map(|r| r.ok())
                    .filter_map(parse_session_row)
                    .collect()
            };

            Ok(sessions)
        }

        /// Search sessions by title
        pub fn search_by_title(
            &self,
            query: &str,
            limit: Option<u32>,
        ) -> Result<Vec<SessionMetadata>, SessionError> {
            let pattern = format!("%{query}%");
            // Use parameterized query for LIMIT to prevent SQL injection
            let (sql, use_limit) = if limit.is_some() {
                (
                    "SELECT id, type, started_at, ended_at, title, message_count, kiln_path FROM sessions WHERE title LIKE ?1 ORDER BY started_at DESC LIMIT ?2",
                    true,
                )
            } else {
                (
                    "SELECT id, type, started_at, ended_at, title, message_count, kiln_path FROM sessions WHERE title LIKE ?1 ORDER BY started_at DESC",
                    false,
                )
            };

            let mut stmt = self
                .conn
                .prepare(sql)
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

            let row_mapper = |row: &rusqlite::Row| -> rusqlite::Result<_> {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, String>(6)?,
                ))
            };

            let sessions: Vec<SessionMetadata> = if use_limit {
                stmt.query_map(params![pattern, limit.unwrap()], row_mapper)
                    .map_err(|e| SessionError::Io(std::io::Error::other(e)))?
                    .filter_map(|r| r.ok())
                    .filter_map(parse_session_row)
                    .collect()
            } else {
                stmt.query_map(params![pattern], row_mapper)
                    .map_err(|e| SessionError::Io(std::io::Error::other(e)))?
                    .filter_map(|r| r.ok())
                    .filter_map(parse_session_row)
                    .collect()
            };

            Ok(sessions)
        }

        /// Count total sessions
        pub fn count(&self) -> Result<u32, SessionError> {
            let count: u32 = self
                .conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
                .map_err(|e| SessionError::Io(std::io::Error::other(e)))?;

            Ok(count)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use chrono::Utc;
        use std::path::PathBuf;

        fn create_test_meta(id: &str, title: Option<&str>) -> SessionMetadata {
            SessionMetadata {
                id: SessionId::parse(id).unwrap(),
                session_type: SessionType::Chat,
                started_at: Utc::now(),
                ended_at: None,
                title: title.map(String::from),
                message_count: 0,
                kiln_path: PathBuf::from("/test/kiln"),
            }
        }

        #[test]
        fn test_open_memory() {
            let index = SessionIndex::open_memory().unwrap();
            assert_eq!(index.count().unwrap(), 0);
        }

        #[test]
        fn test_insert_and_get() {
            let index = SessionIndex::open_memory().unwrap();
            let meta = create_test_meta("chat-20260104-1530-a1b2", Some("Test Session"));

            index.insert(&meta).unwrap();

            let retrieved = index.get(&meta.id).unwrap().unwrap();
            assert_eq!(retrieved.id, meta.id);
            assert_eq!(retrieved.title, Some("Test Session".to_string()));
        }

        #[test]
        fn test_update() {
            let index = SessionIndex::open_memory().unwrap();
            let mut meta = create_test_meta("chat-20260104-1530-a1b2", None);

            index.insert(&meta).unwrap();

            meta.title = Some("Updated Title".to_string());
            meta.message_count = 10;
            meta.ended_at = Some(Utc::now());

            index.update(&meta).unwrap();

            let retrieved = index.get(&meta.id).unwrap().unwrap();
            assert_eq!(retrieved.title, Some("Updated Title".to_string()));
            assert_eq!(retrieved.message_count, 10);
            assert!(retrieved.ended_at.is_some());
        }

        #[test]
        fn test_delete() {
            let index = SessionIndex::open_memory().unwrap();
            let meta = create_test_meta("chat-20260104-1530-a1b2", None);

            index.insert(&meta).unwrap();
            assert!(index.delete(&meta.id).unwrap());
            assert!(index.get(&meta.id).unwrap().is_none());
        }

        #[test]
        fn test_list() {
            let index = SessionIndex::open_memory().unwrap();

            index
                .insert(&create_test_meta("chat-20260104-1530-a1b2", None))
                .unwrap();
            index
                .insert(&create_test_meta("chat-20260104-1531-b2c3", None))
                .unwrap();
            index
                .insert(&create_test_meta("chat-20260104-1532-c3d4", None))
                .unwrap();

            let sessions = index.list(None).unwrap();
            assert_eq!(sessions.len(), 3);

            let sessions = index.list(Some(2)).unwrap();
            assert_eq!(sessions.len(), 2);
        }

        #[test]
        fn test_list_by_type() {
            let index = SessionIndex::open_memory().unwrap();

            index
                .insert(&create_test_meta("chat-20260104-1530-a1b2", None))
                .unwrap();

            let mut workflow_meta = create_test_meta("workflow-20260104-1531-b2c3", None);
            workflow_meta.session_type = SessionType::Workflow;
            workflow_meta.id = SessionId::parse("workflow-20260104-1531-b2c3").unwrap();
            index.insert(&workflow_meta).unwrap();

            let chats = index.list_by_type(SessionType::Chat, None).unwrap();
            assert_eq!(chats.len(), 1);

            let workflows = index.list_by_type(SessionType::Workflow, None).unwrap();
            assert_eq!(workflows.len(), 1);
        }

        #[test]
        fn test_search_by_title() {
            let index = SessionIndex::open_memory().unwrap();

            index
                .insert(&create_test_meta(
                    "chat-20260104-1530-a1b2",
                    Some("Debugging rust code"),
                ))
                .unwrap();
            index
                .insert(&create_test_meta(
                    "chat-20260104-1531-b2c3",
                    Some("Writing python"),
                ))
                .unwrap();
            index
                .insert(&create_test_meta(
                    "chat-20260104-1532-c3d4",
                    Some("More rust work"),
                ))
                .unwrap();

            let results = index.search_by_title("rust", None).unwrap();
            assert_eq!(results.len(), 2);

            let results = index.search_by_title("python", None).unwrap();
            assert_eq!(results.len(), 1);

            let results = index.search_by_title("javascript", None).unwrap();
            assert!(results.is_empty());
        }

        #[test]
        fn test_count() {
            let index = SessionIndex::open_memory().unwrap();

            assert_eq!(index.count().unwrap(), 0);

            index
                .insert(&create_test_meta("chat-20260104-1530-a1b2", None))
                .unwrap();
            index
                .insert(&create_test_meta("chat-20260104-1531-b2c3", None))
                .unwrap();

            assert_eq!(index.count().unwrap(), 2);
        }
    }
}

#[cfg(feature = "sqlite")]
pub use sqlite_impl::SessionIndex;
