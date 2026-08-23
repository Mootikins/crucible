// Re-export uuid for downstream crates
pub use uuid;

pub mod agent;
pub mod background;
pub mod bundled_docs;
pub mod canvas;
pub mod config;
pub mod discovery;
pub mod enrichment;
pub mod error_utils;
pub mod events;
pub mod fs;
pub mod fuzzy;
pub mod http;
pub mod interaction;
pub mod kiln;
pub mod parser;
pub mod paths;
pub mod processing;
pub mod project;
pub mod prompts;
pub mod protocol;
pub mod recording;
pub mod runtime_roots;
pub mod serde_helpers;
pub mod session;
pub mod storage;
// Test helpers only. The `cru` binary never compiles them.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_support;
pub mod text;
pub mod traits;
pub mod turn;
pub mod types;
pub mod utils;
pub mod workflow;

pub use agent::{
    AgentCard, AgentCardFrontmatter, AgentCardLoader, AgentCardMatch, AgentCardMatcher,
    AgentCardQuery, AgentCardRegistry,
};
pub use discovery::DiscoveryPaths;
pub use error_utils::strip_tool_error_prefix;
pub use kiln::{
    is_canvas_file, is_indexable_file, is_note_file, is_plain_text_file, KilnFileKind,
    EXCLUDED_DIRS,
};

// Re-export enrichment types (concrete implementation lives in crucible-daemon::enrichment)
pub use enrichment::{BlockEmbedding, EmbeddingProvider, EnrichedNote, EnrichmentMetadata};

// Re-export the pipeline result type
pub use processing::ProcessingResult;

// Re-export core traits (abstractions for Dependency Inversion)
pub use traits::{ContextMessage, ToolExecutor};

// Re-export key types used across module boundaries
pub use types::{
    // ACP schema types from agent-client-protocol-schema
    AvailableCommand,
    AvailableCommandInput,
    AvailableCommandsUpdate,
    // Hashing types
    BlockHash,
    BlockHashInfo,
    // Storage trait types (from traits/storage.rs)
    // Note: Parser types (ParsedNote, Wikilink, Tag, etc.) are exported from parser:: module below
    ExecutionContext,
    FileHash,
    FileHashInfo,
    HashAlgorithm,
    // Mode descriptor types
    ModeDescriptor,
    SessionMode,
    SessionModeId,
    SessionModeState,
    ToolDefinition,
    ToolExample,
};

pub use parser::{
    // Parser types (canonical definitions in crucible-core::parser::types)
    CodeBlock,
    Frontmatter,
    FrontmatterFormat,
    Heading,
    NoteContent,
    ParsedNote,
    ParsedNoteMetadata,
    // Parser traits and capabilities
    ParserCapabilities,

    // Error types (canonical definitions in crucible-core::parser::error)
    ParserError,
    ParserRequirements,
    ParserResult,
    Tag,
    Wikilink,
};
pub use types::database::{DocumentId, QueryResult, Record, RecordId, SearchResult};

// Re-export interaction protocol types
pub use interaction::{
    ArtifactFormat, AskBatch, AskBatchResponse, AskQuestion, AskRequest, AskResponse, EditRequest,
    EditResponse, InteractionRequest, InteractionResponse, InteractivePanel, PanelHints, PanelItem,
    PanelResult, PanelState, PermAction, PermRequest, PermResponse, PermissionScope, PopupRequest,
    PopupResponse, QuestionAnswer, ShowRequest,
};

// Re-export session types (daemon session management)
pub use session::{Session, SessionState, SessionSummary, SessionType};

// Re-export project types (workspace registration)
pub use project::{Project, ProjectKiln, RepositoryInfo};

pub use background::{generate_job_id, JobError, JobId, JobInfo, JobKind, JobResult, JobStatus};

// Re-export event system types
pub use events::{
    // Emitter types
    EmitOutcome,
    EmitResult,
    // Session event types
    EventEmitter,
    EventError,
    FileChangeKind,
    HandlerErrorInfo,
    NoOpEmitter,
    NoteChangeType,
    Priority,
    SessionEvent,
    SharedEventBus,
};

#[derive(Debug, thiserror::Error)]
pub enum CrucibleError {
    #[error("Note not found: {0}")]
    DocumentNotFound(uuid::Uuid),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Database error: {0}")]
    DatabaseError(String),
}

pub type Result<T> = std::result::Result<T, CrucibleError>;
