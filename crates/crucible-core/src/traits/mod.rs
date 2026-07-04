//! Core abstractions (traits) for Crucible's dependency-inversion architecture.
//!
//! Core defines these trait abstractions (parser, tool execution, knowledge
//! repository, completion backend, etc.); concrete implementations in other
//! crates depend on Core for the definitions and are injected at the edges.

pub mod acp;
pub mod auth;
pub mod chat;
pub mod context_ops;
pub mod input;
pub mod knowledge;
pub mod llm;
pub mod mcp;
pub mod parser;
pub mod permission_gate;
pub mod provider;
pub mod storage_client;
pub mod text_search;
pub mod tools;
pub mod undoable;

// Re-export key traits
pub use acp::SessionManager;

pub use chat::{AgentHandle, ChatError, ChatResult, ChatToolCall};
pub use context_ops::{ContextMessage, MessageMetadata, Position, Range};
pub use knowledge::{KnowledgeRepository, NoteInfo};
pub use llm::{
    ChatCompletionChoice, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessageDelta, CompletionChoice, CompletionChunk, CompletionRequest, CompletionResponse,
    FunctionCall, FunctionCallBehavior, FunctionCallDelta, FunctionDefinition, LlmToolDefinition,
    LogProbs, MessageRole, ModelFeature, ModelStatus, ProviderCapabilities, ResponseFormat,
    TextModelInfo, TokenUsage, ToolCall, ToolCallDelta, ToolChoice,
};
pub use parser::MarkdownParser;
pub use provider::EmbeddingResponse;
pub use storage_client::StorageClient;
pub use tools::{ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult};
// Input abstractions for cross-platform UI
pub use input::{ChatEvent, InputMode, KeyAction, KeyCode, KeyPattern, Modifiers, SessionAction};
// MCP abstractions
pub use mcp::{
    ContentBlock, McpClientConfig, McpError, McpServerInfo, McpToolInfo, McpTransportConfig,
    ToolCallResult,
};
pub use permission_gate::PermissionGate;
pub use text_search::TextSearchMatch;
pub use undoable::Undoable;
