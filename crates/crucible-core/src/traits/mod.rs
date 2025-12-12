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
pub mod input;
pub mod knowledge;
pub mod llm;
pub mod parser;
pub mod registry;
pub mod storage;
pub mod tools;

// Re-export key traits
pub use acp::{FilesystemHandler, SessionManager, StreamHandler, ToolBridge};
pub use change_detection::{ChangeDetector, ContentHasher, HashLookupStorage};
pub use chat::{
    AgentHandle, ChatContext, ChatError, ChatMode, ChatResponse, ChatResult, CommandDescriptor,
    CommandHandler, SearchResult, ToolCall as ChatToolCall,
};
pub use knowledge::{KnowledgeRepository, NoteInfo};
pub use llm::{
    ChatCompletionChoice, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessageDelta, CompletionChoice, CompletionChunk, CompletionRequest, CompletionResponse,
    FunctionCall, FunctionCallBehavior, FunctionCallDelta, FunctionDefinition, LlmError,
    LlmMessage, LlmRequest, LlmResponse, LlmResult, LlmToolDefinition, LogProbs, MessageRole,
    ModelCapability, ModelStatus, ProviderCapabilities, ResponseFormat, TextGenerationProvider,
    TextModelInfo, TokenUsage, ToolCall, ToolCallDelta, ToolChoice,
};
pub use parser::MarkdownParser;
pub use registry::{Registry, RegistryBuilder};
pub use storage::Storage;
pub use tools::{ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult};
// Input abstractions for cross-platform UI
pub use input::{ChatEvent, InputMode, KeyAction, KeyCode, KeyPattern, Modifiers, SessionAction};
