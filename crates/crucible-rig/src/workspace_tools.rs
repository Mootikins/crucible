//! Rig-compatible workspace tools
//!
//! This module provides Rig `Tool` trait implementations for workspace operations.
//! These wrap the core `WorkspaceTools` to work seamlessly with Rig agents.
//!
//! ## Available Tools
//!
//! - `ReadFileTool` - Read file contents with optional line range
//! - `EditFileTool` - Edit file via search/replace
//! - `WriteFileTool` - Write content to file
//! - `BashTool` - Execute shell commands
//! - `GlobTool` - Find files by pattern
//! - `GrepTool` - Search file contents with regex

use crucible_core::background::BackgroundSpawner;
use crucible_tools::WorkspaceTools;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use thiserror::Error;

/// Error type for workspace tool operations
#[derive(Debug, Error)]
pub enum WorkspaceToolError {
    /// File operation failed
    #[error("File error: {0}")]
    File(String),

    /// Command execution failed
    #[error("Command error: {0}")]
    Command(String),

    /// Pattern matching failed
    #[error("Pattern error: {0}")]
    Pattern(String),

    /// Operation blocked by current mode
    #[error("Blocked: {0}")]
    Blocked(String),
}

/// Shared workspace context for tools
#[derive(Clone)]
pub struct WorkspaceContext {
    tools: Arc<WorkspaceTools>,
    mode_id: Arc<RwLock<String>>,
    session_id: Arc<RwLock<Option<String>>>,
    background_spawner: Option<Arc<dyn BackgroundSpawner>>,
}

impl WorkspaceContext {
    /// Create a new workspace context
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            tools: Arc::new(WorkspaceTools::new(workspace_root)),
            mode_id: Arc::new(RwLock::new("auto".to_string())),
            session_id: Arc::new(RwLock::new(None)),
            background_spawner: None,
        }
    }

    /// Set the background task spawner
    pub fn with_background_spawner(mut self, spawner: Arc<dyn BackgroundSpawner>) -> Self {
        self.background_spawner = Some(spawner);
        self
    }

    /// Set the session ID for background task tracking
    pub fn set_session_id(&self, session_id: &str) {
        if let Ok(mut guard) = self.session_id.write() {
            *guard = Some(session_id.to_string());
        }
    }

    /// Get the current session ID
    pub fn session_id(&self) -> Option<String> {
        self.session_id.read().ok().and_then(|g| g.clone())
    }

    /// Set the current mode (plan/act/auto)
    pub fn set_mode(&self, mode_id: &str) {
        if let Ok(mut guard) = self.mode_id.write() {
            *guard = mode_id.to_string();
        }
    }

    /// Check if write operations are blocked (plan mode)
    pub fn is_write_blocked(&self) -> bool {
        self.mode_id
            .read()
            .map(|guard| *guard == "plan")
            .unwrap_or(false)
    }

    pub fn all_tools(&self) -> Vec<Box<dyn rig::tool::ToolDyn>> {
        let mut tools: Vec<Box<dyn rig::tool::ToolDyn>> = vec![
            Box::new(ReadFileTool::new(self.clone())),
            Box::new(EditFileTool::new(self.clone())),
            Box::new(WriteFileTool::new(self.clone())),
            Box::new(BashTool::new(self.clone())),
            Box::new(GlobTool::new(self.clone())),
            Box::new(GrepTool::new(self.clone())),
        ];

        if self.background_spawner.is_some() {
            tools.push(Box::new(ListBackgroundTasksTool::new(self.clone())));
            tools.push(Box::new(GetTaskResultTool::new(self.clone())));
            tools.push(Box::new(CancelTaskTool::new(self.clone())));
            tools.push(Box::new(SpawnSubagentTool::new(self.clone())));
        }

        tools
    }

    /// Get read-only tools for small models
    ///
    /// Returns only: read_file, glob, grep
    /// Excludes write operations to reduce confusion for smaller models
    pub fn read_only_tools(&self) -> Vec<Box<dyn rig::tool::ToolDyn>> {
        vec![
            Box::new(ReadFileTool::new(self.clone())),
            Box::new(GlobTool::new(self.clone())),
            Box::new(GrepTool::new(self.clone())),
        ]
    }

    /// Get tools based on model size
    ///
    /// Small models get read-only tools only (read_file, glob, grep)
    /// Medium and large models get all tools
    pub fn tools_for_size(
        &self,
        size: crucible_core::prompts::ModelSize,
    ) -> Vec<Box<dyn rig::tool::ToolDyn>> {
        if size.is_read_only() {
            self.read_only_tools()
        } else {
            self.all_tools()
        }
    }

    /// Get tools based on mode
    ///
    /// - `plan` mode: read-only tools (read_file, glob, grep)
    /// - `normal`/`auto` mode: all tools
    pub fn tools_for_mode(&self, mode_id: &str) -> Vec<Box<dyn rig::tool::ToolDyn>> {
        match mode_id {
            "plan" => self.read_only_tools(),
            _ => self.all_tools(),
        }
    }

    /// Get tools based on both model size and mode
    ///
    /// Returns the intersection - if either model size OR mode restricts tools,
    /// returns the restricted set.
    pub fn tools_for_size_and_mode(
        &self,
        size: crucible_core::prompts::ModelSize,
        mode_id: &str,
    ) -> Vec<Box<dyn rig::tool::ToolDyn>> {
        if size.is_read_only() || mode_id == "plan" {
            self.read_only_tools()
        } else {
            self.all_tools()
        }
    }
}

// =============================================================================
// ReadFileTool
// =============================================================================

/// Arguments for reading a file
#[derive(Debug, Deserialize)]
pub struct ReadFileArgs {
    /// Path to file (absolute or relative to workspace)
    path: String,
    /// Line number to start from (1-indexed)
    offset: Option<usize>,
    /// Maximum lines to read
    limit: Option<usize>,
}

/// Tool for reading file contents
#[derive(Clone, Serialize, Deserialize)]
pub struct ReadFileTool {
    #[serde(skip)]
    ctx: Option<WorkspaceContext>,
}

impl ReadFileTool {
    /// Create a new ReadFileTool
    pub fn new(ctx: WorkspaceContext) -> Self {
        Self { ctx: Some(ctx) }
    }
}

impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";
    type Error = WorkspaceToolError;
    type Args = ReadFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Read file contents. Returns content with line numbers.".to_string(),
            parameters: json!({
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
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = self.ctx.as_ref().ok_or_else(|| {
            WorkspaceToolError::File("Tool not initialized with context".to_string())
        })?;

        let result = ctx
            .tools
            .read_file(args.path, args.offset, args.limit)
            .await
            .map_err(|e| WorkspaceToolError::File(e.message.to_string()))?;

        // Extract text content from the result
        extract_text_content(&result)
    }
}

// =============================================================================
// EditFileTool
// =============================================================================

/// Arguments for editing a file
#[derive(Debug, Deserialize)]
pub struct EditFileArgs {
    /// Path to file
    path: String,
    /// Text to find and replace
    old_string: String,
    /// Replacement text
    new_string: String,
    /// Replace all occurrences (default: false)
    replace_all: Option<bool>,
}

/// Tool for editing files via search/replace
#[derive(Clone, Serialize, Deserialize)]
pub struct EditFileTool {
    #[serde(skip)]
    ctx: Option<WorkspaceContext>,
}

impl EditFileTool {
    /// Create a new EditFileTool
    pub fn new(ctx: WorkspaceContext) -> Self {
        Self { ctx: Some(ctx) }
    }
}

impl Tool for EditFileTool {
    const NAME: &'static str = "edit_file";
    type Error = WorkspaceToolError;
    type Args = EditFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Edit file by replacing text. old_string must match exactly.".to_string(),
            parameters: json!({
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
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = self.ctx.as_ref().ok_or_else(|| {
            WorkspaceToolError::File("Tool not initialized with context".to_string())
        })?;

        if ctx.is_write_blocked() {
            return Err(WorkspaceToolError::Blocked(
                "edit_file is blocked in plan mode".to_string(),
            ));
        }

        let result = ctx
            .tools
            .edit_file(
                args.path,
                args.old_string,
                args.new_string,
                args.replace_all,
            )
            .await
            .map_err(|e| WorkspaceToolError::File(e.message.to_string()))?;

        extract_text_content(&result)
    }
}

// =============================================================================
// WriteFileTool
// =============================================================================

/// Arguments for writing a file
#[derive(Debug, Deserialize)]
pub struct WriteFileArgs {
    /// Path to file
    path: String,
    /// Content to write
    content: String,
}

/// Tool for writing content to files
#[derive(Clone, Serialize, Deserialize)]
pub struct WriteFileTool {
    #[serde(skip)]
    ctx: Option<WorkspaceContext>,
}

impl WriteFileTool {
    /// Create a new WriteFileTool
    pub fn new(ctx: WorkspaceContext) -> Self {
        Self { ctx: Some(ctx) }
    }
}

impl Tool for WriteFileTool {
    const NAME: &'static str = "write_file";
    type Error = WorkspaceToolError;
    type Args = WriteFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Write content to file. Creates parent directories if needed.".to_string(),
            parameters: json!({
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
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = self.ctx.as_ref().ok_or_else(|| {
            WorkspaceToolError::File("Tool not initialized with context".to_string())
        })?;

        if ctx.is_write_blocked() {
            return Err(WorkspaceToolError::Blocked(
                "write_file is blocked in plan mode".to_string(),
            ));
        }

        let result = ctx
            .tools
            .write_file(args.path, args.content)
            .await
            .map_err(|e| WorkspaceToolError::File(e.message.to_string()))?;

        extract_text_content(&result)
    }
}

// =============================================================================
// BashTool
// =============================================================================

/// Arguments for `bash` tool.
#[derive(Debug, Deserialize)]
pub struct BashArgs {
    command: String,
    timeout_ms: Option<u64>,
    #[serde(default)]
    background: bool,
}

/// Tool for executing bash commands
#[derive(Clone, Serialize, Deserialize)]
pub struct BashTool {
    #[serde(skip)]
    ctx: Option<WorkspaceContext>,
}

impl BashTool {
    /// Create a new BashTool
    pub fn new(ctx: WorkspaceContext) -> Self {
        Self { ctx: Some(ctx) }
    }
}

impl Tool for BashTool {
    const NAME: &'static str = "bash";
    type Error = WorkspaceToolError;
    type Args = BashArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Execute bash command. Use for git, npm, cargo, etc. Set background=true for long-running commands.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Bash command to execute"
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Timeout in milliseconds (default: 120000)"
                    },
                    "background": {
                        "type": "boolean",
                        "description": "Run in background (returns task_id immediately)"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = self.ctx.as_ref().ok_or_else(|| {
            WorkspaceToolError::Command("Tool not initialized with context".to_string())
        })?;

        if ctx.is_write_blocked() {
            return Err(WorkspaceToolError::Blocked(
                "bash is blocked in plan mode".to_string(),
            ));
        }

        if args.background {
            let spawner = ctx.background_spawner.as_ref().ok_or_else(|| {
                WorkspaceToolError::Command(
                    "Background execution not available (no spawner configured)".to_string(),
                )
            })?;

            let session_id = ctx.session_id().ok_or_else(|| {
                WorkspaceToolError::Command(
                    "Background execution requires session_id".to_string(),
                )
            })?;

            let timeout = args.timeout_ms.map(std::time::Duration::from_millis);

            let task_id = spawner
                .spawn_bash(&session_id, args.command.clone(), None, timeout)
                .await
                .map_err(|e| WorkspaceToolError::Command(format!("Failed to spawn background task: {}", e)))?;

            return Ok(format!(
                "Task spawned in background. task_id: {}\nUse list_background_tasks to check status.",
                task_id
            ));
        }

        let result = ctx
            .tools
            .bash(args.command, args.timeout_ms)
            .await
            .map_err(|e| WorkspaceToolError::Command(e.message.to_string()))?;

        extract_text_content(&result)
    }
}

// =============================================================================
// GlobTool
// =============================================================================

/// Arguments for glob pattern matching
#[derive(Debug, Deserialize)]
pub struct GlobArgs {
    /// Glob pattern (e.g., '**/*.rs')
    pattern: String,
    /// Directory to search (default: workspace root)
    path: Option<String>,
    /// Maximum results (default: 100)
    limit: Option<usize>,
}

/// Tool for finding files by glob pattern
#[derive(Clone, Serialize, Deserialize)]
pub struct GlobTool {
    #[serde(skip)]
    ctx: Option<WorkspaceContext>,
}

impl GlobTool {
    /// Create a new GlobTool
    pub fn new(ctx: WorkspaceContext) -> Self {
        Self { ctx: Some(ctx) }
    }
}

impl Tool for GlobTool {
    const NAME: &'static str = "glob";
    type Error = WorkspaceToolError;
    type Args = GlobArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Find files matching glob pattern (e.g., '**/*.rs').".to_string(),
            parameters: json!({
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
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = self.ctx.as_ref().ok_or_else(|| {
            WorkspaceToolError::Pattern("Tool not initialized with context".to_string())
        })?;

        let result = ctx
            .tools
            .glob(args.pattern, args.path, args.limit)
            .map_err(|e| WorkspaceToolError::Pattern(e.message.to_string()))?;

        extract_text_content(&result)
    }
}

// =============================================================================
// GrepTool
// =============================================================================

/// Arguments for grep search
#[derive(Debug, Deserialize)]
pub struct GrepArgs {
    /// Regex pattern to search
    pattern: String,
    /// File or directory to search
    path: Option<String>,
    /// Filter files by glob (e.g., '*.rs')
    glob: Option<String>,
    /// Maximum matches (default: 50)
    limit: Option<usize>,
}

/// Tool for searching file contents with regex
#[derive(Clone, Serialize, Deserialize)]
pub struct GrepTool {
    #[serde(skip)]
    ctx: Option<WorkspaceContext>,
}

impl GrepTool {
    /// Create a new GrepTool
    pub fn new(ctx: WorkspaceContext) -> Self {
        Self { ctx: Some(ctx) }
    }
}

impl Tool for GrepTool {
    const NAME: &'static str = "grep";
    type Error = WorkspaceToolError;
    type Args = GrepArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search file contents with regex. Uses ripgrep.".to_string(),
            parameters: json!({
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
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = self.ctx.as_ref().ok_or_else(|| {
            WorkspaceToolError::Pattern("Tool not initialized with context".to_string())
        })?;

        let result = ctx
            .tools
            .grep(args.pattern, args.path, args.glob, args.limit)
            .await
            .map_err(|e| WorkspaceToolError::Pattern(e.message.to_string()))?;

        extract_text_content(&result)
    }
}

// =============================================================================
// ListBackgroundTasksTool
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct ListBackgroundTasksArgs {
    #[serde(default)]
    filter: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ListBackgroundTasksTool {
    #[serde(skip)]
    ctx: Option<WorkspaceContext>,
}

impl ListBackgroundTasksTool {
    pub fn new(ctx: WorkspaceContext) -> Self {
        Self { ctx: Some(ctx) }
    }
}

impl Tool for ListBackgroundTasksTool {
    const NAME: &'static str = "list_background_tasks";
    type Error = WorkspaceToolError;
    type Args = ListBackgroundTasksArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "List background tasks (running and completed) for this session.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "filter": {
                        "type": "string",
                        "description": "Filter: 'all' (default), 'running', or 'completed'",
                        "enum": ["all", "running", "completed"]
                    }
                },
                "required": []
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = self.ctx.as_ref().ok_or_else(|| {
            WorkspaceToolError::Command("Tool not initialized with context".to_string())
        })?;

        let spawner = ctx.background_spawner.as_ref().ok_or_else(|| {
            WorkspaceToolError::Command("Background task manager not available".to_string())
        })?;

        let session_id = ctx.session_id().ok_or_else(|| {
            WorkspaceToolError::Command("No session ID available".to_string())
        })?;

        let tasks = spawner.list_tasks(&session_id);
        let filter = args.filter.as_deref().unwrap_or("all");

        let filtered: Vec<_> = tasks
            .into_iter()
            .filter(|t| match filter {
                "running" => !t.status.is_terminal(),
                "completed" => t.status.is_terminal(),
                _ => true,
            })
            .collect();

        if filtered.is_empty() {
            return Ok("No background tasks found.".to_string());
        }

        let mut output = String::new();
        for task in filtered {
            let duration = task
                .duration()
                .map(|d| format!(" ({}s)", d.num_seconds()))
                .unwrap_or_default();
            output.push_str(&format!(
                "- {} [{}] {}{}\n",
                task.id,
                task.status,
                task.kind.summary(),
                duration
            ));
        }

        Ok(output)
    }
}

// =============================================================================
// GetTaskResultTool
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct GetTaskResultArgs {
    task_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GetTaskResultTool {
    #[serde(skip)]
    ctx: Option<WorkspaceContext>,
}

impl GetTaskResultTool {
    pub fn new(ctx: WorkspaceContext) -> Self {
        Self { ctx: Some(ctx) }
    }
}

impl Tool for GetTaskResultTool {
    const NAME: &'static str = "get_task_result";
    type Error = WorkspaceToolError;
    type Args = GetTaskResultArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Get the result of a background task.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "string",
                        "description": "The task ID to get results for"
                    }
                },
                "required": ["task_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = self.ctx.as_ref().ok_or_else(|| {
            WorkspaceToolError::Command("Tool not initialized with context".to_string())
        })?;

        let spawner = ctx.background_spawner.as_ref().ok_or_else(|| {
            WorkspaceToolError::Command("Background task manager not available".to_string())
        })?;

        let result = spawner.get_task_result(&args.task_id).ok_or_else(|| {
            WorkspaceToolError::Command(format!("Task not found: {}", args.task_id))
        })?;

        let mut output = format!("Task: {}\nStatus: {}\nKind: {}\n", 
            result.info.id, 
            result.info.status, 
            result.info.kind.name()
        );

        if let Some(duration) = result.info.duration() {
            output.push_str(&format!("Duration: {}s\n", duration.num_seconds()));
        }

        if let Some(ref out) = result.output {
            output.push_str(&format!("\nOutput:\n{}\n", out));
        }

        if let Some(ref err) = result.error {
            output.push_str(&format!("\nError:\n{}\n", err));
        }

        if let Some(code) = result.exit_code {
            output.push_str(&format!("\nExit code: {}\n", code));
        }

        Ok(output)
    }
}

// =============================================================================
// CancelTaskTool
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct CancelTaskArgs {
    task_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CancelTaskTool {
    #[serde(skip)]
    ctx: Option<WorkspaceContext>,
}

impl CancelTaskTool {
    pub fn new(ctx: WorkspaceContext) -> Self {
        Self { ctx: Some(ctx) }
    }
}

impl Tool for CancelTaskTool {
    const NAME: &'static str = "cancel_task";
    type Error = WorkspaceToolError;
    type Args = CancelTaskArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Cancel a running background task.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "string",
                        "description": "The task ID to cancel"
                    }
                },
                "required": ["task_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = self.ctx.as_ref().ok_or_else(|| {
            WorkspaceToolError::Command("Tool not initialized with context".to_string())
        })?;

        let spawner = ctx.background_spawner.as_ref().ok_or_else(|| {
            WorkspaceToolError::Command("Background task manager not available".to_string())
        })?;

        let cancelled = spawner.cancel_task(&args.task_id).await;

        if cancelled {
            Ok(format!("Task {} cancelled successfully.", args.task_id))
        } else {
            Err(WorkspaceToolError::Command(format!(
                "Task {} not found or already completed.",
                args.task_id
            )))
        }
    }
}

// =============================================================================
// SpawnSubagentTool
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct SpawnSubagentArgs {
    prompt: String,
    #[serde(default)]
    context: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SpawnSubagentTool {
    #[serde(skip)]
    ctx: Option<WorkspaceContext>,
}

impl SpawnSubagentTool {
    pub fn new(ctx: WorkspaceContext) -> Self {
        Self { ctx: Some(ctx) }
    }
}

impl Tool for SpawnSubagentTool {
    const NAME: &'static str = "spawn_subagent";
    type Error = WorkspaceToolError;
    type Args = SpawnSubagentArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Spawn a background subagent to work on a task autonomously.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "The task for the subagent to complete"
                    },
                    "context": {
                        "type": "string",
                        "description": "Additional context to provide to the subagent"
                    }
                },
                "required": ["prompt"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = self.ctx.as_ref().ok_or_else(|| {
            WorkspaceToolError::Command("Tool not initialized with context".to_string())
        })?;

        let spawner = ctx.background_spawner.as_ref().ok_or_else(|| {
            WorkspaceToolError::Command("Background task spawning not available".to_string())
        })?;

        let session_id = ctx.session_id().ok_or_else(|| {
            WorkspaceToolError::Command("No session ID available".to_string())
        })?;

        let task_id = spawner
            .spawn_subagent(&session_id, args.prompt.clone(), args.context)
            .await
            .map_err(|e| WorkspaceToolError::Command(format!("Failed to spawn subagent: {}", e)))?;

        Ok(format!(
            "Subagent spawned in background. task_id: {}\nUse list_background_tasks to check status, get_task_result to get output.",
            task_id
        ))
    }
}

// =============================================================================
// Helper functions
// =============================================================================

fn extract_text_content(
    result: &rmcp::model::CallToolResult,
) -> Result<String, WorkspaceToolError> {
    for content in &result.content {
        if let Some(text) = content.as_text() {
            return Ok(text.text.to_string());
        }
    }

    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_context() -> (TempDir, WorkspaceContext) {
        let temp = TempDir::new().unwrap();
        let ctx = WorkspaceContext::new(temp.path());
        (temp, ctx)
    }

    #[tokio::test]
    async fn test_read_file_tool_definition() {
        let (_temp, ctx) = create_test_context();
        let tool = ReadFileTool::new(ctx);

        let def = tool.definition("test".to_string()).await;
        assert_eq!(def.name, "read_file");
        assert!(def.description.contains("Read file"));
    }

    #[tokio::test]
    async fn test_read_file_tool_call() {
        let (temp, ctx) = create_test_context();
        let tool = ReadFileTool::new(ctx);

        // Create a test file
        let file_path = temp.path().join("test.txt");
        tokio::fs::write(&file_path, "line1\nline2\nline3")
            .await
            .unwrap();

        let args = ReadFileArgs {
            path: "test.txt".to_string(),
            offset: None,
            limit: None,
        };

        let result = tool.call(args).await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("line1"));
        assert!(content.contains("line2"));
    }

    #[tokio::test]
    async fn test_edit_file_tool_call() {
        let (temp, ctx) = create_test_context();
        let tool = EditFileTool::new(ctx.clone());

        // Create a test file
        let file_path = temp.path().join("test.txt");
        tokio::fs::write(&file_path, "hello world").await.unwrap();

        let args = EditFileArgs {
            path: "test.txt".to_string(),
            old_string: "world".to_string(),
            new_string: "rust".to_string(),
            replace_all: None,
        };

        let result = tool.call(args).await;
        assert!(result.is_ok());

        // Verify the file was modified
        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "hello rust");
    }

    #[tokio::test]
    async fn test_write_file_tool_call() {
        let (temp, ctx) = create_test_context();
        let tool = WriteFileTool::new(ctx);

        let args = WriteFileArgs {
            path: "new_file.txt".to_string(),
            content: "hello from rig".to_string(),
        };

        let result = tool.call(args).await;
        assert!(result.is_ok());

        // Verify the file was created
        let content = tokio::fs::read_to_string(temp.path().join("new_file.txt"))
            .await
            .unwrap();
        assert_eq!(content, "hello from rig");
    }

    #[tokio::test]
    async fn test_bash_tool_call() {
        let (_temp, ctx) = create_test_context();
        let tool = BashTool::new(ctx);

        let args = BashArgs {
            command: "echo hello".to_string(),
            timeout_ms: None,
            background: false,
        };

        let result = tool.call(args).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn test_bash_background_requires_spawner() {
        let (_temp, ctx) = create_test_context();
        let tool = BashTool::new(ctx);

        let args = BashArgs {
            command: "echo hello".to_string(),
            timeout_ms: None,
            background: true,
        };

        let result = tool.call(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, WorkspaceToolError::Command(_)));
    }

    #[tokio::test]
    async fn test_glob_tool_call() {
        let (temp, ctx) = create_test_context();
        let tool = GlobTool::new(ctx);

        // Create some test files
        tokio::fs::write(temp.path().join("a.rs"), "")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("b.rs"), "")
            .await
            .unwrap();

        let args = GlobArgs {
            pattern: "*.rs".to_string(),
            path: None,
            limit: None,
        };

        let result = tool.call(args).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("a.rs"));
        assert!(output.contains("b.rs"));
    }

    #[tokio::test]
    #[ignore = "requires ripgrep"]
    async fn test_grep_tool_call() {
        let (temp, ctx) = create_test_context();
        let tool = GrepTool::new(ctx);

        // Create a test file
        tokio::fs::write(temp.path().join("test.txt"), "hello\nworld\nhello again")
            .await
            .unwrap();

        let args = GrepArgs {
            pattern: "hello".to_string(),
            path: Some("test.txt".to_string()),
            glob: None,
            limit: None,
        };

        let result = tool.call(args).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("hello"));
    }

    #[test]
    fn test_workspace_context_all_tools() {
        let temp = TempDir::new().unwrap();
        let ctx = WorkspaceContext::new(temp.path());
        let tools = ctx.all_tools();

        assert_eq!(tools.len(), 6);
    }

    #[test]
    fn test_workspace_context_read_only_tools() {
        let temp = TempDir::new().unwrap();
        let ctx = WorkspaceContext::new(temp.path());
        let tools = ctx.read_only_tools();

        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn test_workspace_context_tools_for_size() {
        use crucible_core::prompts::ModelSize;

        let temp = TempDir::new().unwrap();
        let ctx = WorkspaceContext::new(temp.path());

        // Small models get read-only tools (3)
        let small_tools = ctx.tools_for_size(ModelSize::Small);
        assert_eq!(small_tools.len(), 3);

        // Medium models get all tools (6)
        let medium_tools = ctx.tools_for_size(ModelSize::Medium);
        assert_eq!(medium_tools.len(), 6);

        // Large models get all tools (6)
        let large_tools = ctx.tools_for_size(ModelSize::Large);
        assert_eq!(large_tools.len(), 6);
    }
}
