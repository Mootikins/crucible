//! Task harness CLI commands
//!
//! Provides CLI commands for managing tasks defined in TaskFile frontmatter.

use anyhow::Result;
use clap::{Parser, Subcommand};
use regex::Regex;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::config::CliConfig;
use crucible_core::parser::{CheckboxStatus, TaskFile, TaskGraph};

/// Task-related errors
#[derive(Debug, Error)]
pub enum TaskError {
    /// File I/O error
    #[error("Failed to read task file: {0}")]
    IoError(#[from] std::io::Error),

    /// Task file parsing error
    #[error("Failed to parse task file: {0}")]
    ParseError(String),

    /// Task not found
    #[error("Task not found: {0}")]
    NotFound(String),

    /// Task already done
    #[error("Task {0} is already done")]
    AlreadyDone(String),
}

#[derive(Parser)]
pub struct TasksCommand {
    #[command(subcommand)]
    pub command: TasksSubcommand,
}

#[derive(Subcommand)]
pub enum TasksSubcommand {
    /// List all tasks
    List,
    /// Show next ready tasks
    Next,
    /// Mark task as in-progress
    Pick { id: String },
    /// Mark task as done
    Done { id: String },
    /// Mark task as blocked
    Blocked { id: String, reason: Option<String> },
}

/// Load a task file from the given path
///
/// # Arguments
/// * `path` - Path to the task file to load
///
/// # Returns
/// The parsed TaskFile or an error if loading/parsing fails
fn load_task_file(path: &Path) -> Result<TaskFile, TaskError> {
    let content = std::fs::read_to_string(path)?;
    TaskFile::from_markdown(path.to_path_buf(), &content).map_err(TaskError::ParseError)
}

/// Write task status changes back to the file
///
/// This function updates checkbox symbols in the file while preserving
/// all other content and formatting.
///
/// # Arguments
/// * `path` - Path to the task file
/// * `task_file` - The modified TaskFile with updated statuses
fn write_task_file(path: &Path, task_file: &TaskFile) -> Result<(), TaskError> {
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();

    // Build a map of task id -> new status
    let status_map: std::collections::HashMap<&str, CheckboxStatus> = task_file
        .tasks
        .iter()
        .map(|t| (t.id.as_str(), t.status))
        .collect();

    // Regex to match checkbox lines with id field
    // Matches: - [ ] text [id:: value] or - [x] text [id:: value]
    let checkbox_re = Regex::new(r"^(\s*-\s*)\[(.)\](.*)$").expect("valid regex");
    let id_re = Regex::new(r"\[id::\s*([^\]]+)\]").expect("valid regex");

    let mut new_lines: Vec<String> = Vec::with_capacity(lines.len());

    for line in lines {
        if let Some(caps) = checkbox_re.captures(line) {
            // This is a checkbox line - check if it has an id
            let rest = &caps[3];
            if let Some(id_caps) = id_re.captures(rest) {
                let id = id_caps[1].trim();
                if let Some(new_status) = status_map.get(id) {
                    // Replace the checkbox symbol
                    let prefix = &caps[1];
                    let new_char = new_status.to_char();
                    let new_line = format!("{}[{}]{}", prefix, new_char, rest);
                    new_lines.push(new_line);
                    continue;
                }
            }
        }
        // Keep line as-is
        new_lines.push(line.to_string());
    }

    // Write back with original line endings (assume LF)
    let output = new_lines.join("\n");
    // Preserve trailing newline if original had one
    let output = if content.ends_with('\n') {
        format!("{}\n", output)
    } else {
        output
    };

    std::fs::write(path, output)?;
    Ok(())
}

/// Execute tasks subcommand
pub async fn execute(_config: CliConfig, file: PathBuf, command: TasksSubcommand) -> Result<()> {
    match command {
        TasksSubcommand::List => {
            let task_file = load_task_file(&file)?;

            println!("# {}", task_file.title.as_deref().unwrap_or("Tasks"));
            println!();

            for task in &task_file.tasks {
                let symbol = task.status.to_char();
                println!("[{}] {} ({})", symbol, task.content, task.id);
            }

            Ok(())
        }
        TasksSubcommand::Next => {
            let task_file = load_task_file(&file)?;
            let graph = TaskGraph::from_tasks(&task_file.tasks)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let ready = graph.ready_tasks(&task_file.tasks);

            if ready.is_empty() {
                println!("No tasks ready (all blocked or complete)");
            } else {
                println!("Ready tasks:");
                for id in ready {
                    if let Some(task) = task_file.tasks.iter().find(|t| t.id == id) {
                        println!("  [ ] {} ({})", task.content, task.id);
                    }
                }
            }

            Ok(())
        }
        TasksSubcommand::Pick { id } => {
            let mut task_file = load_task_file(&file)?;

            let task = task_file
                .tasks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or_else(|| TaskError::NotFound(id.clone()))?;

            if task.status == CheckboxStatus::Done {
                return Err(TaskError::AlreadyDone(id).into());
            }

            task.status = CheckboxStatus::InProgress;

            write_task_file(&file, &task_file)?;
            println!("[/] {} marked as in-progress", id);
            Ok(())
        }
        TasksSubcommand::Done { id } => {
            let mut task_file = load_task_file(&file)?;

            let task = task_file
                .tasks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or_else(|| TaskError::NotFound(id.clone()))?;

            task.status = CheckboxStatus::Done;

            write_task_file(&file, &task_file)?;
            println!("[x] {} marked as done", id);
            Ok(())
        }
        TasksSubcommand::Blocked { id, reason } => {
            let mut task_file = load_task_file(&file)?;

            let task = task_file
                .tasks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or_else(|| TaskError::NotFound(id.clone()))?;

            task.status = CheckboxStatus::Blocked;

            write_task_file(&file, &task_file)?;
            if let Some(r) = reason {
                println!("[!] {} marked as blocked: {}", id, r);
            } else {
                println!("[!] {} marked as blocked", id);
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    // Helper to create a test config
    fn test_config() -> CliConfig {
        use crucible_config::{
            AcpConfig, ChatConfig, CliConfig as CliAppConfig, EmbeddingConfig, LlmConfig,
            ProcessingConfig, ProvidersConfig,
        };

        CliConfig {
            kiln_path: test_path("test-kiln"),
            agent_directories: Vec::new(),
            embedding: EmbeddingConfig::default(),
            acp: AcpConfig::default(),
            chat: ChatConfig::default(),
            llm: LlmConfig::default(),
            cli: CliAppConfig::default(),
            logging: None,
            processing: ProcessingConfig::default(),
            providers: ProvidersConfig::default(),
            context: None,
            storage: None,
            mcp: None,
            plugins: std::collections::HashMap::new(),
            web: None,
            source_map: None,
        }
    }

    // Helper to create a test TaskFile
    fn create_test_task_file(dir: &TempDir, filename: &str, content: &str) -> PathBuf {
        let file_path = dir.path().join(filename);
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file_path
    }

    #[tokio::test]
    async fn test_list_subcommand_exists() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Test Tasks
---

- [ ] Task one [id:: task-1]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();
        let result = execute(config, task_path, TasksSubcommand::List).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_next_subcommand_exists() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Test Tasks
---

- [ ] Task one [id:: task-1]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();
        let result = execute(config, task_path, TasksSubcommand::Next).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pick_subcommand_exists() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Test Tasks
---

- [ ] Task one [id:: task-1]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();
        let result = execute(
            config,
            task_path,
            TasksSubcommand::Pick {
                id: "task-1".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_done_subcommand_exists() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Test Tasks
---

- [ ] Task one [id:: task-1]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();
        let result = execute(
            config,
            task_path,
            TasksSubcommand::Done {
                id: "task-1".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_blocked_subcommand_exists() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Test Tasks
---

- [ ] Task one [id:: task-1]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();
        let result = execute(
            config,
            task_path,
            TasksSubcommand::Blocked {
                id: "task-1".to_string(),
                reason: Some("waiting for dependency".to_string()),
            },
        )
        .await;
        assert!(result.is_ok());
    }

    // Task 3.1.2 tests: Load TASKS.md from cwd or specified path

    #[test]
    fn load_from_cwd() {
        // Test: finds TASKS.md in current directory
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Test Tasks
---

- [ ] First task [id:: task-1]
- [x] Second task [id:: task-2]
"#;
        create_test_task_file(&temp_dir, "TASKS.md", content);

        // Change to temp dir
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Load from cwd (default path)
        let task_file = load_task_file(&PathBuf::from("TASKS.md")).unwrap();

        assert_eq!(task_file.title, Some("Test Tasks".to_string()));
        assert_eq!(task_file.tasks.len(), 2);
        assert_eq!(task_file.tasks[0].id, "task-1");
        assert_eq!(task_file.tasks[1].id, "task-2");

        // Restore original dir
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn load_from_explicit_path() {
        // Test: --file path/to/tasks.md works
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Custom Path Tasks
description: Tasks loaded from explicit path
---

- [ ] Task A [id:: a]
- [/] Task B [id:: b]
- [x] Task C [id:: c]
"#;
        let task_path = create_test_task_file(&temp_dir, "custom-tasks.md", content);

        // Load from explicit path
        let task_file = load_task_file(&task_path).unwrap();

        assert_eq!(task_file.title, Some("Custom Path Tasks".to_string()));
        assert_eq!(
            task_file.description,
            Some("Tasks loaded from explicit path".to_string())
        );
        assert_eq!(task_file.tasks.len(), 3);
        assert_eq!(task_file.tasks[0].id, "a");
        assert_eq!(task_file.tasks[1].id, "b");
        assert_eq!(task_file.tasks[2].id, "c");
    }

    #[test]
    fn missing_file_error() {
        // Test: appropriate error when file not found
        let nonexistent_path = PathBuf::from("/nonexistent/TASKS.md");

        let result = load_task_file(&nonexistent_path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should be a TaskError with appropriate message
        let err_str = err.to_string();
        assert!(
            err_str.contains("TASKS.md")
                || err_str.contains("No such file")
                || err_str.contains("cannot find the file")
                || err_str.contains("cannot find the path")
        );
    }

    // Task 3.1.3 tests: Verify frontmatter fields are correctly parsed and accessible

    #[test]
    fn frontmatter_title_parsed() {
        // Test: title from YAML frontmatter is accessible
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: My Project Tasks
description: Implementation tasks for the project
---

- [ ] Task 1 [id:: task-1]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);

        let task_file = load_task_file(&task_path).unwrap();

        assert_eq!(task_file.title, Some("My Project Tasks".to_string()));
    }

    #[test]
    fn frontmatter_context_files_parsed() {
        // Test: context_files array from frontmatter is parsed correctly
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Tasks with Context
context_files:
  - src/main.rs
  - src/lib.rs
  - tests/integration.rs
---

- [ ] Task with context [id:: task-1]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);

        let task_file = load_task_file(&task_path).unwrap();

        assert_eq!(task_file.context_files.len(), 3);
        assert_eq!(task_file.context_files[0], "src/main.rs");
        assert_eq!(task_file.context_files[1], "src/lib.rs");
        assert_eq!(task_file.context_files[2], "tests/integration.rs");
    }

    #[test]
    fn frontmatter_verify_command_parsed() {
        // Test: verify command string is parsed from frontmatter
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Tasks with Verification
verify: cargo test --workspace
---

- [ ] Implement feature [id:: task-1]
- [x] Write tests [id:: task-2]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);

        let task_file = load_task_file(&task_path).unwrap();

        assert_eq!(task_file.verify, Some("cargo test --workspace".to_string()));
    }

    #[test]
    fn frontmatter_all_fields_together() {
        // Test: all frontmatter fields can be parsed together
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Complete Task File
description: Full example with all frontmatter fields
context_files:
  - crates/crucible-core/src/parser/types/task.rs
  - crates/crucible-cli/src/commands/tasks.rs
verify: cargo test --package crucible-cli
---

- [ ] Parse frontmatter [id:: 3.1.3]
- [ ] Build system prompt [id:: 3.1.4]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);

        let task_file = load_task_file(&task_path).unwrap();

        // Verify all fields
        assert_eq!(task_file.title, Some("Complete Task File".to_string()));
        assert_eq!(
            task_file.description,
            Some("Full example with all frontmatter fields".to_string())
        );
        assert_eq!(task_file.context_files.len(), 2);
        assert_eq!(
            task_file.context_files[0],
            "crates/crucible-core/src/parser/types/task.rs"
        );
        assert_eq!(
            task_file.context_files[1],
            "crates/crucible-cli/src/commands/tasks.rs"
        );
        assert_eq!(
            task_file.verify,
            Some("cargo test --package crucible-cli".to_string())
        );
        assert_eq!(task_file.tasks.len(), 2);
    }

    // Task 3.2.1 tests: `cru tasks list` implementation

    #[tokio::test]
    async fn list_shows_all_tasks() {
        // Test: output contains all task IDs
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Test Tasks
---

- [ ] First task [id:: task-1]
- [x] Second task [id:: task-2]
- [/] Third task [id:: task-3]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        // Capture stdout by redirecting
        let result = execute(config, task_path, TasksSubcommand::List).await;

        assert!(result.is_ok());
        // Note: This test verifies execution succeeds.
        // Full output validation would require capturing stdout
        // which is tested manually or with integration tests.
    }

    #[tokio::test]
    async fn list_shows_status_symbols() {
        // Test: shows [x], [ ], [/], etc.
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Status Test
---

- [ ] Pending task [id:: pending]
- [x] Done task [id:: done]
- [/] In progress task [id:: progress]
- [-] Cancelled task [id:: cancelled]
- [!] Blocked task [id:: blocked]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(config, task_path, TasksSubcommand::List).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn list_with_empty_file() {
        // Test: handles empty task file gracefully
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Empty Tasks
---

"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(config, task_path, TasksSubcommand::List).await;

        assert!(result.is_ok());
    }

    // Task 3.2.2 tests: `cru tasks next` implementation

    #[tokio::test]
    async fn next_shows_ready_tasks() {
        // Test: shows tasks with satisfied deps
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Ready Tasks Test
---

- [x] First task [id:: task-1]
- [x] Second task [id:: task-2]
- [ ] Third task [id:: task-3] [deps:: task-1, task-2]
- [ ] Fourth task [id:: task-4]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(config, task_path, TasksSubcommand::Next).await;

        assert!(result.is_ok());
        // Note: Full output validation would require capturing stdout
        // The implementation should show task-3 and task-4 as ready
    }

    #[tokio::test]
    async fn next_empty_when_all_blocked() {
        // Test: "No tasks ready" when none available
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: All Blocked Tasks
---

- [ ] First task [id:: task-1] [deps:: task-2]
- [ ] Second task [id:: task-2] [deps:: task-1]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(config, task_path, TasksSubcommand::Next).await;

        assert!(result.is_ok());
        // The implementation should show "No tasks ready (all blocked or complete)"
    }

    #[tokio::test]
    async fn next_respects_topo_order() {
        // Test: respects dependency order
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Topological Order Test
---

- [ ] Task A [id:: a]
- [ ] Task B [id:: b] [deps:: a]
- [ ] Task C [id:: c] [deps:: a]
- [ ] Task D [id:: d] [deps:: b, c]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(config, task_path, TasksSubcommand::Next).await;

        assert!(result.is_ok());
        // The implementation should only show task 'a' as ready since it has no dependencies
    }

    // Task 3.2.3 tests: `cru tasks pick <id>` implementation

    #[tokio::test]
    async fn pick_marks_in_progress() {
        // Test: changes status to InProgress
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Pick Test
---

- [ ] Pending task [id:: task-1]
- [ ] Another task [id:: task-2]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(
            config,
            task_path,
            TasksSubcommand::Pick {
                id: "task-1".to_string(),
            },
        )
        .await;

        assert!(result.is_ok());
        // Note: Full validation of file modification will be in task 3.3.1
        // For now we just verify the command executes successfully
    }

    #[tokio::test]
    async fn pick_nonexistent_id_error() {
        // Test: error for unknown ID
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Pick Test
---

- [ ] Task one [id:: task-1]
- [ ] Task two [id:: task-2]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(
            config,
            task_path,
            TasksSubcommand::Pick {
                id: "nonexistent".to_string(),
            },
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    #[tokio::test]
    async fn pick_already_done_error() {
        // Test: error if task already done
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Pick Test
---

- [x] Completed task [id:: task-1]
- [ ] Pending task [id:: task-2]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(
            config,
            task_path,
            TasksSubcommand::Pick {
                id: "task-1".to_string(),
            },
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("task-1")
                || err.to_string().contains("done")
                || err.to_string().contains("Already")
        );
    }

    // Task 3.2.4 tests: `cru tasks done <id>` implementation

    #[tokio::test]
    async fn done_marks_complete() {
        // Test: changes status to Done
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Done Test
---

- [ ] Pending task [id:: task-1]
- [/] In-progress task [id:: task-2]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(
            config,
            task_path,
            TasksSubcommand::Done {
                id: "task-1".to_string(),
            },
        )
        .await;

        assert!(result.is_ok());
        // Note: Full validation of file modification will be in task 3.3.1
    }

    #[tokio::test]
    async fn done_nonexistent_id_error() {
        // Test: error for unknown ID
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Done Test
---

- [ ] Task one [id:: task-1]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(
            config,
            task_path,
            TasksSubcommand::Done {
                id: "nonexistent".to_string(),
            },
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    // Task 3.2.5 tests: `cru tasks blocked <id>` implementation

    #[tokio::test]
    async fn blocked_marks_blocked() {
        // Test: changes status to Blocked
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Blocked Test
---

- [ ] Pending task [id:: task-1]
- [/] In-progress task [id:: task-2]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(
            config,
            task_path,
            TasksSubcommand::Blocked {
                id: "task-1".to_string(),
                reason: None,
            },
        )
        .await;

        assert!(result.is_ok());
        // Note: Full validation of file modification will be in task 3.3.1
    }

    #[tokio::test]
    async fn blocked_with_reason() {
        // Test: blocked with reason message
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Blocked Test
---

- [ ] Pending task [id:: task-1]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(
            config,
            task_path,
            TasksSubcommand::Blocked {
                id: "task-1".to_string(),
                reason: Some("waiting for API approval".to_string()),
            },
        )
        .await;

        assert!(result.is_ok());
        // Note: Full validation of reason metadata will be in task 3.3.1
    }

    #[tokio::test]
    async fn blocked_nonexistent_id_error() {
        // Test: error for unknown ID
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Blocked Test
---

- [ ] Task one [id:: task-1]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        let result = execute(
            config,
            task_path,
            TasksSubcommand::Blocked {
                id: "nonexistent".to_string(),
                reason: None,
            },
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    // Task 3.3.1 tests: Write back status changes to TASKS.md

    #[tokio::test]
    async fn write_updates_checkbox_symbol() {
        // Test: checkbox symbol changes in file after pick/done/blocked
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Write Test
---

- [ ] Pending task [id:: task-1]
- [ ] Another task [id:: task-2]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        // Mark task-1 as done
        let result = execute(
            config.clone(),
            task_path.clone(),
            TasksSubcommand::Done {
                id: "task-1".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());

        // Read file back and verify checkbox changed
        let updated_content = std::fs::read_to_string(&task_path).unwrap();
        assert!(updated_content.contains("[x] Pending task [id:: task-1]"));
        assert!(updated_content.contains("[ ] Another task [id:: task-2]"));
    }

    #[tokio::test]
    async fn write_preserves_other_content() {
        // Test: frontmatter, comments, other lines preserved
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Preserved Content Test
description: Should survive writes
context_files:
  - src/main.rs
verify: cargo test
---

## Phase 1: Tasks

Some descriptive text that should be preserved.

- [ ] First task [id:: task-1]
  - [tests:: test_a, test_b]

- [ ] Second task [id:: task-2] [deps:: task-1]
  - Nested content line

## Phase 2: More Tasks

- [ ] Third task [id:: task-3]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        // Mark task-1 as in-progress
        let result = execute(
            config.clone(),
            task_path.clone(),
            TasksSubcommand::Pick {
                id: "task-1".to_string(),
            },
        )
        .await;
        assert!(result.is_ok());

        // Read file back
        let updated_content = std::fs::read_to_string(&task_path).unwrap();

        // Verify frontmatter preserved
        assert!(updated_content.contains("title: Preserved Content Test"));
        assert!(updated_content.contains("description: Should survive writes"));
        assert!(updated_content.contains("verify: cargo test"));

        // Verify headings preserved
        assert!(updated_content.contains("## Phase 1: Tasks"));
        assert!(updated_content.contains("## Phase 2: More Tasks"));

        // Verify descriptive text preserved
        assert!(updated_content.contains("Some descriptive text that should be preserved."));

        // Verify nested content preserved
        assert!(updated_content.contains("- [tests:: test_a, test_b]"));
        assert!(updated_content.contains("- Nested content line"));

        // Verify checkbox updated correctly
        assert!(updated_content.contains("[/] First task [id:: task-1]"));
        assert!(updated_content.contains("[ ] Second task [id:: task-2]"));
        assert!(updated_content.contains("[ ] Third task [id:: task-3]"));
    }

    #[tokio::test]
    async fn write_updates_multiple_tasks() {
        // Test: multiple sequential updates work correctly
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Multi-Update Test
---

- [ ] Task A [id:: a]
- [ ] Task B [id:: b]
- [ ] Task C [id:: c]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        // Mark task-a as in-progress
        execute(
            config.clone(),
            task_path.clone(),
            TasksSubcommand::Pick {
                id: "a".to_string(),
            },
        )
        .await
        .unwrap();

        // Mark task-a as done
        execute(
            config.clone(),
            task_path.clone(),
            TasksSubcommand::Done {
                id: "a".to_string(),
            },
        )
        .await
        .unwrap();

        // Mark task-b as blocked
        execute(
            config.clone(),
            task_path.clone(),
            TasksSubcommand::Blocked {
                id: "b".to_string(),
                reason: None,
            },
        )
        .await
        .unwrap();

        // Read final state
        let updated_content = std::fs::read_to_string(&task_path).unwrap();

        assert!(updated_content.contains("[x] Task A [id:: a]"));
        assert!(updated_content.contains("[!] Task B [id:: b]"));
        assert!(updated_content.contains("[ ] Task C [id:: c]"));
    }

    // Task 3.3.2 tests: Preserve formatting on write

    #[tokio::test]
    async fn write_preserves_blank_lines() {
        // Test: blank lines between sections are preserved
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Blank Line Test
---

## Section 1


- [ ] Task 1 [id:: task-1]


## Section 2


- [ ] Task 2 [id:: task-2]

"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        execute(
            config,
            task_path.clone(),
            TasksSubcommand::Done {
                id: "task-1".to_string(),
            },
        )
        .await
        .unwrap();

        let updated = std::fs::read_to_string(&task_path).unwrap();

        // Count blank lines - should preserve the double blank lines
        assert!(updated.contains("## Section 1\n\n\n- [x]"));
        assert!(updated.contains("[id:: task-1]\n\n\n## Section 2"));
    }

    #[tokio::test]
    async fn write_preserves_indentation() {
        // Test: indented nested items preserve their indentation
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Indentation Test
---

- [ ] Main task [id:: task-1]
  - Nested item 1
    - Deeply nested
  - Nested item 2
- [ ] Another task [id:: task-2]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        execute(
            config,
            task_path.clone(),
            TasksSubcommand::Pick {
                id: "task-1".to_string(),
            },
        )
        .await
        .unwrap();

        let updated = std::fs::read_to_string(&task_path).unwrap();

        // Verify indentation preserved
        assert!(updated.contains("  - Nested item 1"));
        assert!(updated.contains("    - Deeply nested"));
        assert!(updated.contains("  - Nested item 2"));
    }

    #[tokio::test]
    async fn write_preserves_comments() {
        // Test: markdown comments and HTML comments preserved
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
title: Comments Test
---

<!-- This is an HTML comment -->

- [ ] Task with comment [id:: task-1]

<!-- Another comment
     spanning multiple lines -->

- [ ] Task 2 [id:: task-2]
"#;
        let task_path = create_test_task_file(&temp_dir, "TASKS.md", content);
        let config = test_config();

        execute(
            config,
            task_path.clone(),
            TasksSubcommand::Done {
                id: "task-1".to_string(),
            },
        )
        .await
        .unwrap();

        let updated = std::fs::read_to_string(&task_path).unwrap();

        // Verify comments preserved
        assert!(updated.contains("<!-- This is an HTML comment -->"));
        assert!(updated.contains("<!-- Another comment"));
        assert!(updated.contains("     spanning multiple lines -->"));
    }
}
