//! Essential types for Crucible Tools - Phase 3.1 Simplified
//!
//! This module contains only essential types for simple async function composition.
//! All legacy complexity has been removed to focus on the core 25+ tools.
//!
//! **Phase 3.1 Changes:**
//! - Reduced from 538 lines to ~200 lines
//! - Removed duplicate result types and simplified error handling
//! - Cleaned up legacy comments and removed references to deleted features
//! - Focused purely on essential types for tool execution

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// Simple tool definition for basic tool registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON schema for tool input
    pub input_schema: Value,
    /// Whether the tool is enabled
    pub enabled: bool,
}

/// Simple context for tool execution - Phase 2.1 simplified
/// Replaced complex `ContextRef` patterns with direct parameters for async function composition
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolExecutionContext {
    /// User ID for the execution
    pub user_id: Option<String>,
    /// Session ID for the execution
    pub session_id: Option<String>,
    /// Working directory (if needed)
    pub working_directory: Option<String>,
    /// Environment variables
    pub environment: HashMap<String, String>,
}

impl ToolExecutionContext {
    /// Create a new context with user and session
    #[must_use]
    pub fn with_user_session(user_id: Option<String>, session_id: Option<String>) -> Self {
        Self {
            user_id,
            session_id,
            working_directory: None,
            environment: HashMap::new(),
        }
    }

    /// Create a context with working directory
    #[must_use]
    pub fn with_working_dir(working_directory: String) -> Self {
        Self {
            user_id: None,
            session_id: None,
            working_directory: Some(working_directory),
            environment: HashMap::new(),
        }
    }

    /// Add environment variable
    #[must_use]
    pub fn with_env(mut self, key: String, value: String) -> Self {
        self.environment.insert(key, value);
        self
    }
}

/// Simple request for tool execution - Phase 2.1 simplified
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRequest {
    /// Tool name to execute
    pub tool_name: String,
    /// Tool input parameters
    pub parameters: Value,
    /// Simple execution context
    pub context: ToolExecutionContext,
    /// Request ID
    pub request_id: String,
}

impl ToolExecutionRequest {
    /// Create a new execution request
    #[must_use]
    pub fn new(tool_name: String, parameters: Value, context: ToolExecutionContext) -> Self {
        Self {
            tool_name,
            parameters,
            context,
            request_id: Uuid::new_v4().to_string(),
        }
    }

    /// Create a request with minimal context
    #[must_use]
    pub fn simple(tool_name: String, parameters: Value) -> Self {
        Self::new(tool_name, parameters, ToolExecutionContext::default())
    }

    /// Create a request with user and session context
    #[must_use]
    pub fn with_user_session(
        tool_name: String,
        parameters: Value,
        user_id: Option<String>,
        session_id: Option<String>,
    ) -> Self {
        let context = ToolExecutionContext::with_user_session(user_id, session_id);
        Self::new(tool_name, parameters, context)
    }
}

/// Simplified tool error type for Phase 3.1
#[derive(Debug, Clone)]
pub enum ToolError {
    /// Tool with the specified name was not found in the registry
    ToolNotFound(String),
    /// Tool execution failed with the provided error message
    ExecutionFailed(String),
    /// Other error with the provided message
    Other(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::ToolNotFound(name) => write!(f, "Tool '{name}' not found"),
            ToolError::ExecutionFailed(msg) => write!(f, "Execution failed: {msg}"),
            ToolError::Other(msg) => write!(f, "Error: {msg}"),
        }
    }
}

impl std::error::Error for ToolError {}

/// Simplified tool execution result for Phase 3.1
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Whether execution was successful
    pub success: bool,
    /// Result data (JSON value)
    pub data: Option<serde_json::Value>,
    /// Error message (if any)
    pub error: Option<String>,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Tool name that was executed
    pub tool_name: String,
}

impl ToolResult {
    /// Create a successful result
    #[must_use]
    pub fn success(tool_name: String, data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            duration_ms: 0,
            tool_name,
        }
    }

    /// Create a successful result with duration
    #[must_use]
    pub fn success_with_duration(
        tool_name: String,
        data: serde_json::Value,
        duration_ms: u64,
    ) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            duration_ms,
            tool_name,
        }
    }

    /// Create an error result
    #[must_use]
    pub fn error(tool_name: String, error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            duration_ms: 0,
            tool_name,
        }
    }

    /// Create an error result with duration
    #[must_use]
    pub fn error_with_duration(tool_name: String, error: String, duration_ms: u64) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            duration_ms,
            tool_name,
        }
    }
}

/// Simplified tool function signature for Phase 3.1
/// All tools should implement this signature for unified execution
pub type ToolFunction = fn(
    tool_name: String,
    parameters: serde_json::Value,
    user_id: Option<String>,
    session_id: Option<String>,
    context: std::sync::Arc<ToolConfigContext>,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<ToolResult, ToolError>> + Send>,
>;

/// Simple tool registry function signature for Phase 3.1
/// Maps tool names to their executable functions
pub type ToolFunctionRegistry = HashMap<String, ToolFunction>;

/// Tool definition registry
/// Maps tool names to their definitions
pub type ToolDefinitionRegistry = HashMap<String, ToolDefinition>;


// ===== GLOBAL TOOL CONFIGURATION CONTEXT =====
// Thread-safe global configuration for tools (separate from per-request context)

use crucible_core::traits::KnowledgeRepository;
use crucible_llm::embeddings::EmbeddingProvider;

/// Global configuration context for tools
///
/// This provides shared configuration that tools can access without
/// requiring parameters on every call. This is distinct from the per-request
/// `ToolExecutionContext` which handles user sessions and environment variables.
#[derive(Clone)]
pub struct ToolConfigContext {
    /// Path to the kiln directory
    pub kiln_path: Option<PathBuf>,
    /// Knowledge repository for database access
    pub knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    /// Embedding provider for semantic search
    pub embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
}

// Manual Debug impl because KnowledgeRepository/EmbeddingProvider don't implement Debug
impl std::fmt::Debug for ToolConfigContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolConfigContext")
            .field("kiln_path", &self.kiln_path)
            .field("knowledge_repo", &if self.knowledge_repo.is_some() { "Some(KnowledgeRepository)" } else { "None" })
            .field("embedding_provider", &if self.embedding_provider.is_some() { "Some(EmbeddingProvider)" } else { "None" })
            .finish()
    }
}

impl ToolConfigContext {
    /// Create empty context
    #[must_use]
    pub fn new() -> Self {
        Self {
            kiln_path: None,
            knowledge_repo: None,
            embedding_provider: None,
        }
    }

    /// Set the kiln path
    #[must_use]
    pub fn with_kiln_path(mut self, kiln_path: PathBuf) -> Self {
        self.kiln_path = Some(kiln_path);
        self
    }

    /// Set the knowledge repository
    #[must_use]
    pub fn with_knowledge_repo(mut self, repo: Arc<dyn KnowledgeRepository>) -> Self {
        self.knowledge_repo = Some(repo);
        self
    }

    /// Set the embedding provider
    #[must_use]
    pub fn with_embedding_provider(mut self, provider: Arc<dyn EmbeddingProvider>) -> Self {
        self.embedding_provider = Some(provider);
        self
    }
}

impl Default for ToolConfigContext {
    fn default() -> Self {
        Self::new()
    }
}



// ===== SIMPLE TOOL LOADER (PHASE 3.1) =====
// Simplified tool loading without hot-reload or dynamic discovery complexity
// Focuses on direct async function registration and execution

