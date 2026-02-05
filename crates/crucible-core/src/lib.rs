// Re-export uuid for downstream crates
pub use uuid;

pub mod agent;
pub mod background;
pub mod canvas;
pub mod content_category;
pub mod crdt;
pub mod crucible_core;
pub mod database;
pub mod discovery;
pub mod enrichment;
pub mod events;
pub mod fuzzy;
pub mod hashing;
pub mod http;
pub mod interaction;
pub mod interaction_context;
pub mod interaction_registry;
pub mod merkle;
pub mod note;
pub mod parser;
pub mod processing;
pub mod project;
pub mod prompts;
pub mod properties;
pub mod serde_md;
pub mod session;
pub mod storage;
pub mod test_support;
pub mod traits;
pub mod types;
pub mod utils;

pub use agent::{
    AgentCard, AgentCardFrontmatter, AgentCardLoader, AgentCardMatch, AgentCardMatcher,
    AgentCardQuery, AgentCardRegistry,
};
pub use canvas::{CanvasEdge, CanvasNode};
pub use content_category::{ContentCategory, ContentCategoryError};
pub use crucible_core::CrucibleCore;
pub use discovery::{DiscoveryConfig, DiscoveryPaths};
pub use interaction_context::{EventPushCallback, InteractionContext};

// Re-export enrichment traits and types (implementations in crucible-enrichment crate)
pub use enrichment::{
    BlockEmbedding, CachedEmbedding, EmbeddingCache, EmbeddingProvider, EnrichedNote,
    EnrichedNoteStore, EnrichmentMetadata, EnrichmentService, InferredRelation, RelationType,
};

// Re-export merkle tree abstractions
pub use merkle::MerkleTreeBuilder;

// Re-export processing handoff types, change detection, and pipeline trait
pub use processing::{
    ChangeDetectionError, ChangeDetectionResult, ChangeDetectionStore, FileState,
    InMemoryChangeDetectionStore, JobConfiguration, JobStats, NotePipelineOrchestrator,
    NoteProcessingJob, NoteProcessingResult, PipelineMetrics, ProcessedNote, ProcessingContext,
    ProcessingMetadata, ProcessingPriority, ProcessingResult, ProcessingSource,
};

// Re-export core traits (abstractions for Dependency Inversion)
pub use traits::{
    ChangeDetector, ContentHasher, FilesystemHandler, HashLookupStorage, MarkdownParser, Registry,
    RegistryBuilder, SessionManager, Storage, StreamHandler, ToolBridge, ToolExecutor,
};

// Re-export key types used across module boundaries
pub use types::{
    // ACP schema types from agent-client-protocol-schema
    AvailableCommand,
    AvailableCommandInput,
    AvailableCommandsUpdate,
    // Hashing types
    BlockHash,
    BlockHashInfo,
    // Change detection types
    ChangeSet,
    ChangeSummary,
    // ACP types
    ChunkType,
    // Storage trait types (from traits/storage.rs)
    // Note: Parser types (ParsedNote, Wikilink, Tag, etc.) are exported from parser:: module below
    ExecutionContext,
    FileHash,
    FileHashInfo,
    FileMetadata,
    HashAlgorithm,
    HashError,
    // Mode descriptor types
    ModeDescriptor,
    SessionConfig,
    SessionId,
    SessionMode,
    SessionModeId,
    SessionModeState,
    StreamChunk,
    StreamMetadata,
    ToolDefinition,
    ToolExample,
    ToolInvocation,
    ToolOutput,
};

pub use database::{
    AggregateFunction,
    AggregateQuery,
    AggregateType,
    AggregationPipeline,
    AggregationResult,
    AggregationStage,
    AnalyticsResult,
    BatchResult,
    ColumnDefinition,
    CommunityAlgorithm,
    DataType,
    DbError,
    // Core types
    DbResult,
    Direction,
    DocumentDB,
    DocumentFieldType,
    DocumentFilter,
    // Note types
    DocumentId,
    DocumentMetadata,
    DocumentQuery,
    DocumentSchema,
    DocumentSort,
    DocumentUpdates,
    Edge,
    EdgeFilter,
    EdgeId,
    EdgePattern,
    EdgeProperties,
    FieldDefinition,
    FilterClause,
    ForeignKey,
    GraphAnalysis,
    GraphDB,
    GroupOperation,
    IndexDefinition,
    IndexType,
    JoinClause,
    JoinQuery,
    JoinType,
    Node,
    // Graph types
    NodeId,
    NodePattern,
    NodeProperties,
    Note,
    OrderClause,
    OrderDirection,
    Path,
    QueryResult,
    Record,
    RecordId,
    ReferentialAction,
    RelationalDB,
    SearchIndexOptions,
    SearchOptions,
    SearchResult,
    SelectQuery,
    Subgraph,
    SubgraphPattern,
    // Relational types
    TableSchema,
    TransactionId,
    TraversalPattern,
    TraversalResult,
    TraversalStep,
    UpdateClause,
    ValidationRules,
};
pub use note::{NoteNode, ViewportState};
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
    ParserCapabilitiesExt,
    // Error types (canonical definitions in crucible-core::parser::error)
    ParserError,
    ParserRequirements,
    ParserResult,
    Tag,
    Wikilink,
    // Note: MarkdownParser trait is exported from traits:: module above
};
pub use properties::{AttributeValue, PropertyMap};

// Re-export interaction protocol types
pub use interaction::{
    ArtifactFormat, AskBatch, AskBatchResponse, AskQuestion, AskRequest, AskResponse, EditRequest,
    EditResponse, InteractionRequest, InteractionResponse, InteractivePanel, PanelAction,
    PanelHints, PanelItem, PanelResult, PanelState, PermAction, PermRequest, PermResponse,
    PermissionScope, PopupRequest, PopupResponse, QuestionAnswer, ShowRequest,
};
pub use interaction_registry::InteractionRegistry;

// Re-export session types (daemon session management)
pub use session::{Session, SessionState, SessionSummary, SessionType};

// Re-export project types (workspace registration)
pub use project::{Project, RepositoryInfo};

pub use background::{generate_job_id, JobError, JobId, JobInfo, JobKind, JobResult, JobStatus};

// Re-export event system types
pub use events::{
    // Subscriber types
    box_handler,
    BoxedHandlerFn,
    // Emitter types
    EmitOutcome,
    EmitResult,
    // Session event types
    EntityType,
    EventBus,
    EventEmitter,
    EventError,
    EventFilter,
    EventSubscriber,
    FileChangeKind,
    HandlerErrorInfo,
    HandlerFuture,
    HandlerResult,
    NoOpEmitter,
    NoteChangeType,
    NotePayload,
    Priority,
    SessionEvent,
    SessionEventConfig,
    SharedEventBus,
    SubscriptionError,
    SubscriptionId,
    SubscriptionIdGenerator,
    SubscriptionInfo,
    SubscriptionResult,
    ToolCall,
    ToolProvider,
};

#[derive(Debug, thiserror::Error)]
pub enum CrucibleError {
    #[error("Note not found: {0}")]
    DocumentNotFound(uuid::Uuid),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("CRDT error: {0}")]
    CrdtError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),
}

pub type Result<T> = std::result::Result<T, CrucibleError>;
