//! Session state types for persistence
//!
//! This module defines the data structures for serializing agent sessions.
//! Sessions are stored as:
//! - Markdown files (human-readable summaries)
//! - JSON files (full state with messages and tasks)
//! - Index file (session directory)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Full session state (serialized to JSON)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    /// Session metadata
    pub metadata: SessionMetadata,
    /// Conversation messages
    pub messages: Vec<SessionMessage>,
    /// Task list
    pub tasks: Vec<Task>,
}

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMetadata {
    /// Workspace name/path
    pub workspace: String,
    /// Session start time
    pub started: DateTime<Utc>,
    /// Session end time (None if still open)
    pub ended: Option<DateTime<Utc>>,
    /// ID of session this continues from
    pub continued_from: Option<String>,
}

/// A message in the session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMessage {
    /// Message role
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Message timestamp
    pub timestamp: DateTime<Utc>,
    /// Tool name (for tool call messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Tool arguments (for tool call messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_args: Option<serde_json::Value>,
    /// Tool result (for tool response messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<serde_json::Value>,
}

/// Message role
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// User message
    User,
    /// Assistant message
    Assistant,
    /// Tool call/response
    Tool,
}

/// A task in the session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    /// Task description
    pub content: String,
    /// Current status
    pub status: TaskStatus,
}

/// Task status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Not yet started
    Pending,
    /// Currently being worked on
    InProgress,
    /// Finished
    Completed,
}

/// Entry in the session index
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionEntry {
    /// Unique session ID
    pub id: String,
    /// Workspace name/path
    pub workspace: String,
    /// Path to markdown file
    pub md_path: String,
    /// Session start time
    pub started: DateTime<Utc>,
    /// Session end time (None if still open)
    pub ended: Option<DateTime<Utc>>,
    /// ID of session this continued to
    pub continued_as: Option<String>,
}

/// Index of all sessions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SessionIndex {
    /// List of session entries
    pub sessions: Vec<SessionEntry>,
}

impl SessionIndex {
    /// Create a new empty index
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a session entry to the index
    pub fn add_entry(&mut self, entry: SessionEntry) {
        self.sessions.push(entry);
    }

    /// Find the latest open session for a workspace
    ///
    /// Returns the most recent session that:
    /// - Matches the workspace
    /// - Has no end time (still open)
    /// - Has not been continued (continued_as is None)
    pub fn find_latest_open(&self, workspace: &str) -> Option<&SessionEntry> {
        self.sessions
            .iter()
            .filter(|entry| {
                entry.workspace == workspace
                    && entry.ended.is_none()
                    && entry.continued_as.is_none()
            })
            .max_by_key(|entry| entry.started)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn create_test_timestamp(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    #[test]
    fn test_session_state_serialization() {
        // Create a session state
        let state = SessionState {
            metadata: SessionMetadata {
                workspace: "test-workspace".to_string(),
                started: create_test_timestamp(1000),
                ended: None,
                continued_from: None,
            },
            messages: vec![
                SessionMessage {
                    role: MessageRole::User,
                    content: "Hello".to_string(),
                    timestamp: create_test_timestamp(1001),
                    tool_name: None,
                    tool_args: None,
                    tool_result: None,
                },
                SessionMessage {
                    role: MessageRole::Assistant,
                    content: "Hi there!".to_string(),
                    timestamp: create_test_timestamp(1002),
                    tool_name: None,
                    tool_args: None,
                    tool_result: None,
                },
            ],
            tasks: vec![
                Task {
                    content: "Implement feature X".to_string(),
                    status: TaskStatus::InProgress,
                },
                Task {
                    content: "Write tests".to_string(),
                    status: TaskStatus::Pending,
                },
            ],
        };

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&state).expect("serialization failed");

        // Deserialize back
        let deserialized: SessionState =
            serde_json::from_str(&json).expect("deserialization failed");

        // Should be identical
        assert_eq!(state, deserialized);

        // Check JSON structure
        assert!(json.contains("\"workspace\": \"test-workspace\""));
        assert!(json.contains("\"role\": \"user\""));
        assert!(json.contains("\"role\": \"assistant\""));
        assert!(json.contains("\"status\": \"in_progress\""));
        assert!(json.contains("\"status\": \"pending\""));
    }

    #[test]
    fn test_session_message_with_tool_call() {
        // Tool call message
        let message = SessionMessage {
            role: MessageRole::Tool,
            content: "Search for Rust documentation".to_string(),
            timestamp: create_test_timestamp(2000),
            tool_name: Some("web_search".to_string()),
            tool_args: Some(serde_json::json!({"query": "Rust traits"})),
            tool_result: Some(serde_json::json!({"results": ["link1", "link2"]})),
        };

        // Serialize and deserialize
        let json = serde_json::to_string(&message).expect("serialization failed");
        let deserialized: SessionMessage =
            serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(message, deserialized);
        assert_eq!(deserialized.tool_name, Some("web_search".to_string()));
    }

    #[test]
    fn test_session_index_add_entry() {
        let mut index = SessionIndex::new();
        assert_eq!(index.sessions.len(), 0);

        let entry = SessionEntry {
            id: "session-001".to_string(),
            workspace: "my-workspace".to_string(),
            md_path: "/path/to/session-001.md".to_string(),
            started: create_test_timestamp(3000),
            ended: None,
            continued_as: None,
        };

        index.add_entry(entry.clone());
        assert_eq!(index.sessions.len(), 1);
        assert_eq!(index.sessions[0], entry);
    }

    #[test]
    fn test_find_latest_open_session() {
        let mut index = SessionIndex::new();

        // Add multiple sessions for different workspaces
        index.add_entry(SessionEntry {
            id: "session-001".to_string(),
            workspace: "workspace-a".to_string(),
            md_path: "/path/to/session-001.md".to_string(),
            started: create_test_timestamp(1000),
            ended: None,
            continued_as: None,
        });

        index.add_entry(SessionEntry {
            id: "session-002".to_string(),
            workspace: "workspace-b".to_string(),
            md_path: "/path/to/session-002.md".to_string(),
            started: create_test_timestamp(2000),
            ended: None,
            continued_as: None,
        });

        // Add an older open session for workspace-a
        index.add_entry(SessionEntry {
            id: "session-003".to_string(),
            workspace: "workspace-a".to_string(),
            md_path: "/path/to/session-003.md".to_string(),
            started: create_test_timestamp(3000), // More recent
            ended: None,
            continued_as: None,
        });

        // Add a closed session for workspace-a
        index.add_entry(SessionEntry {
            id: "session-004".to_string(),
            workspace: "workspace-a".to_string(),
            md_path: "/path/to/session-004.md".to_string(),
            started: create_test_timestamp(4000), // Most recent, but closed
            ended: Some(create_test_timestamp(5000)),
            continued_as: None,
        });

        // Add a continued session for workspace-a
        index.add_entry(SessionEntry {
            id: "session-005".to_string(),
            workspace: "workspace-a".to_string(),
            md_path: "/path/to/session-005.md".to_string(),
            started: create_test_timestamp(3500), // Recent but continued
            ended: None,
            continued_as: Some("session-006".to_string()),
        });

        // Find latest open session for workspace-a
        let latest = index.find_latest_open("workspace-a");
        assert!(latest.is_some());
        let latest = latest.unwrap();
        assert_eq!(latest.id, "session-003"); // Most recent open, not continued

        // Find latest open session for workspace-b
        let latest_b = index.find_latest_open("workspace-b");
        assert!(latest_b.is_some());
        assert_eq!(latest_b.unwrap().id, "session-002");

        // Non-existent workspace
        let none = index.find_latest_open("workspace-c");
        assert!(none.is_none());
    }

    #[test]
    fn test_session_index_serialization() {
        let mut index = SessionIndex::new();
        index.add_entry(SessionEntry {
            id: "session-001".to_string(),
            workspace: "test".to_string(),
            md_path: "/path/to/session.md".to_string(),
            started: create_test_timestamp(6000),
            ended: None,
            continued_as: None,
        });

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&index).expect("serialization failed");

        // Deserialize back
        let deserialized: SessionIndex =
            serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(index, deserialized);
    }

    #[test]
    fn test_task_status_values() {
        // Verify TaskStatus can be created and compared
        let pending = TaskStatus::Pending;
        let in_progress = TaskStatus::InProgress;
        let completed = TaskStatus::Completed;

        assert_ne!(pending, in_progress);
        assert_ne!(in_progress, completed);

        // Verify serialization format
        let task = Task {
            content: "Test task".to_string(),
            status: TaskStatus::InProgress,
        };
        let json = serde_json::to_string(&task).expect("serialization failed");
        assert!(json.contains("\"status\":\"in_progress\""));
    }

    #[test]
    fn test_message_role_values() {
        // Verify MessageRole can be created and compared
        let user = MessageRole::User;
        let assistant = MessageRole::Assistant;
        let tool = MessageRole::Tool;

        assert_ne!(user, assistant);
        assert_ne!(assistant, tool);

        // Verify serialization format
        let msg = SessionMessage {
            role: MessageRole::User,
            content: "Test".to_string(),
            timestamp: create_test_timestamp(7000),
            tool_name: None,
            tool_args: None,
            tool_result: None,
        };
        let json = serde_json::to_string(&msg).expect("serialization failed");
        assert!(json.contains("\"role\":\"user\""));
    }
}
