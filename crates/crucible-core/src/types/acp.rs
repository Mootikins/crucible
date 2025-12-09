//! ACP domain types for cross-crate handoff
//!
//! This module contains concrete data structures used across the ACP integration.
//! These types are implementation-independent and serve as the "lingua franca"
//! between crucible-core, crucible-acp, and crucible-cli.
//!
//! ## Design Principles
//!
//! - **Pure data**: No business logic, just structure
//! - **Serializable**: All types support serde for persistence/transport
//! - **Cross-crate**: Designed for use across module boundaries
//! - **Associated types**: Used as concrete types in trait implementations
//!
//! ## Organization
//!
//! - **Session types**: SessionId, SessionConfig, ChatMode
//! - **Tool types**: ToolDescriptor, ToolInvocation, ToolOutput
//! - **Stream types**: StreamChunk, StreamMetadata
//! - **Filesystem types**: FileMetadata

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

// ============================================================================
// Session Types
// ============================================================================

/// Session identifier
///
/// Uniquely identifies an ACP session. Wraps a UUID for type safety.
///
/// # Example
///
/// ```rust
/// use crucible_core::types::acp::SessionId;
///
/// let session = SessionId::new();
/// println!("Session: {}", session);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(Uuid);

impl SessionId {
    /// Create a new random session ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a session ID from a UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the underlying UUID
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Get the session ID as a string
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for SessionId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<SessionId> for Uuid {
    fn from(id: SessionId) -> Self {
        id.0
    }
}

/// Session configuration
///
/// Configuration for creating a new ACP session, including working directory,
/// chat mode, and context limits.
///
/// # Example
///
/// ```rust
/// use crucible_core::types::acp::{SessionConfig, ChatMode};
/// use std::path::PathBuf;
///
/// let config = SessionConfig::new(PathBuf::from("/workspace"))
///     .with_mode(ChatMode::Plan)
///     .with_context_size(100_000);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Current working directory for the session
    pub cwd: PathBuf,

    /// Chat mode (plan/act)
    pub mode: ChatMode,

    /// Maximum context size in tokens
    pub context_size: usize,

    /// Whether to enable context enrichment (semantic search)
    pub enable_enrichment: bool,

    /// Number of semantic search results to include
    pub enrichment_count: usize,

    /// Additional session metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl SessionConfig {
    /// Create a new session configuration with defaults
    ///
    /// # Arguments
    ///
    /// * `cwd` - Current working directory for the session
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            mode: ChatMode::Plan,
            context_size: 100_000,
            enable_enrichment: true,
            enrichment_count: 5,
            metadata: HashMap::new(),
        }
    }

    /// Set the chat mode
    pub fn with_mode(mut self, mode: ChatMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the context size
    pub fn with_context_size(mut self, size: usize) -> Self {
        self.context_size = size;
        self
    }

    /// Enable or disable context enrichment
    pub fn with_enrichment(mut self, enabled: bool) -> Self {
        self.enable_enrichment = enabled;
        self
    }

    /// Set the number of enrichment results
    pub fn with_enrichment_count(mut self, count: usize) -> Self {
        self.enrichment_count = count;
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self::new(PathBuf::from("."))
    }
}

/// Chat mode for ACP sessions
///
/// Determines the permission level for agent operations:
/// - **Plan**: Read-only mode for exploration and planning
/// - **Act**: Read-write mode for execution and modification
///
/// # Permission Model
///
/// | Operation | Plan Mode | Act Mode |
/// |-----------|-----------|----------|
/// | Read files | ✅ | ✅ |
/// | Write files | ❌ | ✅ |
/// | List files | ✅ | ✅ |
/// | Execute tools | ✅ (read-only) | ✅ (all) |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatMode {
    /// Read-only mode (exploration, analysis, planning)
    Plan,

    /// Read-write mode (execution, modification, creation)
    Act,
}

impl ChatMode {
    /// Check if this mode allows write operations
    pub fn can_write(&self) -> bool {
        matches!(self, ChatMode::Act)
    }

    /// Check if this mode is read-only
    pub fn is_read_only(&self) -> bool {
        matches!(self, ChatMode::Plan)
    }
}

impl std::fmt::Display for ChatMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMode::Plan => write!(f, "plan"),
            ChatMode::Act => write!(f, "act"),
        }
    }
}

impl std::str::FromStr for ChatMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "plan" => Ok(ChatMode::Plan),
            "act" => Ok(ChatMode::Act),
            _ => Err(format!("Invalid chat mode: {}", s)),
        }
    }
}

// ============================================================================
// Tool Types
// ============================================================================

/// Tool descriptor for agent discovery
///
/// Describes a Crucible tool's capabilities, parameters, and usage for
/// agent tool discovery.
///
/// # Example
///
/// ```rust
/// use crucible_core::types::acp::ToolDescriptor;
///
/// let tool = ToolDescriptor::new(
///     "semantic_search",
///     "Search the knowledge base using semantic similarity",
/// )
/// .with_category("search")
/// .with_parameter_schema(serde_json::json!({
///     "type": "object",
///     "properties": {
///         "query": { "type": "string" }
///     }
/// }));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// Tool name/identifier
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// Tool category (e.g., "search", "note", "kiln")
    pub category: Option<String>,

    /// Parameter schema (JSON Schema format)
    pub parameter_schema: Option<serde_json::Value>,

    /// Return type schema (JSON Schema format)
    pub return_schema: Option<serde_json::Value>,

    /// Example invocations
    pub examples: Vec<ToolExample>,

    /// Required permissions
    pub required_permissions: Vec<String>,
}

impl ToolDescriptor {
    /// Create a new tool descriptor
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            category: None,
            parameter_schema: None,
            return_schema: None,
            examples: Vec::new(),
            required_permissions: Vec::new(),
        }
    }

    /// Set the category
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set the parameter schema
    pub fn with_parameter_schema(mut self, schema: serde_json::Value) -> Self {
        self.parameter_schema = Some(schema);
        self
    }

    /// Set the return schema
    pub fn with_return_schema(mut self, schema: serde_json::Value) -> Self {
        self.return_schema = Some(schema);
        self
    }

    /// Add an example
    pub fn with_example(mut self, example: ToolExample) -> Self {
        self.examples.push(example);
        self
    }

    /// Add a required permission
    pub fn with_permission(mut self, permission: impl Into<String>) -> Self {
        self.required_permissions.push(permission.into());
        self
    }
}

/// Tool usage example
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExample {
    /// Example description
    pub description: String,

    /// Example parameters
    pub parameters: serde_json::Value,

    /// Expected result (optional)
    pub result: Option<serde_json::Value>,
}

impl ToolExample {
    /// Create a new tool example
    pub fn new(description: impl Into<String>, parameters: serde_json::Value) -> Self {
        Self {
            description: description.into(),
            parameters,
            result: None,
        }
    }

    /// Set the expected result
    pub fn with_result(mut self, result: serde_json::Value) -> Self {
        self.result = Some(result);
        self
    }
}

/// Tool invocation request
///
/// Represents a request to execute a tool, including the tool name,
/// parameters, and execution context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    /// Tool name
    pub tool_name: String,

    /// Tool parameters
    pub parameters: serde_json::Value,

    /// Invocation ID for tracking
    pub invocation_id: Option<String>,

    /// Additional context
    pub context: HashMap<String, serde_json::Value>,
}

impl ToolInvocation {
    /// Create a new tool invocation
    pub fn new(tool_name: impl Into<String>, parameters: serde_json::Value) -> Self {
        Self {
            tool_name: tool_name.into(),
            parameters,
            invocation_id: None,
            context: HashMap::new(),
        }
    }

    /// Set the invocation ID
    pub fn with_invocation_id(mut self, id: impl Into<String>) -> Self {
        self.invocation_id = Some(id.into());
        self
    }

    /// Add context
    pub fn with_context(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }
}

/// Tool call information for streaming/display
///
/// Represents a tool call during agent execution. Used by streaming handlers
/// and UI layers to display tool activity. This is a protocol-agnostic type
/// that can be populated from ACP, MCP, or other agent protocols.
///
/// # Example
///
/// ```rust
/// use crucible_core::types::acp::ToolCallInfo;
///
/// let tool = ToolCallInfo::new("semantic_search")
///     .with_id("call-123")
///     .with_arguments(serde_json::json!({"query": "rust async"}));
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallInfo {
    /// Human-readable title/description of the tool call
    pub title: String,

    /// Tool parameters/arguments as JSON
    pub arguments: Option<serde_json::Value>,

    /// Unique identifier for deduplication/updates during streaming
    pub id: Option<String>,

    /// File diffs produced by this tool call (for write operations)
    pub diffs: Vec<FileDiff>,
}

impl ToolCallInfo {
    /// Create a new tool call info with a title
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            arguments: None,
            id: None,
            diffs: Vec::new(),
        }
    }

    /// Set the tool call ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the tool arguments
    pub fn with_arguments(mut self, args: serde_json::Value) -> Self {
        self.arguments = Some(args);
        self
    }

    /// Add a file diff
    pub fn with_diff(mut self, diff: FileDiff) -> Self {
        self.diffs.push(diff);
        self
    }

    /// Add multiple file diffs
    pub fn with_diffs(mut self, diffs: impl IntoIterator<Item = FileDiff>) -> Self {
        self.diffs.extend(diffs);
        self
    }
}

/// File diff representing changes to a file
///
/// Protocol-agnostic representation of file modifications. Can be populated
/// from ACP's `ToolCallContent::Diff`, generated from tool arguments, or
/// computed by comparing file states.
///
/// # Example
///
/// ```rust
/// use crucible_core::types::acp::FileDiff;
///
/// let diff = FileDiff::new("/path/to/file.rs", "fn new() {}")
///     .with_old_content("fn old() {}");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    /// Path to the modified file
    pub path: String,

    /// Original content (None for new files)
    pub old_content: Option<String>,

    /// New content after modification
    pub new_content: String,
}

impl FileDiff {
    /// Create a new file diff
    pub fn new(path: impl Into<String>, new_content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            old_content: None,
            new_content: new_content.into(),
        }
    }

    /// Set the old content (before modification)
    pub fn with_old_content(mut self, content: impl Into<String>) -> Self {
        self.old_content = Some(content.into());
        self
    }

    /// Create from old and new content
    pub fn from_contents(
        path: impl Into<String>,
        old: Option<String>,
        new: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            old_content: old,
            new_content: new.into(),
        }
    }
}

/// Tool execution output
///
/// Result of tool execution, including the result value, execution time,
/// and any metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Execution result
    pub result: serde_json::Value,

    /// Execution time in milliseconds
    pub execution_time_ms: Option<u64>,

    /// Whether execution was successful
    pub success: bool,

    /// Error message (if failed)
    pub error: Option<String>,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ToolOutput {
    /// Create a successful tool output
    pub fn success(result: serde_json::Value) -> Self {
        Self {
            result,
            execution_time_ms: None,
            success: true,
            error: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a failed tool output
    pub fn error(error: impl Into<String>) -> Self {
        Self {
            result: serde_json::Value::Null,
            execution_time_ms: None,
            success: false,
            error: Some(error.into()),
            metadata: HashMap::new(),
        }
    }

    /// Set execution time
    pub fn with_execution_time(mut self, time_ms: u64) -> Self {
        self.execution_time_ms = Some(time_ms);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

// ============================================================================
// Stream Types
// ============================================================================

/// Streaming response chunk
///
/// Represents a single chunk of a streaming response from an agent.
/// Chunks can be text, tool calls, metadata, or other content types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Chunk type
    pub chunk_type: ChunkType,

    /// Chunk content
    pub content: String,

    /// Chunk metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl StreamChunk {
    /// Create a new text chunk
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            chunk_type: ChunkType::Text,
            content: content.into(),
            metadata: HashMap::new(),
        }
    }

    /// Create a new tool call chunk
    pub fn tool_call(content: impl Into<String>) -> Self {
        Self {
            chunk_type: ChunkType::ToolCall,
            content: content.into(),
            metadata: HashMap::new(),
        }
    }

    /// Create a new metadata chunk
    pub fn metadata(content: impl Into<String>) -> Self {
        Self {
            chunk_type: ChunkType::Metadata,
            content: content.into(),
            metadata: HashMap::new(),
        }
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// Stream chunk type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkType {
    /// Text content
    Text,

    /// Tool call
    ToolCall,

    /// Metadata/status update
    Metadata,

    /// Error
    Error,
}

/// Stream metadata
///
/// Metadata about a completed stream, including token counts, timing, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMetadata {
    /// Total tokens used
    pub total_tokens: Option<u64>,

    /// Execution time in milliseconds
    pub execution_time_ms: Option<u64>,

    /// Number of tool calls
    pub tool_call_count: usize,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl StreamMetadata {
    /// Create new empty metadata
    pub fn new() -> Self {
        Self {
            total_tokens: None,
            execution_time_ms: None,
            tool_call_count: 0,
            metadata: HashMap::new(),
        }
    }

    /// Set total tokens
    pub fn with_tokens(mut self, tokens: u64) -> Self {
        self.total_tokens = Some(tokens);
        self
    }

    /// Set execution time
    pub fn with_execution_time(mut self, time_ms: u64) -> Self {
        self.execution_time_ms = Some(time_ms);
        self
    }

    /// Set tool call count
    pub fn with_tool_calls(mut self, count: usize) -> Self {
        self.tool_call_count = count;
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

impl Default for StreamMetadata {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Filesystem Types
// ============================================================================

/// File metadata
///
/// Metadata about a file in the workspace, used for file operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// File path
    pub path: PathBuf,

    /// File size in bytes
    pub size: Option<u64>,

    /// Whether this is a directory
    pub is_directory: bool,

    /// Last modified timestamp (Unix epoch)
    pub modified: Option<u64>,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl FileMetadata {
    /// Create new file metadata
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            size: None,
            is_directory: false,
            modified: None,
            metadata: HashMap::new(),
        }
    }

    /// Set file size
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Mark as directory
    pub fn as_directory(mut self) -> Self {
        self.is_directory = true;
        self
    }

    /// Set modified timestamp
    pub fn with_modified(mut self, modified: u64) -> Self {
        self.modified = Some(modified);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1, id2);

        let id3 = SessionId::from_uuid(id1.0);
        assert_eq!(id1, id3);

        let s = id1.to_string();
        assert!(!s.is_empty());
    }

    #[test]
    fn test_session_config() {
        let config = SessionConfig::new(PathBuf::from("/workspace"))
            .with_mode(ChatMode::Act)
            .with_context_size(50_000)
            .with_enrichment(false);

        assert_eq!(config.mode, ChatMode::Act);
        assert_eq!(config.context_size, 50_000);
        assert!(!config.enable_enrichment);
    }

    #[test]
    fn test_chat_mode() {
        assert!(ChatMode::Act.can_write());
        assert!(!ChatMode::Plan.can_write());
        assert!(ChatMode::Plan.is_read_only());
        assert!(!ChatMode::Act.is_read_only());

        assert_eq!(ChatMode::Plan.to_string(), "plan");
        assert_eq!(ChatMode::Act.to_string(), "act");

        assert_eq!("plan".parse::<ChatMode>().unwrap(), ChatMode::Plan);
        assert_eq!("act".parse::<ChatMode>().unwrap(), ChatMode::Act);
        assert_eq!("ACT".parse::<ChatMode>().unwrap(), ChatMode::Act);
    }

    #[test]
    fn test_tool_descriptor() {
        let tool = ToolDescriptor::new("test_tool", "A test tool")
            .with_category("test")
            .with_permission("read:notes");

        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.category, Some("test".to_string()));
        assert_eq!(tool.required_permissions, vec!["read:notes"]);
    }

    #[test]
    fn test_tool_invocation() {
        let inv = ToolInvocation::new("semantic_search", serde_json::json!({"query": "test"}))
            .with_invocation_id("inv123");

        assert_eq!(inv.tool_name, "semantic_search");
        assert_eq!(inv.invocation_id, Some("inv123".to_string()));
    }

    #[test]
    fn test_tool_output() {
        let output =
            ToolOutput::success(serde_json::json!({"result": "data"})).with_execution_time(100);

        assert!(output.success);
        assert_eq!(output.execution_time_ms, Some(100));

        let error = ToolOutput::error("Something went wrong");
        assert!(!error.success);
        assert!(error.error.is_some());
    }

    #[test]
    fn test_stream_chunk() {
        let chunk =
            StreamChunk::text("Hello, world!").with_metadata("timestamp", serde_json::json!(12345));

        assert_eq!(chunk.chunk_type, ChunkType::Text);
        assert_eq!(chunk.content, "Hello, world!");
        assert!(chunk.metadata.contains_key("timestamp"));
    }

    #[test]
    fn test_file_metadata() {
        let metadata = FileMetadata::new(PathBuf::from("test.md"))
            .with_size(1024)
            .with_modified(123456789);

        assert_eq!(metadata.path, PathBuf::from("test.md"));
        assert_eq!(metadata.size, Some(1024));
        assert_eq!(metadata.modified, Some(123456789));
        assert!(!metadata.is_directory);
    }
}
