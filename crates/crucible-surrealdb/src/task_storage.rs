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
            &format!("{}:{}", task.task_file.id, task.task_id),
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
        let task_ref = crate::schema_types::RecordId::new("tasks", &format!("{}:{}", file_path, task_id));
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
            &format!("{}", hist.len()),
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
            .save_task(&create_test_task("test/TASKS.md", "b", vec!["a".to_string()]))
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
            .save_task(&create_test_task("test/TASKS.md", "b", vec!["a".to_string()]))
            .await
            .unwrap();

        // Task C depends on B - should NOT be ready (B not done)
        storage
            .save_task(&create_test_task("test/TASKS.md", "c", vec!["b".to_string()]))
            .await
            .unwrap();

        let ready = storage.find_ready_tasks("test/TASKS.md").await.unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].task_id, "b");
    }
}
