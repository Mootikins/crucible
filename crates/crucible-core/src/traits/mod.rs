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
pub mod agent;
pub mod change_detection;
pub mod chat;
pub mod knowledge;
pub mod llm;
pub mod parser;
pub mod storage;
pub mod tools;

// Re-export key traits
pub use acp::{FilesystemHandler, SessionManager, StreamHandler, ToolBridge};
pub use agent::AgentProvider;
pub use change_detection::{ChangeDetector, ContentHasher, HashLookupStorage};
pub use chat::{
    ChatAgent, ChatContext, ChatError, ChatMode, ChatResponse, ChatResult,
    CommandDescriptor, CommandHandler, CommandRegistry, SearchResult, ToolCall as ChatToolCall,
};
pub use knowledge::{KnowledgeRepository, NoteMetadata};
pub use llm::{
    LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmResult, LlmToolDefinition,
    MessageRole, ToolCall, TokenUsage,
};
pub use parser::MarkdownParser;
pub use storage::Storage;
pub use tools::{ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult};
