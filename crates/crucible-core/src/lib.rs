// Re-export uuid for downstream crates
pub use uuid;

// Lets a row of `rpc_methods!` (`protocol::rpc::method`) name its params/reply
// type as `crucible_core::...` rather than `crate::...`. A macro-captured
// `crate::` path loses its definition-site crate root once
// `for_each_rpc_method!` hands the row on to a callback macro in another
// crate (`macro_expanded_macro_exports_accessed_by_absolute_paths`-adjacent
// hygiene: `crate` re-resolves against the *expanding* crate, not the one
// that wrote it); an absolute path through the crate's own name does not.
extern crate self as crucible_core;

pub mod agent;
pub mod background;
pub mod bundled_docs;
pub mod canvas;
pub mod config;
pub mod diff;
pub mod enrichment;
pub mod error_utils;
pub mod events;
pub mod file_write;
pub mod fs;
pub mod fuzzy;
pub mod git;
pub mod http;
pub mod interaction;
pub mod kiln;
pub mod lua_source;
pub mod note_edit;
pub mod note_frontmatter;
pub mod note_merge;
pub mod parser;
pub mod paths;
pub mod processing;
pub mod project;
pub mod prompts;
pub mod proposal;
pub mod protocol;
pub mod recording;
pub mod runtime_path;
pub mod runtime_roots;
pub mod serde_helpers;
pub mod session;
pub mod sources;
pub mod status_color;
pub mod storage;
// Test helpers only. The `cru` binary never compiles them.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_support;
pub mod text;
pub mod traits;
pub mod transcript;
pub mod turn;
pub mod types;
pub mod utils;
pub mod workflow;

pub use agent::{
    AgentCard, AgentCardFrontmatter, AgentCardLoader, AgentCardMatch, AgentCardMatcher,
    AgentCardQuery, AgentCardRegistry,
};
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
    // The one content hash type
    BlockHash,
    // Storage trait types (from traits/storage.rs)
    // Note: Parser types (ParsedNote, Wikilink, Tag, etc.) are exported from parser:: module below
    ExecutionContext,
    // Mode descriptor types
    ModeDescriptor,
    SessionMode,
    SessionModeId,
    SessionModeState,
    ToolDefinition,
};

pub use parser::{
    // Parser types (canonical definitions in crucible-core::parser::types)
    Frontmatter,
    FrontmatterFormat,
    NoteContent,
    ParsedNote,
    ParsedNoteMetadata,
    // Parser traits and capabilities
    ParserCapabilities,

    // Error types (canonical definitions in crucible-core::parser::error)
    ParserError,
    ParserResult,
    Tag,
    Wikilink,
};
pub use types::database::SearchResult;

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
    // Session event types
    EventEmitter,
    FileChangeKind,
    HandlerErrorInfo,
    NoOpEmitter,
    NoteChangeType,
    SessionEvent,
    SharedEventBus,
};

pub mod bases;
