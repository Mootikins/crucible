// Event types for the TUI
//
// All state mutations in the TUI are triggered by events. This module defines
// the event types that flow through the system.

use chrono::{DateTime, Utc};
use crossterm::event::Event as CrosstermEvent;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Events that trigger UI updates
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// User input from terminal (keyboard, mouse, resize)
    Input(CrosstermEvent),

    /// Log entry from worker threads
    Log(LogEntry),

    /// Status update (doc count, DB size, etc.)
    Status(StatusUpdate),

    /// REPL command execution result
    ReplResult(ReplResult),

    /// Graceful shutdown request
    Shutdown,
}

/// Structured log entry
///
/// Represents a single log event from the tracing infrastructure.
/// These are collected in a ring buffer for display in the log window.
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// When the log occurred
    pub timestamp: DateTime<Utc>,

    /// Log level (ERROR, WARN, INFO, DEBUG, TRACE)
    pub level: tracing::Level,

    /// Module that generated the log (e.g., "crucible_watch::parser")
    pub target: String,

    /// Log message
    pub message: String,

    /// Structured fields (key-value pairs from tracing spans)
    pub fields: HashMap<String, String>,
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(
        level: tracing::Level,
        target: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            level,
            target: target.into(),
            message: message.into(),
            fields: HashMap::new(),
        }
    }

    /// Add a structured field
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Format as a single line for display
    pub fn format(&self) -> String {
        let timestamp = self.timestamp.format("%H:%M:%S");
        format!("{} {:<5} {}", timestamp, self.level, self.message)
    }
}

/// Status bar information
///
/// Partial updates are supported - None values mean "don't change".
#[derive(Debug, Clone, Default)]
pub struct StatusUpdate {
    /// Kiln root path
    pub kiln_path: Option<PathBuf>,

    /// Database backend type (e.g., "SurrealDB")
    pub db_type: Option<String>,

    /// Total document count
    pub doc_count: Option<u64>,

    /// Database size in bytes
    pub db_size: Option<u64>,
}

impl StatusUpdate {
    /// Create an empty update
    pub fn new() -> Self {
        Self::default()
    }

    /// Set kiln path
    pub fn with_kiln_path(mut self, path: PathBuf) -> Self {
        self.kiln_path = Some(path);
        self
    }

    /// Set DB type
    pub fn with_db_type(mut self, db_type: impl Into<String>) -> Self {
        self.db_type = Some(db_type.into());
        self
    }

    /// Set document count
    pub fn with_doc_count(mut self, count: u64) -> Self {
        self.doc_count = Some(count);
        self
    }

    /// Set database size
    pub fn with_db_size(mut self, size: u64) -> Self {
        self.db_size = Some(size);
        self
    }
}

/// REPL command execution result
#[derive(Debug, Clone)]
pub enum ReplResult {
    /// Successful execution with output
    Success {
        /// Output text
        output: String,
        /// Execution duration
        duration: Duration,
    },

    /// Error during execution
    Error {
        /// Error message
        message: String,
    },

    /// Tabular result (e.g., from SQL query)
    Table {
        /// Column headers
        headers: Vec<String>,
        /// Data rows
        rows: Vec<Vec<String>>,
    },
}

impl ReplResult {
    /// Create a success result
    pub fn success(output: impl Into<String>, duration: Duration) -> Self {
        Self::Success {
            output: output.into(),
            duration,
        }
    }

    /// Create an error result
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }

    /// Create a table result
    pub fn table(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self::Table { headers, rows }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry::new(tracing::Level::INFO, "test_module", "Test message")
            .with_field("file", "test.md")
            .with_field("duration_ms", "42");

        assert_eq!(entry.level, tracing::Level::INFO);
        assert_eq!(entry.target, "test_module");
        assert_eq!(entry.message, "Test message");
        assert_eq!(entry.fields.get("file"), Some(&"test.md".to_string()));
    }

    #[test]
    fn test_status_update_builder() {
        let update = StatusUpdate::new()
            .with_kiln_path(PathBuf::from("/kiln"))
            .with_db_type("SurrealDB")
            .with_doc_count(42);

        assert!(update.kiln_path.is_some());
        assert_eq!(update.db_type, Some("SurrealDB".to_string()));
        assert_eq!(update.doc_count, Some(42));
        assert!(update.db_size.is_none()); // Partial update
    }

    #[test]
    fn test_repl_result_types() {
        let success = ReplResult::success("OK", Duration::from_millis(100));
        assert!(matches!(success, ReplResult::Success { .. }));

        let error = ReplResult::error("Syntax error");
        assert!(matches!(error, ReplResult::Error { .. }));

        let table = ReplResult::table(vec!["id".to_string()], vec![vec!["1".to_string()]]);
        assert!(matches!(table, ReplResult::Table { .. }));
    }
}
