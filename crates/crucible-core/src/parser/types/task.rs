//! Task types for task list items with inline metadata
//!
//! Supports TASKS.md format with Dataview-style inline metadata:
//! - `[id:: task-1]` - task identifier
//! - `[deps:: task-a, task-b]` - task dependencies
//! - Other metadata like `[priority:: high]`, `[estimate:: 2h]`

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::frontmatter::Frontmatter;
use super::inline_metadata::{extract_inline_metadata, InlineMetadata};
use super::lists::CheckboxStatus;

/// Task item from TASKS.md file
///
/// Represents a single task with:
/// - Unique identifier (from `[id:: x]` or auto-generated)
/// - Content text (with metadata stripped)
/// - Checkbox status (from `[x]`, `[ ]`, etc.)
/// - Dependencies (from `[deps:: a, b]`)
/// - Additional metadata fields
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskItem {
    /// Task identifier (from [id:: x] or auto-generated)
    pub id: String,

    /// Task text content (metadata stripped)
    pub content: String,

    /// Checkbox status from task list item
    pub status: CheckboxStatus,

    /// Task dependencies (from [deps:: a, b])
    pub deps: Vec<String>,

    /// All other inline metadata fields
    pub metadata: HashMap<String, InlineMetadata>,
}

impl TaskItem {
    /// Create a new task item from parsed data
    ///
    /// This constructor extracts the ID and dependencies from the metadata
    /// and builds the task item.
    ///
    /// # Arguments
    /// * `content` - Task text content (metadata stripped)
    /// * `status` - Checkbox status from the task list item
    /// * `metadata` - All inline metadata fields from the task
    pub fn new(
        content: String,
        status: CheckboxStatus,
        metadata: HashMap<String, InlineMetadata>,
    ) -> Self {
        // Extract ID from metadata, or generate one
        let id = metadata
            .get("id")
            .and_then(|m| m.as_string())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Self::generate_id());

        // Extract dependencies from metadata
        let deps = metadata
            .get("deps")
            .map(|m| m.as_vec().to_vec())
            .unwrap_or_default();

        Self {
            id,
            content,
            status,
            deps,
            metadata,
        }
    }

    /// Create a new task item with explicit ID
    ///
    /// Useful when the ID is already extracted from metadata.
    pub fn with_id(
        id: String,
        content: String,
        status: CheckboxStatus,
        deps: Vec<String>,
        metadata: HashMap<String, InlineMetadata>,
    ) -> Self {
        Self {
            id,
            content,
            status,
            deps,
            metadata,
        }
    }

    /// Generate a unique task ID
    ///
    /// Uses a simple counter-based approach for now.
    /// In production, this would use a UUID or content-based hash.
    fn generate_id() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("generated-{}", count)
    }
}

/// Task file representing an entire TASKS.md file
///
/// Represents a TASKS.md file with:
/// - Frontmatter metadata (title, description, context_files, verify)
/// - Collection of all tasks in the file
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskFile {
    /// Source file path
    pub path: std::path::PathBuf,

    /// Title from frontmatter
    pub title: Option<String>,

    /// Description from frontmatter
    pub description: Option<String>,

    /// Context files from frontmatter
    pub context_files: Vec<String>,

    /// Verify command from frontmatter
    pub verify: Option<String>,

    /// All tasks in file
    pub tasks: Vec<TaskItem>,
}

impl TaskFile {
    /// Parse a TaskFile from markdown content
    ///
    /// # Arguments
    /// * `path` - Source file path
    /// * `content` - Markdown content with frontmatter and tasks
    ///
    /// # Returns
    /// A TaskFile with parsed frontmatter and tasks
    pub fn from_markdown(path: std::path::PathBuf, content: &str) -> Result<Self, String> {
        // Parse frontmatter
        let (frontmatter, body) = Self::extract_frontmatter(content);

        // Extract frontmatter fields
        let title = frontmatter.as_ref().and_then(|fm| fm.get_string("title"));
        let description = frontmatter.as_ref().and_then(|fm| fm.get_string("description"));
        let context_files = frontmatter
            .as_ref()
            .and_then(|fm| fm.get_array("context_files"))
            .unwrap_or_default();
        let verify = frontmatter.as_ref().and_then(|fm| fm.get_string("verify"));

        // Parse tasks from body
        let tasks = Self::parse_tasks(&body);

        Ok(Self {
            path,
            title,
            description,
            context_files,
            verify,
            tasks,
        })
    }

    /// Extract frontmatter from content
    fn extract_frontmatter(content: &str) -> (Option<Frontmatter>, String) {
        // Simple YAML frontmatter extraction
        if let Some(rest) = content.strip_prefix("---\n") {
            if let Some(end_idx) = rest.find("\n---\n") {
                let yaml_content = &rest[..end_idx];
                let body = &rest[end_idx + 5..];
                let frontmatter = Frontmatter::new(
                    yaml_content.to_string(),
                    super::frontmatter::FrontmatterFormat::Yaml,
                );
                return (Some(frontmatter), body.to_string());
            }
        }
        (None, content.to_string())
    }

    /// Parse all tasks from markdown body
    fn parse_tasks(body: &str) -> Vec<TaskItem> {
        let mut tasks = Vec::new();

        // Regex to match checkbox items: - [ ] or - [x] or - [/] etc.
        let checkbox_re = Regex::new(r"^[\s]*-\s*\[(.)\]\s*(.+)$").expect("valid regex");

        for line in body.lines() {
            if let Some(caps) = checkbox_re.captures(line) {
                let status_char = caps[1].chars().next().unwrap();
                let content = caps[2].to_string();

                // Parse checkbox status
                let status = CheckboxStatus::from_char(status_char)
                    .unwrap_or(CheckboxStatus::Pending);

                // Extract inline metadata
                let metadata_vec = extract_inline_metadata(&content);
                let mut metadata_map = HashMap::new();
                for meta in metadata_vec {
                    metadata_map.insert(meta.key.clone(), meta);
                }

                // Strip metadata from content
                let clean_content = Self::strip_inline_metadata(&content);

                // Create task item
                let task = TaskItem::new(clean_content, status, metadata_map);
                tasks.push(task);
            }
        }

        tasks
    }

    /// Strip inline metadata from content
    fn strip_inline_metadata(content: &str) -> String {
        let re = Regex::new(r"\s*\[([^:]+)::\s*([^\]]+)\]").expect("valid regex");
        re.replace_all(content, "").trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_item_from_list_item() {
        // Test basic construction from parsed data
        let mut metadata = HashMap::new();
        metadata.insert(
            "id".to_string(),
            InlineMetadata::new("id", "task-1"),
        );
        metadata.insert(
            "priority".to_string(),
            InlineMetadata::new("priority", "high"),
        );

        let task = TaskItem::new(
            "Implement TaskItem struct".to_string(),
            CheckboxStatus::InProgress,
            metadata,
        );

        assert_eq!(task.id, "task-1");
        assert_eq!(task.content, "Implement TaskItem struct");
        assert_eq!(task.status, CheckboxStatus::InProgress);
        assert!(task.deps.is_empty());
        assert_eq!(task.metadata.len(), 2);
    }

    #[test]
    fn task_item_deps_parsed_as_vec() {
        // Test that deps field is correctly parsed as array
        let mut metadata = HashMap::new();
        metadata.insert(
            "id".to_string(),
            InlineMetadata::new("id", "task-3"),
        );
        metadata.insert(
            "deps".to_string(),
            InlineMetadata::new_array(
                "deps",
                vec!["task-1".to_string(), "task-2".to_string()],
            ),
        );

        let task = TaskItem::new(
            "Final task".to_string(),
            CheckboxStatus::Pending,
            metadata,
        );

        assert_eq!(task.id, "task-3");
        assert_eq!(task.deps.len(), 2);
        assert_eq!(task.deps[0], "task-1");
        assert_eq!(task.deps[1], "task-2");
    }

    #[test]
    fn task_item_without_id_uses_generated() {
        // Test that tasks without [id::] get a generated ID
        let metadata = HashMap::new();

        let task = TaskItem::new(
            "Task without explicit ID".to_string(),
            CheckboxStatus::Pending,
            metadata,
        );

        assert!(task.id.starts_with("generated-"));
        assert!(!task.metadata.contains_key("id"));
    }

    // TaskFile tests
    #[test]
    fn task_file_parse_frontmatter() {
        let content = r#"---
title: My Task List
description: Test tasks for the project
context_files:
  - file1.rs
  - file2.rs
verify: cargo test
---

# Tasks

- [ ] First task
"#;

        let path = std::path::PathBuf::from("/test/TASKS.md");
        let task_file = TaskFile::from_markdown(path.clone(), content).unwrap();

        assert_eq!(task_file.path, path);
        assert_eq!(task_file.title, Some("My Task List".to_string()));
        assert_eq!(task_file.description, Some("Test tasks for the project".to_string()));
        assert_eq!(task_file.context_files.len(), 2);
        assert_eq!(task_file.context_files[0], "file1.rs");
        assert_eq!(task_file.context_files[1], "file2.rs");
        assert_eq!(task_file.verify, Some("cargo test".to_string()));
    }

    #[test]
    fn task_file_collect_all_tasks() {
        let content = r#"---
title: Task List
---

# Tasks

- [ ] First task [id:: task-1]
- [x] Second task [id:: task-2]
- [/] Third task [id:: task-3]
"#;

        let path = std::path::PathBuf::from("/test/TASKS.md");
        let task_file = TaskFile::from_markdown(path, content).unwrap();

        assert_eq!(task_file.tasks.len(), 3);
        assert_eq!(task_file.tasks[0].id, "task-1");
        assert_eq!(task_file.tasks[0].status, CheckboxStatus::Pending);
        assert_eq!(task_file.tasks[1].id, "task-2");
        assert_eq!(task_file.tasks[1].status, CheckboxStatus::Done);
        assert_eq!(task_file.tasks[2].id, "task-3");
        assert_eq!(task_file.tasks[2].status, CheckboxStatus::InProgress);
    }

    #[test]
    fn task_file_from_markdown_string() {
        let content = r#"---
title: Complete Task File
description: Full example with all features
context_files:
  - src/main.rs
verify: just test
---

# My Tasks

- [ ] Implement feature A [id:: feat-a] [priority:: high]
- [x] Fix bug B [id:: bug-b]
- [/] Review PR [id:: pr-review] [deps:: feat-a, bug-b]

Some other content...

- [ ] Another task [id:: task-4]
"#;

        let path = std::path::PathBuf::from("/test/TASKS.md");
        let task_file = TaskFile::from_markdown(path.clone(), content).unwrap();

        // Verify frontmatter
        assert_eq!(task_file.title, Some("Complete Task File".to_string()));
        assert_eq!(task_file.description, Some("Full example with all features".to_string()));
        assert_eq!(task_file.context_files, vec!["src/main.rs"]);
        assert_eq!(task_file.verify, Some("just test".to_string()));

        // Verify tasks
        assert_eq!(task_file.tasks.len(), 4);
        assert_eq!(task_file.tasks[0].id, "feat-a");
        assert_eq!(task_file.tasks[1].id, "bug-b");
        assert_eq!(task_file.tasks[2].id, "pr-review");
        assert_eq!(task_file.tasks[2].deps, vec!["feat-a", "bug-b"]);
        assert_eq!(task_file.tasks[3].id, "task-4");
    }
}
