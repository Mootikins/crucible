//! Task storage trait and implementations
//!
//! Provides abstraction for persisting task state to SurrealDB.

use anyhow::Result;
use async_trait::async_trait;

use crate::task_types::{TaskDependency, TaskFileRecord, TaskHistory, TaskRecord};

// ============================================================================
// TaskStorage Trait (4.3.1)
// ============================================================================

/// Storage abstraction for task harness persistence
#[async_trait]
pub trait TaskStorage: Send + Sync {
    // Task file operations

    /// Save a task file record
    async fn save_task_file(&self, file: &TaskFileRecord) -> Result<TaskFileRecord>;

    /// Get a task file by path
    async fn get_task_file(&self, path: &str) -> Result<Option<TaskFileRecord>>;

    /// List all task files
    async fn list_task_files(&self) -> Result<Vec<TaskFileRecord>>;

    /// Delete a task file and all its tasks
    async fn delete_task_file(&self, path: &str) -> Result<()>;

    // Task operations

    /// Save a task record
    async fn save_task(&self, task: &TaskRecord) -> Result<TaskRecord>;

    /// Get a task by file and task_id
    async fn get_task(&self, file_path: &str, task_id: &str) -> Result<Option<TaskRecord>>;

    /// List all tasks in a file
    async fn list_tasks(&self, file_path: &str) -> Result<Vec<TaskRecord>>;

    /// Update a task's status
    async fn update_status(
        &self,
        file_path: &str,
        task_id: &str,
        status: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<TaskRecord>;

    /// Get tasks by status
    async fn get_tasks_by_status(&self, file_path: &str, status: &str) -> Result<Vec<TaskRecord>>;

    // Dependency operations

    /// Save task dependencies (creates edges in graph)
    async fn save_dependencies(&self, deps: &[TaskDependency]) -> Result<()>;

    /// Get dependencies for a task
    async fn get_dependencies(&self, task_id: &str) -> Result<Vec<TaskRecord>>;

    /// Get tasks that depend on this task
    async fn get_dependents(&self, task_id: &str) -> Result<Vec<TaskRecord>>;

    // History operations

    /// Record a status change in history
    async fn record_history(&self, history: &TaskHistory) -> Result<TaskHistory>;

    /// Get history for a task
    async fn get_task_history(&self, task_id: &str) -> Result<Vec<TaskHistory>>;

    // Graph queries

    /// Find ready tasks (all deps satisfied)
    async fn find_ready_tasks(&self, file_path: &str) -> Result<Vec<TaskRecord>>;

    /// Check if task can be started (deps done, not blocked)
    async fn can_start_task(&self, file_path: &str, task_id: &str) -> Result<bool>;
}

// ============================================================================
// Mock Implementation (for testing)
// ============================================================================

/// In-memory mock implementation for testing
#[derive(Debug, Default)]
pub struct MockTaskStorage {
    pub files: std::sync::RwLock<Vec<TaskFileRecord>>,
    pub tasks: std::sync::RwLock<Vec<TaskRecord>>,
    pub history: std::sync::RwLock<Vec<TaskHistory>>,
}

impl MockTaskStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TaskStorage for MockTaskStorage {
    async fn save_task_file(&self, file: &TaskFileRecord) -> Result<TaskFileRecord> {
        let mut files = self.files.write().unwrap();

        // Remove existing if present
        files.retain(|f| f.path != file.path);

        let mut saved = file.clone();
        saved.id = Some(crate::schema_types::RecordId::new("task_files", &file.path));
        files.push(saved.clone());

        Ok(saved)
    }

    async fn get_task_file(&self, path: &str) -> Result<Option<TaskFileRecord>> {
        let files = self.files.read().unwrap();
        Ok(files.iter().find(|f| f.path == path).cloned())
    }

    async fn list_task_files(&self) -> Result<Vec<TaskFileRecord>> {
        let files = self.files.read().unwrap();
        Ok(files.clone())
    }

    async fn delete_task_file(&self, path: &str) -> Result<()> {
        let mut files = self.files.write().unwrap();
        files.retain(|f| f.path != path);

        let mut tasks = self.tasks.write().unwrap();
        tasks.retain(|t| t.task_file.id != path);

        Ok(())
    }

    async fn save_task(&self, task: &TaskRecord) -> Result<TaskRecord> {
        let mut tasks = self.tasks.write().unwrap();

        // Remove existing if present
        tasks.retain(|t| !(t.task_file.id == task.task_file.id && t.task_id == task.task_id));

        let mut saved = task.clone();
        saved.id = Some(crate::schema_types::RecordId::new(
            "tasks",
            format!("{}:{}", task.task_file.id, task.task_id),
        ));
        tasks.push(saved.clone());

        Ok(saved)
    }

    async fn get_task(&self, file_path: &str, task_id: &str) -> Result<Option<TaskRecord>> {
        let tasks = self.tasks.read().unwrap();
        Ok(tasks
            .iter()
            .find(|t| t.task_file.id == file_path && t.task_id == task_id)
            .cloned())
    }

    async fn list_tasks(&self, file_path: &str) -> Result<Vec<TaskRecord>> {
        let tasks = self.tasks.read().unwrap();
        Ok(tasks
            .iter()
            .filter(|t| t.task_file.id == file_path)
            .cloned()
            .collect())
    }

    async fn update_status(
        &self,
        file_path: &str,
        task_id: &str,
        status: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<TaskRecord> {
        // Update task and collect info for history (lock scope ends here)
        let (updated, old_status) = {
            let mut tasks = self.tasks.write().unwrap();

            let task = tasks
                .iter_mut()
                .find(|t| t.task_file.id == file_path && t.task_id == task_id)
                .ok_or_else(|| anyhow::anyhow!("Task not found"))?;

            let old_status = task.status.clone();
            task.status = status.to_string();
            task.updated_at = chrono::Utc::now();

            (task.clone(), old_status)
        };

        // Record history (no locks held during await)
        let task_ref =
            crate::schema_types::RecordId::new("tasks", format!("{}:{}", file_path, task_id));
        let history = TaskHistory::new(task_ref, &old_status, status, actor);
        let history = if let Some(r) = reason {
            history.with_reason(r)
        } else {
            history
        };
        self.record_history(&history).await?;

        Ok(updated)
    }

    async fn get_tasks_by_status(&self, file_path: &str, status: &str) -> Result<Vec<TaskRecord>> {
        let tasks = self.tasks.read().unwrap();
        Ok(tasks
            .iter()
            .filter(|t| t.task_file.id == file_path && t.status == status)
            .cloned()
            .collect())
    }

    async fn save_dependencies(&self, _deps: &[TaskDependency]) -> Result<()> {
        // Dependencies are stored in task.deps field, not as separate edges in mock
        Ok(())
    }

    async fn get_dependencies(&self, task_id: &str) -> Result<Vec<TaskRecord>> {
        let tasks = self.tasks.read().unwrap();

        // Find the task
        let task = tasks.iter().find(|t| t.task_id == task_id);

        if let Some(t) = task {
            // Find all tasks that this task depends on
            let deps: Vec<TaskRecord> = tasks
                .iter()
                .filter(|other| t.deps.contains(&other.task_id))
                .cloned()
                .collect();
            Ok(deps)
        } else {
            Ok(vec![])
        }
    }

    async fn get_dependents(&self, task_id: &str) -> Result<Vec<TaskRecord>> {
        let tasks = self.tasks.read().unwrap();

        // Find all tasks that depend on this task
        Ok(tasks
            .iter()
            .filter(|t| t.deps.contains(&task_id.to_string()))
            .cloned()
            .collect())
    }

    async fn record_history(&self, history: &TaskHistory) -> Result<TaskHistory> {
        let mut hist = self.history.write().unwrap();

        let mut saved = history.clone();
        saved.id = Some(crate::schema_types::RecordId::new(
            "task_history",
            format!("{}", hist.len()),
        ));
        hist.push(saved.clone());

        Ok(saved)
    }

    async fn get_task_history(&self, task_id: &str) -> Result<Vec<TaskHistory>> {
        let hist = self.history.read().unwrap();
        Ok(hist
            .iter()
            .filter(|h| h.task.id.contains(task_id))
            .cloned()
            .collect())
    }

    async fn find_ready_tasks(&self, file_path: &str) -> Result<Vec<TaskRecord>> {
        let tasks = self.tasks.read().unwrap();
        let file_tasks: Vec<&TaskRecord> = tasks
            .iter()
            .filter(|t| t.task_file.id == file_path)
            .collect();

        // Find pending tasks with all deps done
        let ready: Vec<TaskRecord> = file_tasks
            .iter()
            .filter(|t| {
                if t.status != "pending" {
                    return false;
                }

                // Check all deps are done
                t.deps.iter().all(|dep_id| {
                    file_tasks
                        .iter()
                        .find(|other| other.task_id == *dep_id)
                        .map(|other| other.status == "done")
                        .unwrap_or(false) // Missing dep = not ready
                })
            })
            .cloned()
            .cloned()
            .collect();

        Ok(ready)
    }

    async fn can_start_task(&self, file_path: &str, task_id: &str) -> Result<bool> {
        let tasks = self.tasks.read().unwrap();

        let task = tasks
            .iter()
            .find(|t| t.task_file.id == file_path && t.task_id == task_id);

        if let Some(t) = task {
            if t.status != "pending" {
                return Ok(false);
            }

            // Check all deps are done
            let file_tasks: Vec<&TaskRecord> = tasks
                .iter()
                .filter(|other| other.task_file.id == file_path)
                .collect();

            let all_deps_done = t.deps.iter().all(|dep_id| {
                file_tasks
                    .iter()
                    .find(|other| other.task_id == *dep_id)
                    .map(|other| other.status == "done")
                    .unwrap_or(false)
            });

            Ok(all_deps_done)
        } else {
            Ok(false)
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_types::RecordId;

    fn create_test_file() -> TaskFileRecord {
        TaskFileRecord::new("test/TASKS.md").with_title("Test Tasks")
    }

    fn create_test_task(file_id: &str, task_id: &str, deps: Vec<String>) -> TaskRecord {
        TaskRecord {
            id: None,
            task_id: task_id.to_string(),
            task_file: RecordId::new("task_files", file_id),
            content: format!("Task {}", task_id),
            status: "pending".to_string(),
            deps,
            metadata: Default::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    // Task 4.3.1 tests: TaskStorage trait

    #[tokio::test]
    async fn trait_has_save_task() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        let task = create_test_task("test/TASKS.md", "1.1", vec![]);
        let saved = storage.save_task(&task).await.unwrap();

        assert_eq!(saved.task_id, "1.1");
        assert!(saved.id.is_some());
    }

    #[tokio::test]
    async fn trait_has_get_task() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        let task = create_test_task("test/TASKS.md", "1.1", vec![]);
        storage.save_task(&task).await.unwrap();

        let found = storage.get_task("test/TASKS.md", "1.1").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().task_id, "1.1");

        let not_found = storage.get_task("test/TASKS.md", "999").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn trait_has_list_tasks() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        storage
            .save_task(&create_test_task("test/TASKS.md", "1.1", vec![]))
            .await
            .unwrap();
        storage
            .save_task(&create_test_task("test/TASKS.md", "1.2", vec![]))
            .await
            .unwrap();
        storage
            .save_task(&create_test_task("test/TASKS.md", "1.3", vec![]))
            .await
            .unwrap();

        let tasks = storage.list_tasks("test/TASKS.md").await.unwrap();
        assert_eq!(tasks.len(), 3);
    }

    #[tokio::test]
    async fn trait_has_update_status() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        let task = create_test_task("test/TASKS.md", "1.1", vec![]);
        storage.save_task(&task).await.unwrap();

        let updated = storage
            .update_status("test/TASKS.md", "1.1", "done", "user", None)
            .await
            .unwrap();

        assert_eq!(updated.status, "done");

        // Verify history was recorded
        let history = storage.get_task_history("1.1").await.unwrap();
        assert!(!history.is_empty());
        assert_eq!(history[0].from_status, "pending");
        assert_eq!(history[0].to_status, "done");
    }

    // Task 4.3.4 tests: Graph queries

    #[tokio::test]
    async fn graph_query_finds_ready() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        // Task A has no deps - should be ready
        storage
            .save_task(&create_test_task("test/TASKS.md", "a", vec![]))
            .await
            .unwrap();

        // Task B depends on A - should NOT be ready
        storage
            .save_task(&create_test_task(
                "test/TASKS.md",
                "b",
                vec!["a".to_string()],
            ))
            .await
            .unwrap();

        let ready = storage.find_ready_tasks("test/TASKS.md").await.unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].task_id, "a");
    }

    #[tokio::test]
    async fn graph_query_excludes_blocked_deps() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        // Task A - done
        let mut task_a = create_test_task("test/TASKS.md", "a", vec![]);
        task_a.status = "done".to_string();
        storage.save_task(&task_a).await.unwrap();

        // Task B depends on A - should be ready (A is done)
        storage
            .save_task(&create_test_task(
                "test/TASKS.md",
                "b",
                vec!["a".to_string()],
            ))
            .await
            .unwrap();

        // Task C depends on B - should NOT be ready (B not done)
        storage
            .save_task(&create_test_task(
                "test/TASKS.md",
                "c",
                vec!["b".to_string()],
            ))
            .await
            .unwrap();

        let ready = storage.find_ready_tasks("test/TASKS.md").await.unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].task_id, "b");
    }

    // Additional tests for untested methods

    #[tokio::test]
    async fn save_task_file_and_get_task_file() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();

        let saved = storage.save_task_file(&file).await.unwrap();
        assert_eq!(saved.path, "test/TASKS.md");
        assert_eq!(saved.title, Some("Test Tasks".to_string()));
        assert!(saved.id.is_some());

        let retrieved = storage.get_task_file("test/TASKS.md").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.path, "test/TASKS.md");
        assert_eq!(retrieved.title, Some("Test Tasks".to_string()));
    }

    #[tokio::test]
    async fn list_task_files_returns_all_saved() {
        let storage = MockTaskStorage::new();

        let file1 = TaskFileRecord::new("test/TASKS1.md").with_title("Tasks 1");
        let file2 = TaskFileRecord::new("test/TASKS2.md").with_title("Tasks 2");
        let file3 = TaskFileRecord::new("test/TASKS3.md").with_title("Tasks 3");

        storage.save_task_file(&file1).await.unwrap();
        storage.save_task_file(&file2).await.unwrap();
        storage.save_task_file(&file3).await.unwrap();

        let files = storage.list_task_files().await.unwrap();
        assert_eq!(files.len(), 3);
        assert!(files.iter().any(|f| f.path == "test/TASKS1.md"));
        assert!(files.iter().any(|f| f.path == "test/TASKS2.md"));
        assert!(files.iter().any(|f| f.path == "test/TASKS3.md"));
    }

    #[tokio::test]
    async fn get_task_file_returns_none_for_missing() {
        let storage = MockTaskStorage::new();

        let result = storage.get_task_file("nonexistent/TASKS.md").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn save_task_file_upserts_existing() {
        let storage = MockTaskStorage::new();

        let file1 = TaskFileRecord::new("test/TASKS.md").with_title("Original Title");
        storage.save_task_file(&file1).await.unwrap();

        let file2 = TaskFileRecord::new("test/TASKS.md").with_title("Updated Title");
        storage.save_task_file(&file2).await.unwrap();

        let files = storage.list_task_files().await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].title, Some("Updated Title".to_string()));
    }

    #[tokio::test]
    async fn delete_task_file_removes_file_and_tasks() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        storage
            .save_task(&create_test_task("test/TASKS.md", "1.1", vec![]))
            .await
            .unwrap();
        storage
            .save_task(&create_test_task("test/TASKS.md", "1.2", vec![]))
            .await
            .unwrap();

        storage.delete_task_file("test/TASKS.md").await.unwrap();

        let files = storage.list_task_files().await.unwrap();
        assert_eq!(files.len(), 0);

        let tasks = storage.list_tasks("test/TASKS.md").await.unwrap();
        assert_eq!(tasks.len(), 0);
    }

    #[tokio::test]
    async fn get_tasks_by_status_filters_correctly() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        let mut task1 = create_test_task("test/TASKS.md", "1.1", vec![]);
        task1.status = "pending".to_string();
        storage.save_task(&task1).await.unwrap();

        let mut task2 = create_test_task("test/TASKS.md", "1.2", vec![]);
        task2.status = "done".to_string();
        storage.save_task(&task2).await.unwrap();

        let mut task3 = create_test_task("test/TASKS.md", "1.3", vec![]);
        task3.status = "pending".to_string();
        storage.save_task(&task3).await.unwrap();

        let pending = storage
            .get_tasks_by_status("test/TASKS.md", "pending")
            .await
            .unwrap();
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().all(|t| t.status == "pending"));

        let done = storage
            .get_tasks_by_status("test/TASKS.md", "done")
            .await
            .unwrap();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].task_id, "1.2");
    }

    #[tokio::test]
    async fn get_dependencies_returns_dep_tasks() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        storage
            .save_task(&create_test_task("test/TASKS.md", "a", vec![]))
            .await
            .unwrap();
        storage
            .save_task(&create_test_task("test/TASKS.md", "b", vec![]))
            .await
            .unwrap();
        storage
            .save_task(&create_test_task(
                "test/TASKS.md",
                "c",
                vec!["a".to_string(), "b".to_string()],
            ))
            .await
            .unwrap();

        let deps = storage.get_dependencies("c").await.unwrap();
        assert_eq!(deps.len(), 2);
        assert!(deps.iter().any(|t| t.task_id == "a"));
        assert!(deps.iter().any(|t| t.task_id == "b"));
    }

    #[tokio::test]
    async fn get_dependents_returns_dependent_tasks() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        storage
            .save_task(&create_test_task("test/TASKS.md", "a", vec![]))
            .await
            .unwrap();
        storage
            .save_task(&create_test_task(
                "test/TASKS.md",
                "b",
                vec!["a".to_string()],
            ))
            .await
            .unwrap();
        storage
            .save_task(&create_test_task(
                "test/TASKS.md",
                "c",
                vec!["a".to_string()],
            ))
            .await
            .unwrap();

        let dependents = storage.get_dependents("a").await.unwrap();
        assert_eq!(dependents.len(), 2);
        assert!(dependents.iter().any(|t| t.task_id == "b"));
        assert!(dependents.iter().any(|t| t.task_id == "c"));
    }

    #[tokio::test]
    async fn record_history_and_get_task_history() {
        let storage = MockTaskStorage::new();

        let task_ref = RecordId::new("tasks", "test:1.1");
        let history1 = TaskHistory::new(task_ref.clone(), "pending", "in_progress", "user1");
        let history2 = TaskHistory::new(task_ref, "in_progress", "done", "user2");

        storage.record_history(&history1).await.unwrap();
        storage.record_history(&history2).await.unwrap();

        let histories = storage.get_task_history("1.1").await.unwrap();
        assert_eq!(histories.len(), 2);
        assert_eq!(histories[0].from_status, "pending");
        assert_eq!(histories[0].to_status, "in_progress");
        assert_eq!(histories[1].from_status, "in_progress");
        assert_eq!(histories[1].to_status, "done");
    }

    #[tokio::test]
    async fn update_status_with_reason_records_in_history() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        let task = create_test_task("test/TASKS.md", "1.1", vec![]);
        storage.save_task(&task).await.unwrap();

        storage
            .update_status(
                "test/TASKS.md",
                "1.1",
                "done",
                "user",
                Some("Completed successfully"),
            )
            .await
            .unwrap();

        let histories = storage.get_task_history("1.1").await.unwrap();
        assert_eq!(histories.len(), 1);
        assert_eq!(
            histories[0].reason,
            Some("Completed successfully".to_string())
        );
    }

    #[tokio::test]
    async fn can_start_task_returns_true_when_deps_done() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        let mut dep_task = create_test_task("test/TASKS.md", "a", vec![]);
        dep_task.status = "done".to_string();
        storage.save_task(&dep_task).await.unwrap();

        storage
            .save_task(&create_test_task(
                "test/TASKS.md",
                "b",
                vec!["a".to_string()],
            ))
            .await
            .unwrap();

        let can_start = storage.can_start_task("test/TASKS.md", "b").await.unwrap();
        assert!(can_start);
    }

    #[tokio::test]
    async fn can_start_task_returns_false_when_deps_pending() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        storage
            .save_task(&create_test_task("test/TASKS.md", "a", vec![]))
            .await
            .unwrap();

        storage
            .save_task(&create_test_task(
                "test/TASKS.md",
                "b",
                vec!["a".to_string()],
            ))
            .await
            .unwrap();

        let can_start = storage.can_start_task("test/TASKS.md", "b").await.unwrap();
        assert!(!can_start);
    }

    #[tokio::test]
    async fn can_start_task_returns_false_for_non_pending_task() {
        let storage = MockTaskStorage::new();
        let file = create_test_file();
        storage.save_task_file(&file).await.unwrap();

        let mut task = create_test_task("test/TASKS.md", "a", vec![]);
        task.status = "done".to_string();
        storage.save_task(&task).await.unwrap();

        let can_start = storage.can_start_task("test/TASKS.md", "a").await.unwrap();
        assert!(!can_start);
    }

    #[tokio::test]
    async fn can_start_task_returns_false_for_missing_task() {
        let storage = MockTaskStorage::new();

        let can_start = storage
            .can_start_task("test/TASKS.md", "nonexistent")
            .await
            .unwrap();
        assert!(!can_start);
    }
}
