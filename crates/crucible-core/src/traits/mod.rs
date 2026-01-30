//! Core abstractions (traits) for Crucible's Dependency Inversion architecture
//!
//! This module defines the core abstractions that enable dependency inversion:
//! - Core defines traits (abstractions)
//! - Implementations (SurrealDB, Pulldown parser, etc.) depend on Core for trait definitions
//! - Core orchestrates through trait interfaces, never depends on concrete implementations
//!
//! ## Architecture Pattern
//!
//! ```text
//! ┌─────────────────┐
//! │  CrucibleCore   │  ← Orchestrator (defines traits, coordinates operations)
//! │   (defines)     │
//! │   - Storage     │
//! │   - Parser      │
//! │   - ToolExecutor│
//! │   - Agent       │
//! └────────┬────────┘
//!          │ uses (trait bounds)
//!          ▼
//! ┌─────────────────┐
//! │ Implementations │  ← Depend on Core for trait definitions
//! │  - SurrealDB    │
//! │  - Pulldown     │
//! │  - Rune MCP     │
//! └─────────────────┘
//! ```

pub mod acp;
pub mod change_detection;
pub mod chat;
pub mod completion_backend;
pub mod context;
pub mod context_ops;
pub mod graph_query;
pub mod input;
pub mod knowledge;
pub mod llm;
pub mod mcp;
pub mod parser;
pub mod permission_gate;
pub mod prompt_builder;
pub mod provider;
pub mod registry;
pub mod storage;
pub mod storage_client;
pub mod text_search;
pub mod tools;

// Re-export key traits
pub use acp::{FilesystemHandler, SessionManager, StreamHandler, ToolBridge};
pub use change_detection::{ChangeDetector, ContentHasher, HashLookupStorage};
pub use graph_query::{GraphQueryError, GraphQueryExecutor, GraphQueryResult};

pub use chat::{
    AgentHandle, ArgumentSpec, ChatChunk, ChatContext, ChatError, ChatResponse, ChatResult,
    ChatToolCall, CommandDescriptor, CommandHandler, CommandKind, CompletionSource, SearchResult,
};
pub use completion_backend::{
    BackendCompletionChunk, BackendCompletionRequest, BackendCompletionResponse, BackendError,
    BackendResult, CompletionBackend,
};
pub use context::ContextManager;
pub use context_ops::{
    ContextError, ContextMessage, ContextOps, MessageMetadata, MessagePredicate, Position, Range,
};
pub use knowledge::{KnowledgeRepository, NoteInfo};
pub use llm::{
    ChatCompletionChoice, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessageDelta, CompletionChoice, CompletionChunk, CompletionRequest, CompletionResponse,
    FunctionCall, FunctionCallBehavior, FunctionCallDelta, FunctionDefinition, LlmToolDefinition,
    LogProbs, MessageRole, ModelFeature, ModelStatus, ProviderCapabilities, ResponseFormat,
    TextModelInfo, TokenUsage, ToolCall, ToolCallDelta, ToolChoice,
};
pub use parser::MarkdownParser;
pub use prompt_builder::{priorities, PromptBuilder};
pub use provider::{
    CanChat, CanConstrainGeneration, CanEmbed, ConstrainedRequest, ConstrainedResponse,
    EmbeddingResponse, ExtendedCapabilities, FullProvider, Provider, ProviderExt, SchemaFormat,
};
pub use registry::{Registry, RegistryBuilder};
pub use storage::Storage;
pub use storage_client::StorageClient;
pub use tools::{ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult};
// Input abstractions for cross-platform UI
pub use input::{ChatEvent, InputMode, KeyAction, KeyCode, KeyPattern, Modifiers, SessionAction};
// MCP abstractions
pub use mcp::{
    ContentBlock, McpClient, McpClientConfig, McpConnection, McpError, McpServerInfo,
    McpToolDiscovery, McpToolExecutor, McpToolInfo, McpTransportConfig, ToolCallResult,
};
pub use permission_gate::PermissionGate;
pub use text_search::{TextSearchMatch, TextSearcher};
