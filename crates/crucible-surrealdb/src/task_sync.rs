//! Task sync between markdown files and database (4.3.3)
//!
//! Provides bidirectional sync between TASKS.md files and SurrealDB storage.

use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

use crate::schema_types::RecordId;
use crate::task_storage::TaskStorage;
use crate::task_types::{TaskFileRecord, TaskRecord};
use crucible_core::parser::{CheckboxStatus, TaskFile};

// ============================================================================
// Sync Result Types
// ============================================================================

/// Result of a sync operation
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// Tasks that were created
    pub created: Vec<String>,
    /// Tasks that were updated
    pub updated: Vec<String>,
    /// Tasks that were deleted (in DB but not in file)
    pub deleted: Vec<String>,
    /// Whether file was modified
    pub file_modified: bool,
    /// Conflicts detected (if any)
    pub conflicts: Vec<SyncConflict>,
}

impl SyncResult {
    pub fn empty() -> Self {
        Self {
            created: Vec::new(),
            updated: Vec::new(),
            deleted: Vec::new(),
            file_modified: false,
            conflicts: Vec::new(),
        }
    }

    pub fn has_changes(&self) -> bool {
        !self.created.is_empty()
            || !self.updated.is_empty()
            || !self.deleted.is_empty()
            || self.file_modified
    }
}

/// A sync conflict between markdown and DB
#[derive(Debug, Clone)]
pub struct SyncConflict {
    /// Task ID with conflict
    pub task_id: String,
    /// Status in markdown file
    pub file_status: String,
    /// Status in database
    pub db_status: String,
    /// Recommended resolution
    pub resolution: ConflictResolution,
}

/// How to resolve a conflict
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Use file version (markdown wins)
    UseFile,
    /// Use DB version (database wins)
    UseDb,
    /// Needs manual resolution
    Manual,
}

// ============================================================================
// Task Sync Service
// ============================================================================

/// Service for syncing task files with storage
pub struct TaskSync<S: TaskStorage> {
    storage: S,
}

impl<S: TaskStorage> TaskSync<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }

    /// Import tasks from a markdown file into the database
    ///
    /// Creates/updates TaskFileRecord and TaskRecords based on file content.
    pub async fn import_from_markdown(&self, path: &Path) -> Result<SyncResult> {
        let content = std::fs::read_to_string(path)?;
        let file_hash = compute_hash(&content);

        // Parse the markdown file
        let task_file = TaskFile::from_markdown(path.to_path_buf(), &content)
            .map_err(|e| anyhow::anyhow!("Parse error: {}", e))?;

        let path_str = path.to_string_lossy().to_string();

        // Check if file already exists in DB
        let existing_file = self.storage.get_task_file(&path_str).await?;
        let existing_tasks = if existing_file.is_some() {
            self.storage.list_tasks(&path_str).await?
        } else {
            Vec::new()
        };

        // Create task ID -> existing record map
        let existing_map: HashMap<String, TaskRecord> = existing_tasks
            .into_iter()
            .map(|t| (t.task_id.clone(), t))
            .collect();

        // Save/update file record
        let file_record = TaskFileRecord {
            id: existing_file.as_ref().and_then(|f| f.id.clone()),
            path: path_str.clone(),
            title: task_file.title.clone(),
            description: task_file.description.clone(),
            context_files: task_file.context_files.clone(),
            verify: task_file.verify.clone(),
            tdd: false, // TODO: parse from frontmatter
            metadata: HashMap::new(),
            file_hash: Some(file_hash),
            created_at: existing_file
                .as_ref()
                .map(|f| f.created_at)
                .unwrap_or_else(chrono::Utc::now),
            synced_at: chrono::Utc::now(),
        };

        let saved_file = self.storage.save_task_file(&file_record).await?;
        let file_id = saved_file
            .id
            .unwrap_or_else(|| RecordId::new("task_files", &path_str));

        let mut result = SyncResult::empty();

        // Process each task
        for task_item in &task_file.tasks {
            let task_record = TaskRecord::from_task_item(task_item, file_id.clone());

            if existing_map.contains_key(&task_item.id) {
                // Update existing
                self.storage.save_task(&task_record).await?;
                result.updated.push(task_item.id.clone());
            } else {
                // Create new
                self.storage.save_task(&task_record).await?;
                result.created.push(task_item.id.clone());
            }
        }

        // Find deleted tasks (in DB but not in file)
        let current_ids: std::collections::HashSet<_> =
            task_file.tasks.iter().map(|t| &t.id).collect();
        for (id, _) in &existing_map {
            if !current_ids.contains(id) {
                result.deleted.push(id.clone());
                // Note: We don't actually delete from DB - just track it
            }
        }

        Ok(result)
    }

    /// Export task state from database to markdown file
    ///
    /// Updates checkbox symbols in the file to match DB state.
    pub async fn export_to_markdown(&self, path: &Path) -> Result<SyncResult> {
        let path_str = path.to_string_lossy().to_string();

        // Get DB state
        let tasks = self.storage.list_tasks(&path_str).await?;
        if tasks.is_empty() {
            return Ok(SyncResult::empty());
        }

        // Build task_id -> status map
        let status_map: HashMap<String, String> = tasks
            .iter()
            .map(|t| (t.task_id.clone(), t.status.clone()))
            .collect();

        // Read and update file
        let content = std::fs::read_to_string(path)?;
        let updated_content = update_checkbox_symbols(&content, &status_map);

        if content != updated_content {
            std::fs::write(path, &updated_content)?;

            // Update file hash in DB
            let new_hash = compute_hash(&updated_content);
            if let Some(mut file_record) = self.storage.get_task_file(&path_str).await? {
                file_record.file_hash = Some(new_hash);
                file_record.synced_at = chrono::Utc::now();
                self.storage.save_task_file(&file_record).await?;
            }

            Ok(SyncResult {
                file_modified: true,
                updated: status_map.keys().cloned().collect(),
                ..SyncResult::empty()
            })
        } else {
            Ok(SyncResult::empty())
        }
    }

    /// Detect conflicts between markdown file and database
    ///
    /// Returns conflicts when both sides have changed independently.
    pub async fn detect_conflicts(&self, path: &Path) -> Result<Vec<SyncConflict>> {
        let path_str = path.to_string_lossy().to_string();

        // Get DB state
        let file_record = self.storage.get_task_file(&path_str).await?;
        let db_tasks = self.storage.list_tasks(&path_str).await?;

        if file_record.is_none() || db_tasks.is_empty() {
            return Ok(Vec::new());
        }

        let file_record = file_record.unwrap();

        // Read current file
        let content = std::fs::read_to_string(path)?;
        let current_hash = compute_hash(&content);

        // If hashes match, no conflict possible
        if file_record.file_hash.as_deref() == Some(&current_hash) {
            return Ok(Vec::new());
        }

        // Parse current file state
        let task_file = TaskFile::from_markdown(path.to_path_buf(), &content)
            .map_err(|e| anyhow::anyhow!("Parse error: {}", e))?;

        // Compare states
        let mut conflicts = Vec::new();

        let db_status_map: HashMap<String, String> = db_tasks
            .iter()
            .map(|t| (t.task_id.clone(), t.status.clone()))
            .collect();

        for task_item in &task_file.tasks {
            if let Some(db_status) = db_status_map.get(&task_item.id) {
                let file_status = status_to_string(&task_item.status);

                if file_status != *db_status {
                    // Determine resolution strategy
                    let resolution = determine_resolution(&file_status, db_status);

                    conflicts.push(SyncConflict {
                        task_id: task_item.id.clone(),
                        file_status,
                        db_status: db_status.clone(),
                        resolution,
                    });
                }
            }
        }

        Ok(conflicts)
    }

    /// Full bidirectional sync with conflict handling
    pub async fn sync(&self, path: &Path, prefer: ConflictResolution) -> Result<SyncResult> {
        // Detect conflicts first
        let conflicts = self.detect_conflicts(path).await?;

        if conflicts.is_empty() {
            // No conflicts - import from file (file is source of truth)
            return self.import_from_markdown(path).await;
        }

        // Handle conflicts based on preference
        match prefer {
            ConflictResolution::UseFile => {
                // File wins - just import
                self.import_from_markdown(path).await
            }
            ConflictResolution::UseDb => {
                // DB wins - export to file first, then import
                self.export_to_markdown(path).await?;
                self.import_from_markdown(path).await
            }
            ConflictResolution::Manual => {
                // Return conflicts for manual resolution
                Ok(SyncResult {
                    conflicts,
                    ..SyncResult::empty()
                })
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Compute a simple hash of content for change detection
fn compute_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Convert CheckboxStatus to string
fn status_to_string(status: &CheckboxStatus) -> String {
    match status {
        CheckboxStatus::Pending => "pending".to_string(),
        CheckboxStatus::Done => "done".to_string(),
        CheckboxStatus::InProgress => "in_progress".to_string(),
        CheckboxStatus::Cancelled => "cancelled".to_string(),
        CheckboxStatus::Blocked => "blocked".to_string(),
    }
}

/// Convert string to checkbox character
fn status_to_char(status: &str) -> char {
    match status {
        "done" => 'x',
        "in_progress" => '/',
        "cancelled" => '-',
        "blocked" => '!',
        _ => ' ',
    }
}

/// Update checkbox symbols in content based on status map
fn update_checkbox_symbols(content: &str, status_map: &HashMap<String, String>) -> String {
    let checkbox_re = Regex::new(r"^(\s*-\s*)\[(.)\](.*)$").expect("valid regex");
    let id_re = Regex::new(r"\[id::\s*([^\]]+)\]").expect("valid regex");

    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines: Vec<String> = Vec::with_capacity(lines.len());

    for line in lines {
        if let Some(caps) = checkbox_re.captures(line) {
            let rest = &caps[3];
            if let Some(id_caps) = id_re.captures(rest) {
                let id = id_caps[1].trim();
                if let Some(new_status) = status_map.get(id) {
                    let prefix = &caps[1];
                    let new_char = status_to_char(new_status);
                    let new_line = format!("{}[{}]{}", prefix, new_char, rest);
                    new_lines.push(new_line);
                    continue;
                }
            }
        }
        new_lines.push(line.to_string());
    }

    let output = new_lines.join("\n");
    if content.ends_with('\n') {
        format!("{}\n", output)
    } else {
        output
    }
}

/// Determine resolution strategy for a conflict
fn determine_resolution(file_status: &str, db_status: &str) -> ConflictResolution {
    // If DB shows more progress (done > in_progress > pending), prefer DB
    let file_priority = status_priority(file_status);
    let db_priority = status_priority(db_status);

    if db_priority > file_priority {
        ConflictResolution::UseDb
    } else if file_priority > db_priority {
        ConflictResolution::UseFile
    } else {
        ConflictResolution::Manual
    }
}

/// Get priority for status (higher = more progress)
fn status_priority(status: &str) -> u8 {
    match status {
        "done" => 4,
        "cancelled" => 3,
        "blocked" => 2,
        "in_progress" => 1,
        _ => 0, // pending
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_storage::MockTaskStorage;
    use tempfile::TempDir;

    fn create_test_tasks_md(dir: &TempDir, content: &str) -> std::path::PathBuf {
        let path = dir.path().join("TASKS.md");
        std::fs::write(&path, content).unwrap();
        path
    }

    // Task 4.3.3 tests

    #[tokio::test]
    async fn sync_imports_from_markdown() {
        let storage = MockTaskStorage::new();
        let sync = TaskSync::new(storage);

        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Test Tasks
description: Test task file
---

- [ ] Task A [id:: a]
- [x] Task B [id:: b]
- [/] Task C [id:: c] [deps:: a, b]
"#;
        let path = create_test_tasks_md(&temp_dir, content);

        let result = sync.import_from_markdown(&path).await.unwrap();

        assert_eq!(result.created.len(), 3);
        assert!(result.created.contains(&"a".to_string()));
        assert!(result.created.contains(&"b".to_string()));
        assert!(result.created.contains(&"c".to_string()));

        // Verify file record was created
        let file_record = sync
            .storage
            .get_task_file(&path.to_string_lossy())
            .await
            .unwrap();
        assert!(file_record.is_some());
        let file_record = file_record.unwrap();
        assert_eq!(file_record.title, Some("Test Tasks".to_string()));

        // Verify tasks were created with correct status
        let tasks = sync
            .storage
            .list_tasks(&path.to_string_lossy())
            .await
            .unwrap();
        assert_eq!(tasks.len(), 3);

        let task_a = tasks.iter().find(|t| t.task_id == "a").unwrap();
        assert_eq!(task_a.status, "pending");

        let task_b = tasks.iter().find(|t| t.task_id == "b").unwrap();
        assert_eq!(task_b.status, "done");

        let task_c = tasks.iter().find(|t| t.task_id == "c").unwrap();
        assert_eq!(task_c.status, "in_progress");
        assert_eq!(task_c.deps, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn sync_exports_to_markdown() {
        let storage = MockTaskStorage::new();

        // Setup: Create file record and tasks in DB
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Export Test
---

- [ ] Task A [id:: a]
- [ ] Task B [id:: b]
"#;
        let path = create_test_tasks_md(&temp_dir, content);
        let path_str = path.to_string_lossy().to_string();

        // Import first to create records
        let sync = TaskSync::new(storage);
        sync.import_from_markdown(&path).await.unwrap();

        // Update task status in DB
        sync.storage
            .update_status(&path_str, "a", "done", "test", None)
            .await
            .unwrap();

        // Export back to file
        let result = sync.export_to_markdown(&path).await.unwrap();

        assert!(result.file_modified);

        // Verify file was updated
        let updated_content = std::fs::read_to_string(&path).unwrap();
        assert!(updated_content.contains("[x] Task A [id:: a]"));
        assert!(updated_content.contains("[ ] Task B [id:: b]"));
    }

    #[tokio::test]
    async fn sync_detects_conflicts() {
        let storage = MockTaskStorage::new();
        let sync = TaskSync::new(storage);

        // Create initial file
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Conflict Test
---

- [ ] Task A [id:: a]
"#;
        let path = create_test_tasks_md(&temp_dir, content);
        let path_str = path.to_string_lossy().to_string();

        // Import to DB
        sync.import_from_markdown(&path).await.unwrap();

        // Update DB state
        sync.storage
            .update_status(&path_str, "a", "done", "agent", None)
            .await
            .unwrap();

        // Modify file independently (simulates user edit)
        let modified_content = r#"---
title: Conflict Test
---

- [/] Task A [id:: a]
"#;
        std::fs::write(&path, modified_content).unwrap();

        // Detect conflicts
        let conflicts = sync.detect_conflicts(&path).await.unwrap();

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].task_id, "a");
        assert_eq!(conflicts[0].file_status, "in_progress");
        assert_eq!(conflicts[0].db_status, "done");
        // DB has more progress, so should prefer DB
        assert_eq!(conflicts[0].resolution, ConflictResolution::UseDb);
    }

    #[test]
    fn test_update_checkbox_symbols() {
        let content = r#"---
title: Test
---

- [ ] Task A [id:: a]
- [ ] Task B [id:: b]
- [ ] Task C [id:: c]
"#;

        let mut status_map = HashMap::new();
        status_map.insert("a".to_string(), "done".to_string());
        status_map.insert("b".to_string(), "in_progress".to_string());
        status_map.insert("c".to_string(), "blocked".to_string());

        let updated = update_checkbox_symbols(content, &status_map);

        assert!(updated.contains("[x] Task A [id:: a]"));
        assert!(updated.contains("[/] Task B [id:: b]"));
        assert!(updated.contains("[!] Task C [id:: c]"));
    }

    #[test]
    fn test_status_priority() {
        assert!(status_priority("done") > status_priority("in_progress"));
        assert!(status_priority("in_progress") > status_priority("pending"));
        assert!(status_priority("blocked") > status_priority("in_progress"));
        assert!(status_priority("cancelled") > status_priority("blocked"));
    }
}
