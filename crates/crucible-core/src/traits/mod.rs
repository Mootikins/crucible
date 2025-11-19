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

pub mod agent;
pub mod change_detection;
pub mod knowledge;
pub mod parser;
pub mod storage;
pub mod tools;

// Re-export key traits
pub use agent::AgentProvider;
pub use change_detection::{ChangeDetector, ContentHasher, HashLookupStorage};
pub use knowledge::{KnowledgeRepository, NoteMetadata};
pub use parser::MarkdownParser;
pub use storage::Storage;
pub use tools::ToolExecutor;
