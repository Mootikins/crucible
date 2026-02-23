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
use crucible_core::events::SessionEvent;
use crucible_core::interaction::AskRequest;
use crucible_core::interaction::InteractionRequest;
use crucible_core::interaction_context::InteractionContext;
use crucible_tools::WorkspaceTools;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use thiserror::Error;
use uuid::Uuid;

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
    delegation_enabled: Arc<RwLock<bool>>,
    delegation_targets: Vec<String>,
    delegation_depth: Arc<RwLock<u32>>,
    background_spawner: Option<Arc<dyn BackgroundSpawner>>,
    interaction_context: Option<Arc<InteractionContext>>,
}

impl WorkspaceContext {
    /// Create a new workspace context
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            tools: Arc::new(WorkspaceTools::new(workspace_root)),
            mode_id: Arc::new(RwLock::new("auto".to_string())),
            session_id: Arc::new(RwLock::new(None)),
            delegation_enabled: Arc::new(RwLock::new(true)),
            delegation_targets: Vec::new(),
            delegation_depth: Arc::new(RwLock::new(0)),
            background_spawner: None,
            interaction_context: None,
        }
    }

    /// Set pre-resolved delegation target names for tool schemas.
    pub fn with_delegation_targets(mut self, targets: Vec<String>) -> Self {
        self.delegation_targets = targets;
        self
    }

    /// Set the background task spawner
    pub fn with_background_spawner(mut self, spawner: Arc<dyn BackgroundSpawner>) -> Self {
        self.background_spawner = Some(spawner);
        self
    }

    /// Set the interaction context for user interactions
    pub fn with_interaction_context(mut self, ctx: Arc<InteractionContext>) -> Self {
        self.interaction_context = Some(ctx);
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

    /// Enable or disable delegation tools for the current session.
    pub fn set_delegation_enabled(&self, enabled: bool) {
        if let Ok(mut guard) = self.delegation_enabled.write() {
            *guard = enabled;
        }
    }

    /// Returns whether delegation is enabled for the current session.
    pub fn is_delegation_enabled(&self) -> bool {
        self.delegation_enabled
            .read()
            .map(|guard| *guard)
            .unwrap_or(false)
    }

    /// Set the current delegation depth for this session.
    pub fn set_delegation_depth(&self, depth: u32) {
        if let Ok(mut guard) = self.delegation_depth.write() {
            *guard = depth;
        }
    }

    /// Get the current delegation depth for this session.
    pub fn delegation_depth(&self) -> u32 {
        self.delegation_depth
            .read()
            .map(|guard| *guard)
            .unwrap_or(0)
    }

    /// Check if write operations are blocked (plan mode)
    pub fn is_write_blocked(&self) -> bool {
        self.mode_id
            .read()
            .map(|guard| *guard == "plan")
            .unwrap_or(false)
    }

    /// Check if a background task spawner is configured
    pub fn has_background_spawner(&self) -> bool {
        self.background_spawner.is_some()
    }

    /// Check if an interaction context is configured
    pub fn has_interaction_context(&self) -> bool {
        self.interaction_context.is_some()
    }

    /// Get the interaction context if configured
    pub fn interaction_context(&self) -> Option<Arc<InteractionContext>> {
        self.interaction_context.clone()
    }

    /// Get all available workspace tools.
    pub fn all_tools(&self) -> Vec<Box<dyn rig::tool::ToolDyn>> {
        let tools: Vec<Box<dyn rig::tool::ToolDyn>> = vec![
            Box::new(ReadFileTool::new(self.clone())),
            Box::new(EditFileTool::new(self.clone())),
            Box::new(WriteFileTool::new(self.clone())),
            Box::new(BashTool::new(self.clone())),
            Box::new(GlobTool::new(self.clone())),
            Box::new(GrepTool::new(self.clone())),
        ];

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
                        "description": "Run in background (returns job_id immediately)"
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
                WorkspaceToolError::Command("Background execution requires session_id".to_string())
            })?;

            let timeout = args.timeout_ms.map(std::time::Duration::from_millis);

            let task_id = spawner
                .spawn_bash(&session_id, args.command.clone(), None, timeout)
                .await
                .map_err(|e| {
                    WorkspaceToolError::Command(format!("Failed to spawn background job: {}", e))
                })?;

            return Ok(format!(
                "Job spawned in background. job_id: {}\nUse list_jobs to check status.",
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
// AskUserTool
// =============================================================================

/// Arguments for asking the user a question
#[derive(Debug, Deserialize)]
pub struct AskUserArgs {
    /// The question to ask
    question: String,
    /// Optional list of choices
    #[serde(default)]
    choices: Option<Vec<String>>,
    /// Allow multiple selections
    #[serde(default)]
    multi_select: bool,
    /// Allow free-text input
    #[serde(default)]
    allow_other: bool,
}

/// Tool for asking the user a question and waiting for their response
#[derive(Clone, Serialize, Deserialize)]
pub struct AskUserTool {
    #[serde(skip)]
    ctx: Option<InteractionContext>,
}

impl AskUserTool {
    /// Create a new AskUserTool
    pub fn new(ctx: InteractionContext) -> Self {
        Self { ctx: Some(ctx) }
    }
}

impl Tool for AskUserTool {
    const NAME: &'static str = "ask_user";
    type Error = WorkspaceToolError;
    type Args = AskUserArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Ask the user a question and wait for their response".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The question to ask"
                    },
                    "choices": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of choices"
                    },
                    "multi_select": {
                        "type": "boolean",
                        "description": "Allow multiple selections"
                    },
                    "allow_other": {
                        "type": "boolean",
                        "description": "Allow free-text input"
                    }
                },
                "required": ["question"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = self.ctx.as_ref().ok_or_else(|| {
            WorkspaceToolError::Blocked("Tool not initialized with context".to_string())
        })?;

        // Generate request ID
        let id = Uuid::new_v4();

        // Register receiver with the registry
        let rx = {
            let mut registry = ctx.registry.lock().await;
            registry.register(id)
        };

        // Build AskRequest
        let mut request = AskRequest::new(args.question);
        if let Some(choices) = args.choices {
            request = request.choices(choices);
        }
        if args.multi_select {
            request = request.multi_select();
        }
        if args.allow_other {
            request = request.allow_other();
        }

        // Emit InteractionRequested event
        (ctx.push_event)(SessionEvent::InteractionRequested {
            request_id: id.to_string(),
            request: InteractionRequest::Ask(request),
        });

        // Await response from user
        let response = rx.await.map_err(|_| {
            WorkspaceToolError::Blocked("Interaction was cancelled or dropped".to_string())
        })?;

        // Convert response to JSON
        match response {
            crucible_core::interaction::InteractionResponse::Ask(ask_response) => {
                let json = serde_json::to_string(&ask_response).map_err(|e| {
                    WorkspaceToolError::Blocked(format!("Failed to serialize response: {}", e))
                })?;
                Ok(json)
            }
            crucible_core::interaction::InteractionResponse::Cancelled => Err(
                WorkspaceToolError::Blocked("User cancelled the interaction".to_string()),
            ),
            _ => Err(WorkspaceToolError::Blocked(
                "Unexpected response type".to_string(),
            )),
        }
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
