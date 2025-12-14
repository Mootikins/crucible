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

/// Task dependency graph
///
/// Represents the dependency relationships between tasks for topological sorting
/// and scheduling. Maintains both forward (dependencies) and reverse (dependents) edges.
#[derive(Debug, Clone)]
pub struct TaskGraph {
    /// Forward edges: task_id -> tasks it depends on
    dependencies: HashMap<String, Vec<String>>,
    /// Reverse edges: task_id -> tasks that depend on it
    dependents: HashMap<String, Vec<String>>,
}

/// Error building task graph
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    /// Task depends on a non-existent task ID
    MissingDependency { task_id: String, missing_dep: String },
    /// Dependency graph contains a cycle
    CycleDetected { cycle: Vec<String> },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::MissingDependency { task_id, missing_dep } => {
                write!(f, "Task '{}' depends on non-existent task '{}'", task_id, missing_dep)
            }
            GraphError::CycleDetected { cycle } => {
                write!(f, "Dependency cycle detected: {}", cycle.join(" -> "))
            }
        }
    }
}

impl std::error::Error for GraphError {}

impl TaskGraph {
    /// Build a task graph from a list of tasks
    ///
    /// # Arguments
    /// * `tasks` - List of tasks to build graph from
    ///
    /// # Returns
    /// TaskGraph with dependency edges, or error if dependencies are invalid
    ///
    /// # Errors
    /// Returns `GraphError::MissingDependency` if a task depends on a non-existent task ID
    pub fn from_tasks(tasks: &[TaskItem]) -> Result<Self, GraphError> {
        let mut dependencies = HashMap::new();
        let mut dependents = HashMap::new();

        // Collect all task IDs for validation
        let task_ids: std::collections::HashSet<_> = tasks.iter().map(|t| t.id.as_str()).collect();

        // Build dependency graph
        for task in tasks {
            // Verify all dependencies exist
            for dep in &task.deps {
                if !task_ids.contains(dep.as_str()) {
                    return Err(GraphError::MissingDependency {
                        task_id: task.id.clone(),
                        missing_dep: dep.clone(),
                    });
                }
            }

            // Add forward edges (dependencies)
            dependencies.insert(task.id.clone(), task.deps.clone());

            // Add reverse edges (dependents)
            for dep in &task.deps {
                dependents
                    .entry(dep.clone())
                    .or_insert_with(Vec::new)
                    .push(task.id.clone());
            }

            // Initialize empty dependents list if not already present
            dependents.entry(task.id.clone()).or_insert_with(Vec::new);
        }

        Ok(Self {
            dependencies,
            dependents,
        })
    }

    /// Get tasks that this task depends on
    pub fn dependencies_of(&self, id: &str) -> &[String] {
        self.dependencies.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get tasks that depend on this task
    pub fn dependents_of(&self, id: &str) -> &[String] {
        self.dependents.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Returns task IDs in topological order (dependencies before dependents)
    ///
    /// Uses Kahn's algorithm for topological sorting:
    /// 1. Find all nodes with in-degree 0 (no dependencies)
    /// 2. Add to result, remove their edges
    /// 3. Repeat until empty
    /// 4. If nodes remain with in-degree > 0, there's a cycle
    ///
    /// # Returns
    /// Vector of task IDs in execution order
    ///
    /// # Errors
    /// Returns `GraphError::CycleDetected` if graph contains a cycle
    pub fn topo_sort(&self) -> Result<Vec<String>, GraphError> {
        // Build in-degree map (count of dependencies for each task)
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        // Initialize all tasks with their dependency count
        for (task_id, deps) in &self.dependencies {
            in_degree.insert(task_id.clone(), deps.len());
        }

        // Find all nodes with in-degree 0 (no dependencies)
        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|(_, &count)| count == 0)
            .map(|(id, _)| id.clone())
            .collect();

        // Sort queue for deterministic ordering when multiple nodes have same in-degree
        queue.sort();

        let mut result = Vec::new();

        // Process nodes in topological order
        while !queue.is_empty() {
            // Remove first element (sorted order)
            let task_id = queue.remove(0);
            result.push(task_id.clone());

            // For each dependent of this task
            for dependent in self.dependents_of(&task_id) {
                // Decrease in-degree
                if let Some(count) = in_degree.get_mut(dependent) {
                    *count -= 1;

                    // If in-degree reaches 0, add to queue
                    if *count == 0 {
                        queue.push(dependent.clone());
                        queue.sort(); // Keep sorted for deterministic order
                    }
                }
            }
        }

        // Check for cycles: if not all nodes were processed, there's a cycle
        // Nodes with in-degree > 0 are part of cycles
        let unprocessed: Vec<String> = in_degree
            .iter()
            .filter(|(_, &count)| count > 0)
            .map(|(id, _)| id.clone())
            .collect();

        if !unprocessed.is_empty() {
            // Return the cycle nodes (sorted for deterministic error messages)
            let mut cycle = unprocessed;
            cycle.sort();
            return Err(GraphError::CycleDetected { cycle });
        }

        Ok(result)
    }

    /// Find tasks ready to execute (pending + all deps satisfied)
    ///
    /// A task is ready if:
    /// 1. Its status is Pending (not Done, InProgress, or Blocked)
    /// 2. All of its dependencies have status Done
    ///
    /// # Arguments
    /// * `tasks` - List of tasks to check for readiness
    ///
    /// # Returns
    /// Vector of task IDs that are ready to execute
    pub fn ready_tasks(&self, tasks: &[TaskItem]) -> Vec<String> {
        // Build a map of task ID to status for fast lookups
        let status_map: HashMap<&str, CheckboxStatus> = tasks
            .iter()
            .map(|t| (t.id.as_str(), t.status))
            .collect();

        let mut ready = Vec::new();

        for task in tasks {
            // Only consider pending tasks
            if task.status != CheckboxStatus::Pending {
                continue;
            }

            // Check if all dependencies are done
            let all_deps_done = task.deps.iter().all(|dep_id| {
                status_map
                    .get(dep_id.as_str())
                    .map(|&status| status == CheckboxStatus::Done)
                    .unwrap_or(false)
            });

            if all_deps_done {
                ready.push(task.id.clone());
            }
        }

        // Sort for deterministic ordering
        ready.sort();

        ready
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

    // TaskGraph tests
    #[test]
    fn build_graph_from_tasks() {
        // Creates graph with nodes for each task
        let tasks = vec![
            TaskItem::with_id(
                "task-1".to_string(),
                "First task".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "task-2".to_string(),
                "Second task".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "task-3".to_string(),
                "Third task".to_string(),
                CheckboxStatus::Pending,
                vec!["task-1".to_string()],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();

        // All tasks should be in the graph
        assert_eq!(graph.dependencies_of("task-1").len(), 0);
        assert_eq!(graph.dependencies_of("task-2").len(), 0);
        assert_eq!(graph.dependencies_of("task-3").len(), 1);
    }

    #[test]
    fn graph_edges_match_deps() {
        // Edges correctly represent dependencies
        let tasks = vec![
            TaskItem::with_id(
                "task-1".to_string(),
                "First".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "task-2".to_string(),
                "Second".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "task-3".to_string(),
                "Third".to_string(),
                CheckboxStatus::Pending,
                vec!["task-1".to_string(), "task-2".to_string()],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();

        // Check forward edges (dependencies)
        let deps = graph.dependencies_of("task-3");
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&"task-1".to_string()));
        assert!(deps.contains(&"task-2".to_string()));

        // Check reverse edges (dependents)
        assert_eq!(graph.dependents_of("task-1"), &["task-3"]);
        assert_eq!(graph.dependents_of("task-2"), &["task-3"]);
        assert!(graph.dependents_of("task-3").is_empty());
    }

    #[test]
    fn missing_dep_returns_error() {
        // Task depends on non-existent ID -> error
        let tasks = vec![
            TaskItem::with_id(
                "task-1".to_string(),
                "First".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "task-2".to_string(),
                "Second".to_string(),
                CheckboxStatus::Pending,
                vec!["task-1".to_string(), "task-nonexistent".to_string()],
                HashMap::new(),
            ),
        ];

        let result = TaskGraph::from_tasks(&tasks);
        assert!(result.is_err());

        if let Err(GraphError::MissingDependency { task_id, missing_dep }) = result {
            assert_eq!(task_id, "task-2");
            assert_eq!(missing_dep, "task-nonexistent");
        } else {
            panic!("Expected MissingDependency error");
        }
    }

    // Topological sort tests
    #[test]
    fn topo_sort_linear_deps() {
        // A→B→C should return [A, B, C]
        let tasks = vec![
            TaskItem::with_id(
                "A".to_string(),
                "Task A".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "B".to_string(),
                "Task B".to_string(),
                CheckboxStatus::Pending,
                vec!["A".to_string()],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "C".to_string(),
                "Task C".to_string(),
                CheckboxStatus::Pending,
                vec!["B".to_string()],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();
        let order = graph.topo_sort().unwrap();

        assert_eq!(order, vec!["A", "B", "C"]);
    }

    #[test]
    fn topo_sort_diamond_deps() {
        // A→B,C→D should return valid order (A before B,C before D)
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        let tasks = vec![
            TaskItem::with_id(
                "A".to_string(),
                "Task A".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "B".to_string(),
                "Task B".to_string(),
                CheckboxStatus::Pending,
                vec!["A".to_string()],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "C".to_string(),
                "Task C".to_string(),
                CheckboxStatus::Pending,
                vec!["A".to_string()],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "D".to_string(),
                "Task D".to_string(),
                CheckboxStatus::Pending,
                vec!["B".to_string(), "C".to_string()],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();
        let order = graph.topo_sort().unwrap();

        // Verify A is first
        assert_eq!(order[0], "A");
        // Verify D is last
        assert_eq!(order[3], "D");
        // Verify B and C come after A and before D
        let b_pos = order.iter().position(|x| x == "B").unwrap();
        let c_pos = order.iter().position(|x| x == "C").unwrap();
        assert!(b_pos > 0 && b_pos < 3);
        assert!(c_pos > 0 && c_pos < 3);
    }

    #[test]
    fn topo_sort_independent_tasks() {
        // Tasks with no dependencies can be in any order
        let tasks = vec![
            TaskItem::with_id(
                "X".to_string(),
                "Task X".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "Y".to_string(),
                "Task Y".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "Z".to_string(),
                "Task Z".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();
        let order = graph.topo_sort().unwrap();

        // All tasks should be in the result
        assert_eq!(order.len(), 3);
        assert!(order.contains(&"X".to_string()));
        assert!(order.contains(&"Y".to_string()));
        assert!(order.contains(&"Z".to_string()));
    }

    // Cycle detection tests
    #[test]
    fn cycle_detection_self_reference() {
        // Task depends on itself -> error
        let tasks = vec![
            TaskItem::with_id(
                "A".to_string(),
                "Task A".to_string(),
                CheckboxStatus::Pending,
                vec!["A".to_string()],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();
        let result = graph.topo_sort();

        assert!(result.is_err());
        if let Err(GraphError::CycleDetected { cycle }) = result {
            assert!(!cycle.is_empty());
            assert!(cycle.contains(&"A".to_string()));
        } else {
            panic!("Expected CycleDetected error");
        }
    }

    #[test]
    fn cycle_detection_two_node() {
        // A→B→A should return error
        let tasks = vec![
            TaskItem::with_id(
                "A".to_string(),
                "Task A".to_string(),
                CheckboxStatus::Pending,
                vec!["B".to_string()],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "B".to_string(),
                "Task B".to_string(),
                CheckboxStatus::Pending,
                vec!["A".to_string()],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();
        let result = graph.topo_sort();

        assert!(result.is_err());
        if let Err(GraphError::CycleDetected { cycle }) = result {
            assert!(!cycle.is_empty());
            // Both nodes should be in the cycle
            assert!(cycle.contains(&"A".to_string()));
            assert!(cycle.contains(&"B".to_string()));
        } else {
            panic!("Expected CycleDetected error");
        }
    }

    #[test]
    fn cycle_detection_three_node() {
        // A→B→C→A should return error
        let tasks = vec![
            TaskItem::with_id(
                "A".to_string(),
                "Task A".to_string(),
                CheckboxStatus::Pending,
                vec!["C".to_string()],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "B".to_string(),
                "Task B".to_string(),
                CheckboxStatus::Pending,
                vec!["A".to_string()],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "C".to_string(),
                "Task C".to_string(),
                CheckboxStatus::Pending,
                vec!["B".to_string()],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();
        let result = graph.topo_sort();

        assert!(result.is_err());
        if let Err(GraphError::CycleDetected { cycle }) = result {
            assert!(!cycle.is_empty());
            // All three nodes should be in the cycle
            assert!(cycle.contains(&"A".to_string()));
            assert!(cycle.contains(&"B".to_string()));
            assert!(cycle.contains(&"C".to_string()));
        } else {
            panic!("Expected CycleDetected error");
        }
    }

    #[test]
    fn no_cycle_returns_ok() {
        // Valid graph without cycles should succeed
        let tasks = vec![
            TaskItem::with_id(
                "A".to_string(),
                "Task A".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "B".to_string(),
                "Task B".to_string(),
                CheckboxStatus::Pending,
                vec!["A".to_string()],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "C".to_string(),
                "Task C".to_string(),
                CheckboxStatus::Pending,
                vec!["B".to_string()],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();
        let result = graph.topo_sort();

        assert!(result.is_ok());
        let order = result.unwrap();
        assert_eq!(order, vec!["A", "B", "C"]);
    }

    // ready_tasks tests
    #[test]
    fn ready_tasks_no_deps() {
        // Tasks with no deps are ready (if pending)
        let tasks = vec![
            TaskItem::with_id(
                "task-1".to_string(),
                "First task".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "task-2".to_string(),
                "Second task".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();
        let ready = graph.ready_tasks(&tasks);

        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&"task-1".to_string()));
        assert!(ready.contains(&"task-2".to_string()));
    }

    #[test]
    fn ready_tasks_all_deps_done() {
        // Task with all deps Done is ready
        let tasks = vec![
            TaskItem::with_id(
                "task-1".to_string(),
                "First task".to_string(),
                CheckboxStatus::Done,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "task-2".to_string(),
                "Second task".to_string(),
                CheckboxStatus::Done,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "task-3".to_string(),
                "Third task".to_string(),
                CheckboxStatus::Pending,
                vec!["task-1".to_string(), "task-2".to_string()],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();
        let ready = graph.ready_tasks(&tasks);

        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], "task-3");
    }

    #[test]
    fn ready_tasks_excludes_in_progress() {
        // InProgress tasks not in ready list
        let tasks = vec![
            TaskItem::with_id(
                "task-1".to_string(),
                "First task".to_string(),
                CheckboxStatus::InProgress,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "task-2".to_string(),
                "Second task".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();
        let ready = graph.ready_tasks(&tasks);

        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], "task-2");
    }

    #[test]
    fn ready_tasks_excludes_blocked() {
        // Blocked tasks not in ready list
        let tasks = vec![
            TaskItem::with_id(
                "task-1".to_string(),
                "First task".to_string(),
                CheckboxStatus::Blocked,
                vec![],
                HashMap::new(),
            ),
            TaskItem::with_id(
                "task-2".to_string(),
                "Second task".to_string(),
                CheckboxStatus::Pending,
                vec![],
                HashMap::new(),
            ),
        ];

        let graph = TaskGraph::from_tasks(&tasks).unwrap();
        let ready = graph.ready_tasks(&tasks);

        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], "task-2");
    }
}
