//! Core workspace tools for file and shell operations
//!
//! These tools provide essential workspace operations for agents:
//! - `read_file`: Read file contents with optional line range
//! - `edit_file`: Edit file via search/replace
//! - `write_file`: Write content to file
//! - `bash`: Execute shell commands
//! - `glob`: Find files by pattern
//! - `grep`: Search file contents
//!
//! ## Design
//!
//! - All tools operate on absolute paths or relative to workspace root
//! - Uses `ToolRef` for unified tool representation
//! - Compatible with both Rig (direct) and MCP (gateway) modes

#![allow(clippy::missing_errors_doc)] // Tool methods have obvious error conditions
#![allow(clippy::doc_markdown)] // Parameter names in docs don't need backticks
#![allow(clippy::needless_pass_by_value)] // Tools take owned strings for JSON compat

use rmcp::model::{CallToolResult, Content, Tool};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;

/// Workspace tools for file and shell operations
#[derive(Debug, Clone)]
pub struct WorkspaceTools {
    /// Workspace root directory
    workspace_root: PathBuf,
    /// Default timeout for bash commands (ms)
    default_timeout_ms: u64,
}

impl WorkspaceTools {
    /// Create new workspace tools
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            default_timeout_ms: 120_000,
        }
    }

    /// Set default timeout for bash commands
    #[must_use]
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.default_timeout_ms = timeout_ms;
        self
    }

    /// Resolve a path (absolute or relative to workspace)
    fn resolve_path(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            self.workspace_root.join(p)
        }
    }

    /// Get tool definitions for registration
    #[must_use]
    pub fn tool_definitions() -> Vec<Tool> {
        vec![
            Self::read_file_definition(),
            Self::edit_file_definition(),
            Self::write_file_definition(),
            Self::bash_definition(),
            Self::glob_definition(),
            Self::grep_definition(),
        ]
    }

    fn read_file_definition() -> Tool {
        Tool {
            name: Cow::Borrowed("read_file"),
            title: None,
            description: Some(Cow::Borrowed(
                "Read file contents. Returns content with line numbers.",
            )),
            input_schema: Arc::new(
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to file (absolute or relative to workspace)"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Line number to start from (1-indexed)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum lines to read"
                        }
                    },
                    "required": ["path"]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    fn edit_file_definition() -> Tool {
        Tool {
            name: Cow::Borrowed("edit_file"),
            title: None,
            description: Some(Cow::Borrowed(
                "Edit file by replacing text. old_string must match exactly.",
            )),
            input_schema: Arc::new(
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to file"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "Text to find and replace"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "Replacement text"
                        },
                        "replace_all": {
                            "type": "boolean",
                            "description": "Replace all occurrences (default: false)"
                        }
                    },
                    "required": ["path", "old_string", "new_string"]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    fn write_file_definition() -> Tool {
        Tool {
            name: Cow::Borrowed("write_file"),
            title: None,
            description: Some(Cow::Borrowed(
                "Write content to file. Creates parent directories if needed.",
            )),
            input_schema: Arc::new(
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to file"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write"
                        }
                    },
                    "required": ["path", "content"]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    fn bash_definition() -> Tool {
        Tool {
            name: Cow::Borrowed("bash"),
            title: None,
            description: Some(Cow::Borrowed(
                "Execute bash command. Use for git, npm, cargo, etc.",
            )),
            input_schema: Arc::new(
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Bash command to execute"
                        },
                        "timeout_ms": {
                            "type": "integer",
                            "description": "Timeout in milliseconds (default: 120000)"
                        }
                    },
                    "required": ["command"]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    fn glob_definition() -> Tool {
        Tool {
            name: Cow::Borrowed("glob"),
            title: None,
            description: Some(Cow::Borrowed(
                "Find files matching glob pattern (e.g., '**/*.rs').",
            )),
            input_schema: Arc::new(
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search (default: workspace root)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results (default: 100)"
                        }
                    },
                    "required": ["pattern"]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    fn grep_definition() -> Tool {
        Tool {
            name: Cow::Borrowed("grep"),
            title: None,
            description: Some(Cow::Borrowed(
                "Search file contents with regex. Uses ripgrep.",
            )),
            input_schema: Arc::new(
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Regex pattern to search"
                        },
                        "path": {
                            "type": "string",
                            "description": "File or directory to search"
                        },
                        "glob": {
                            "type": "string",
                            "description": "Filter files by glob (e.g., '*.rs')"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum matches (default: 50)"
                        }
                    },
                    "required": ["pattern"]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }
}

// Tool implementations - direct methods for Rig integration
impl WorkspaceTools {
    /// Read file contents with optional line range
    pub async fn read_file(
        &self,
        path: String,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let resolved = self.resolve_path(&path);

        let content = tokio::fs::read_to_string(&resolved)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Read error: {e}"), None))?;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = offset.unwrap_or(1).saturating_sub(1);
        let count = limit.unwrap_or(usize::MAX);

        let output: Vec<String> = lines
            .iter()
            .skip(start)
            .take(count)
            .enumerate()
            .map(|(i, line)| format!("{:>6}\t{}", start + i + 1, line))
            .collect();

        let result = format!(
            "{}\n\n[{} lines read, {} total]",
            output.join("\n"),
            output.len(),
            total_lines
        );

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Edit file by replacing text (old_string must match exactly)
    pub async fn edit_file(
        &self,
        path: String,
        old_string: String,
        new_string: String,
        replace_all: Option<bool>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let resolved = self.resolve_path(&path);

        let content = tokio::fs::read_to_string(&resolved)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Read error: {e}"), None))?;

        if !content.contains(&old_string) {
            return Ok(CallToolResult::success(vec![Content::text(
                "Error: old_string not found in file",
            )]));
        }

        let (new_content, count) = if replace_all.unwrap_or(false) {
            let count = content.matches(&old_string).count();
            (content.replace(&old_string, &new_string), count)
        } else {
            (content.replacen(&old_string, &new_string, 1), 1)
        };

        tokio::fs::write(&resolved, &new_content)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Write error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Replaced {count} occurrence(s)"
        ))]))
    }

    /// Write content to file (creates parent directories if needed)
    pub async fn write_file(
        &self,
        path: String,
        content: String,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let resolved = self.resolve_path(&path);

        // Create parent directories if needed
        if let Some(parent) = resolved.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| rmcp::ErrorData::internal_error(format!("Mkdir error: {e}"), None))?;
        }

        tokio::fs::write(&resolved, &content)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Write error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Written {} bytes to {}",
            content.len(),
            path
        ))]))
    }

    /// Execute bash command (use for git, npm, cargo, etc.)
    pub async fn bash(
        &self,
        command: String,
        timeout_ms: Option<u64>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let timeout =
            std::time::Duration::from_millis(timeout_ms.unwrap_or(self.default_timeout_ms));

        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(&command);
        cmd.current_dir(&self.workspace_root);

        let output = tokio::time::timeout(timeout, cmd.output())
            .await
            .map_err(|_| {
                rmcp::ErrorData::internal_error(
                    format!("Command timed out after {}ms", timeout.as_millis()),
                    None,
                )
            })?
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Exec error: {e}"), None))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let result = if output.status.success() {
            stdout.to_string()
        } else {
            format!("Exit code: {exit_code}\nStdout:\n{stdout}\nStderr:\n{stderr}")
        };

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Find files matching glob pattern (e.g., '**/*.rs')
    pub fn glob(
        &self,
        pattern: String,
        path: Option<String>,
        limit: Option<usize>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let search_path =
            path.map_or_else(|| self.workspace_root.clone(), |p| self.resolve_path(&p));

        let full_pattern = search_path.join(&pattern);
        let pattern_str = full_pattern.to_string_lossy();
        let max_results = limit.unwrap_or(100);

        let paths: Vec<String> = glob::glob(&pattern_str)
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Glob error: {e}"), None))?
            .filter_map(std::result::Result::ok)
            .take(max_results + 1)
            .map(|p| p.display().to_string())
            .collect();

        let truncated = paths.len() > max_results;
        let files: Vec<&str> = paths.iter().take(max_results).map(String::as_str).collect();

        let result = if truncated {
            format!(
                "{}\n\n[{} files, truncated at {}]",
                files.join("\n"),
                files.len(),
                max_results
            )
        } else {
            format!("{}\n\n[{} files]", files.join("\n"), files.len())
        };

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Search file contents with regex (uses ripgrep)
    pub async fn grep(
        &self,
        pattern: String,
        path: Option<String>,
        glob: Option<String>,
        limit: Option<usize>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let search_path =
            path.map_or_else(|| self.workspace_root.clone(), |p| self.resolve_path(&p));

        let max_matches = limit.unwrap_or(50);

        let mut cmd = Command::new("rg");
        cmd.arg("--line-number")
            .arg("--max-count")
            .arg("1000")
            .arg(&pattern);

        if let Some(g) = glob {
            cmd.arg("--glob").arg(g);
        }

        cmd.arg(&search_path);

        let output = cmd
            .output()
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Grep error: {e}"), None))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().take(max_matches + 1).collect();
        let truncated = lines.len() > max_matches;

        let result_lines: Vec<&str> = lines.into_iter().take(max_matches).collect();

        let result = if truncated {
            format!(
                "{}\n\n[{} matches, truncated at {}]",
                result_lines.join("\n"),
                result_lines.len(),
                max_matches
            )
        } else {
            format!(
                "{}\n\n[{} matches]",
                result_lines.join("\n"),
                result_lines.len()
            )
        };

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // Test fixtures
    // =========================================================================

    fn create_workspace() -> (TempDir, WorkspaceTools) {
        let temp = TempDir::new().unwrap();
        let tools = WorkspaceTools::new(temp.path());
        (temp, tools)
    }

    // =========================================================================
    // read_file tests
    // =========================================================================

    #[tokio::test]
    async fn test_read_file_returns_content_with_line_numbers() {
        let (temp, tools) = create_workspace();
        let file = temp.path().join("test.txt");
        tokio::fs::write(&file, "line1\nline2\nline3")
            .await
            .unwrap();

        let result = tools.read_file("test.txt".to_string(), None, None).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(!result.is_error.unwrap_or(false));

        // Check content contains line numbers
        let content = format!("{:?}", result.content);
        assert!(content.contains("line1"));
        assert!(content.contains("line2"));
        assert!(content.contains("line3"));
    }

    #[tokio::test]
    async fn test_read_file_with_offset_and_limit() {
        let (temp, tools) = create_workspace();
        let file = temp.path().join("test.txt");
        tokio::fs::write(&file, "line1\nline2\nline3\nline4\nline5")
            .await
            .unwrap();

        // Read lines 2-3 only
        let result = tools
            .read_file("test.txt".to_string(), Some(2), Some(2))
            .await;

        assert!(result.is_ok());
        let content = format!("{:?}", result.unwrap().content);
        assert!(content.contains("line2"));
        assert!(content.contains("line3"));
        assert!(!content.contains("line1")); // Should be skipped
        assert!(!content.contains("line4")); // Should be limited
    }

    #[tokio::test]
    async fn test_read_file_nonexistent_returns_error() {
        let (_temp, tools) = create_workspace();

        let result = tools
            .read_file("nonexistent.txt".to_string(), None, None)
            .await;

        assert!(result.is_err());
    }

    // =========================================================================
    // edit_file tests
    // =========================================================================

    #[tokio::test]
    async fn test_edit_file_replaces_text() {
        let (temp, tools) = create_workspace();
        let file = temp.path().join("test.txt");
        tokio::fs::write(&file, "hello world").await.unwrap();

        let result = tools
            .edit_file(
                "test.txt".to_string(),
                "world".to_string(),
                "rust".to_string(),
                None,
            )
            .await;

        assert!(result.is_ok());

        let content = tokio::fs::read_to_string(&file).await.unwrap();
        assert_eq!(content, "hello rust");
    }

    #[tokio::test]
    async fn test_edit_file_replace_all() {
        let (temp, tools) = create_workspace();
        let file = temp.path().join("test.txt");
        tokio::fs::write(&file, "foo bar foo baz foo")
            .await
            .unwrap();

        let result = tools
            .edit_file(
                "test.txt".to_string(),
                "foo".to_string(),
                "qux".to_string(),
                Some(true),
            )
            .await;

        assert!(result.is_ok());

        let content = tokio::fs::read_to_string(&file).await.unwrap();
        assert_eq!(content, "qux bar qux baz qux");
    }

    #[tokio::test]
    async fn test_edit_file_not_found_returns_message() {
        let (temp, tools) = create_workspace();
        let file = temp.path().join("test.txt");
        tokio::fs::write(&file, "hello world").await.unwrap();

        let result = tools
            .edit_file(
                "test.txt".to_string(),
                "notfound".to_string(),
                "replacement".to_string(),
                None,
            )
            .await;

        assert!(result.is_ok());
        let content = format!("{:?}", result.unwrap().content);
        assert!(content.contains("not found"));
    }

    // =========================================================================
    // write_file tests
    // =========================================================================

    #[tokio::test]
    async fn test_write_file_creates_file() {
        let (temp, tools) = create_workspace();

        let result = tools
            .write_file("new.txt".to_string(), "hello".to_string())
            .await;

        assert!(result.is_ok());

        let content = tokio::fs::read_to_string(temp.path().join("new.txt"))
            .await
            .unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_write_file_creates_parent_dirs() {
        let (temp, tools) = create_workspace();

        let result = tools
            .write_file("a/b/c/new.txt".to_string(), "nested".to_string())
            .await;

        assert!(result.is_ok());

        let content = tokio::fs::read_to_string(temp.path().join("a/b/c/new.txt"))
            .await
            .unwrap();
        assert_eq!(content, "nested");
    }

    // =========================================================================
    // bash tests
    // =========================================================================

    #[tokio::test]
    async fn test_bash_executes_command() {
        let (_temp, tools) = create_workspace();

        let result = tools.bash("echo hello".to_string(), None).await;

        assert!(result.is_ok());
        let content = format!("{:?}", result.unwrap().content);
        assert!(content.contains("hello"));
    }

    #[tokio::test]
    async fn test_bash_returns_exit_code_on_failure() {
        let (_temp, tools) = create_workspace();

        let result = tools.bash("exit 42".to_string(), None).await;

        assert!(result.is_ok());
        let content = format!("{:?}", result.unwrap().content);
        assert!(content.contains("42"));
    }

    #[tokio::test]
    async fn test_bash_timeout() {
        let (_temp, tools) = create_workspace();

        let result = tools.bash("sleep 10".to_string(), Some(100)).await;

        assert!(result.is_err());
    }

    // =========================================================================
    // glob tests
    // =========================================================================

    #[tokio::test]
    async fn test_glob_finds_files() {
        let (temp, tools) = create_workspace();
        tokio::fs::write(temp.path().join("a.rs"), "")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("b.rs"), "")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("c.txt"), "")
            .await
            .unwrap();

        let result = tools.glob("*.rs".to_string(), None, None);

        assert!(result.is_ok());
        let content = format!("{:?}", result.unwrap().content);
        assert!(content.contains("a.rs"));
        assert!(content.contains("b.rs"));
        assert!(!content.contains("c.txt"));
    }

    #[tokio::test]
    async fn test_glob_respects_limit() {
        let (temp, tools) = create_workspace();
        for i in 0..10 {
            tokio::fs::write(temp.path().join(format!("{}.rs", i)), "")
                .await
                .unwrap();
        }

        let result = tools.glob("*.rs".to_string(), None, Some(3));

        assert!(result.is_ok());
        let content = format!("{:?}", result.unwrap().content);
        assert!(content.contains("3 files"));
        assert!(content.contains("truncated"));
    }

    // =========================================================================
    // grep tests
    // =========================================================================

    #[tokio::test]
    async fn test_grep_finds_matches() {
        let (temp, tools) = create_workspace();
        tokio::fs::write(temp.path().join("test.txt"), "hello\nworld\nhello again")
            .await
            .unwrap();

        let result = tools
            .grep(
                "hello".to_string(),
                Some("test.txt".to_string()),
                None,
                None,
            )
            .await;

        assert!(result.is_ok());
        let content = format!("{:?}", result.unwrap().content);
        assert!(content.contains("hello"));
        assert!(content.contains("2 matches")); // Two lines with "hello"
    }

    #[tokio::test]
    async fn test_grep_with_glob_filter() {
        let (temp, tools) = create_workspace();
        tokio::fs::write(temp.path().join("test.rs"), "fn main() {}")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("test.txt"), "fn in txt")
            .await
            .unwrap();

        let result = tools
            .grep("fn".to_string(), None, Some("*.rs".to_string()), None)
            .await;

        assert!(result.is_ok());
        let content = format!("{:?}", result.unwrap().content);
        assert!(content.contains("test.rs"));
        assert!(!content.contains("test.txt"));
    }

    // =========================================================================
    // tool_definitions tests
    // =========================================================================

    #[test]
    fn test_tool_definitions_returns_all_tools() {
        let defs = WorkspaceTools::tool_definitions();

        assert_eq!(defs.len(), 6);

        let names: Vec<&str> = defs.iter().map(|t| t.name.as_ref()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"glob"));
        assert!(names.contains(&"grep"));
    }

    #[test]
    fn test_tool_definitions_have_descriptions() {
        let defs = WorkspaceTools::tool_definitions();

        for def in defs {
            assert!(
                def.description.is_some(),
                "Tool {} should have description",
                def.name
            );
        }
    }
}
