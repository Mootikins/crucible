//! Type definitions for task harness storage
//!
//! These types support persisting task state from TASKS.md files
//! to SurrealDB for querying, history tracking, and multi-agent coordination.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::schema_types::RecordId;
use crucible_core::parser::{CheckboxStatus, TaskItem};

// ============================================================================
// Task Record (4.1.1)
// ============================================================================

/// A task stored in the database
///
/// Maps to TaskItem from the parser but with DB-specific fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    /// Record ID (format: "tasks:file_id:task_id")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<TaskRecord>>,

    /// Task ID from inline metadata (e.g., "1.1.1", "task-a")
    pub task_id: String,

    /// Reference to parent TaskFileRecord
    pub task_file: RecordId<TaskFileRecord>,

    /// Task content/description (with metadata stripped)
    pub content: String,

    /// Current status
    pub status: String, // "pending", "done", "in_progress", "cancelled", "blocked"

    /// Dependency IDs (task_ids this depends on)
    #[serde(default)]
    pub deps: Vec<String>,

    /// Additional metadata from inline fields
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last status change timestamp
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

impl TaskRecord {
    /// Create a TaskRecord from a TaskItem
    pub fn from_task_item(item: &TaskItem, task_file_id: RecordId<TaskFileRecord>) -> Self {
        let status = match item.status {
            CheckboxStatus::Pending => "pending",
            CheckboxStatus::Done => "done",
            CheckboxStatus::InProgress => "in_progress",
            CheckboxStatus::Cancelled => "cancelled",
            CheckboxStatus::Blocked => "blocked",
        };

        let metadata: HashMap<String, serde_json::Value> = item
            .metadata
            .iter()
            .map(|(k, v)| {
                let value = if v.is_array() {
                    serde_json::json!(v.as_vec())
                } else {
                    serde_json::json!(v.as_string())
                };
                (k.clone(), value)
            })
            .collect();

        Self {
            id: None,
            task_id: item.id.clone(),
            task_file: task_file_id,
            content: item.content.clone(),
            status: status.to_string(),
            deps: item.deps.clone(),
            metadata,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Convert back to TaskItem
    pub fn to_task_item(&self) -> TaskItem {
        let status = match self.status.as_str() {
            "done" => CheckboxStatus::Done,
            "in_progress" => CheckboxStatus::InProgress,
            "cancelled" => CheckboxStatus::Cancelled,
            "blocked" => CheckboxStatus::Blocked,
            _ => CheckboxStatus::Pending,
        };

        TaskItem {
            id: self.task_id.clone(),
            content: self.content.clone(),
            status,
            deps: self.deps.clone(),
            metadata: HashMap::new(), // Simplified - metadata conversion omitted
        }
    }
}

// ============================================================================
// TaskFile Record (4.1.2)
// ============================================================================

/// A task file (TASKS.md) stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFileRecord {
    /// Record ID (format: "task_files:path_hash")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<TaskFileRecord>>,

    /// File path relative to workspace root
    pub path: String,

    /// Title from frontmatter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Description from frontmatter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Context files for agent prompts
    #[serde(default)]
    pub context_files: Vec<String>,

    /// Verification command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,

    /// Whether TDD is enabled
    #[serde(default)]
    pub tdd: bool,

    /// Additional frontmatter metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,

    /// File content hash for change detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_hash: Option<String>,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last sync timestamp
    #[serde(default = "Utc::now")]
    pub synced_at: DateTime<Utc>,
}

impl TaskFileRecord {
    /// Create a new TaskFileRecord
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            id: None,
            path: path.into(),
            title: None,
            description: None,
            context_files: Vec::new(),
            verify: None,
            tdd: false,
            metadata: HashMap::new(),
            file_hash: None,
            created_at: Utc::now(),
            synced_at: Utc::now(),
        }
    }

    /// Set title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set context files
    pub fn with_context_files(mut self, files: Vec<String>) -> Self {
        self.context_files = files;
        self
    }

    /// Set verify command
    pub fn with_verify(mut self, verify: impl Into<String>) -> Self {
        self.verify = Some(verify.into());
        self
    }
}

// ============================================================================
// TaskDependency Edge (4.1.3)
// ============================================================================

/// Edge representing task→task dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDependency {
    /// Source task (the one that depends)
    #[serde(rename = "in")]
    pub from: RecordId<TaskRecord>,

    /// Target task (the dependency)
    #[serde(rename = "out")]
    pub to: RecordId<TaskRecord>,

    /// Dependency type (e.g., "blocks", "requires")
    #[serde(default = "default_dep_type")]
    pub dep_type: String,

    /// When the dependency was created
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
}

fn default_dep_type() -> String {
    "requires".to_string()
}

impl TaskDependency {
    pub fn new(from: RecordId<TaskRecord>, to: RecordId<TaskRecord>) -> Self {
        Self {
            from,
            to,
            dep_type: "requires".to_string(),
            created_at: Utc::now(),
        }
    }

    pub fn with_type(mut self, dep_type: impl Into<String>) -> Self {
        self.dep_type = dep_type.into();
        self
    }
}

// ============================================================================
// TaskHistory (4.1.4)
// ============================================================================

/// Record of a task status change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHistory {
    /// Record ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<TaskHistory>>,

    /// Reference to the task
    pub task: RecordId<TaskRecord>,

    /// Previous status
    pub from_status: String,

    /// New status
    pub to_status: String,

    /// Who/what made the change
    pub actor: String, // "user", "agent:name", "system"

    /// Optional reason or context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// When the change occurred
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
}

impl TaskHistory {
    pub fn new(
        task: RecordId<TaskRecord>,
        from_status: impl Into<String>,
        to_status: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            id: None,
            task,
            from_status: from_status.into(),
            to_status: to_status.into(),
            actor: actor.into(),
            reason: None,
            timestamp: Utc::now(),
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Task 4.1.1 tests

    #[test]
    fn task_record_from_task_item() {
        let item = TaskItem {
            id: "1.1.1".to_string(),
            content: "Implement feature X".to_string(),
            status: CheckboxStatus::Pending,
            deps: vec!["1.1".to_string()],
            metadata: HashMap::new(),
        };

        let file_id = RecordId::new("task_files", "test");
        let record = TaskRecord::from_task_item(&item, file_id);

        assert_eq!(record.task_id, "1.1.1");
        assert_eq!(record.content, "Implement feature X");
        assert_eq!(record.status, "pending");
        assert_eq!(record.deps, vec!["1.1"]);
    }

    #[test]
    fn task_record_to_task_item_roundtrip() {
        let file_id = RecordId::new("task_files", "test");
        let record = TaskRecord {
            id: None,
            task_id: "2.1".to_string(),
            task_file: file_id,
            content: "Test task".to_string(),
            status: "done".to_string(),
            deps: vec!["1.1".to_string(), "1.2".to_string()],
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let item = record.to_task_item();
        assert_eq!(item.id, "2.1");
        assert_eq!(item.content, "Test task");
        assert_eq!(item.status, CheckboxStatus::Done);
        assert_eq!(item.deps, vec!["1.1", "1.2"]);
    }

    // Task 4.1.2 tests

    #[test]
    fn task_file_record_stores_path() {
        let record = TaskFileRecord::new("thoughts/plans/TASKS.md");
        assert_eq!(record.path, "thoughts/plans/TASKS.md");
    }

    #[test]
    fn task_file_record_stores_frontmatter() {
        let record = TaskFileRecord::new("TASKS.md")
            .with_title("My Tasks")
            .with_description("Task list")
            .with_context_files(vec!["src/lib.rs".to_string()])
            .with_verify("cargo test");

        assert_eq!(record.title, Some("My Tasks".to_string()));
        assert_eq!(record.description, Some("Task list".to_string()));
        assert_eq!(record.context_files, vec!["src/lib.rs"]);
        assert_eq!(record.verify, Some("cargo test".to_string()));
    }

    // Task 4.1.3 tests

    #[test]
    fn dependency_edge_from_to() {
        let from = RecordId::new("tasks", "task-2");
        let to = RecordId::new("tasks", "task-1");
        let dep = TaskDependency::new(from.clone(), to.clone());

        assert_eq!(dep.from.to_string(), "tasks:task-2");
        assert_eq!(dep.to.to_string(), "tasks:task-1");
        assert_eq!(dep.dep_type, "requires");
    }

    #[test]
    fn dependency_edge_queryable() {
        let from = RecordId::new("tasks", "a");
        let to = RecordId::new("tasks", "b");
        let dep = TaskDependency::new(from, to).with_type("blocks");

        assert_eq!(dep.dep_type, "blocks");
    }

    // Task 4.1.4 tests

    #[test]
    fn history_records_status_change() {
        let task_id = RecordId::new("tasks", "1.1");
        let history = TaskHistory::new(task_id, "pending", "in_progress", "user");

        assert_eq!(history.from_status, "pending");
        assert_eq!(history.to_status, "in_progress");
    }

    #[test]
    fn history_records_timestamp() {
        let task_id = RecordId::new("tasks", "1.1");
        let before = Utc::now();
        let history = TaskHistory::new(task_id, "pending", "done", "agent:worker");
        let after = Utc::now();

        assert!(history.timestamp >= before);
        assert!(history.timestamp <= after);
    }

    #[test]
    fn history_records_actor() {
        let task_id = RecordId::new("tasks", "1.1");
        let history = TaskHistory::new(task_id, "in_progress", "blocked", "agent:coordinator")
            .with_reason("Waiting for API access");

        assert_eq!(history.actor, "agent:coordinator");
        assert_eq!(history.reason, Some("Waiting for API access".to_string()));
    }
}
