pub mod agent;
pub mod canvas;
pub mod content_category;
pub mod crdt;
pub mod crucible_core;
pub mod database;
pub mod enrichment;
pub mod hashing;
pub mod merkle;
pub mod note;
pub mod parser;
pub mod processing;
pub mod properties;
pub mod sink;
pub mod storage;
pub mod test_support;
pub mod traits;
pub mod types;

pub use agent::{
    AgentDefinition, AgentLoader, AgentMatch, AgentQuery, AgentRegistry, CapabilityMatcher,
};
pub use canvas::{CanvasEdge, CanvasNode};
pub use content_category::{ContentCategory, ContentCategoryError};
pub use crucible_core::CrucibleCore;

// Re-export enrichment traits and types (implementations in crucible-enrichment crate)
pub use enrichment::{
    BlockEmbedding, CachedEmbedding, EmbeddingCache, EmbeddingProvider, EnrichedNote,
    EnrichedNoteStore, EnrichmentService, InferredRelation, NoteMetadata, RelationType,
};

// Re-export merkle tree abstractions
pub use merkle::MerkleTreeBuilder;

// Re-export processing handoff types, change detection, and pipeline trait
pub use processing::{
    ChangeDetectionError, ChangeDetectionResult, ChangeDetectionStore, FileState,
    InMemoryChangeDetectionStore, JobConfiguration, JobStats, NoteProcessingJob,
    NoteProcessingResult, NotePipelineOrchestrator, PipelineMetrics, ProcessedNote,
    ProcessingContext, ProcessingMetadata, ProcessingPriority, ProcessingResult,
    ProcessingSource,
};

// Re-export core traits (abstractions for Dependency Inversion)
pub use traits::{
    AgentProvider, ChangeDetector, ContentHasher, FilesystemHandler, HashLookupStorage,
    MarkdownParser, SessionManager, Storage, StreamHandler, ToolBridge, ToolExecutor,
};

// Re-export key types used across module boundaries
pub use types::{
    // ACP types
    ChatMode,
    ChunkType,
    FileMetadata,
    SessionConfig,
    SessionId,
    StreamChunk,
    StreamMetadata,
    ToolDescriptor,
    ToolInvocation,
    ToolOutput,
    // Hashing types
    BlockHash,
    BlockHashInfo,
    // Change detection types
    ChangeSet,
    ChangeSummary,
    // Storage trait types (from traits/storage.rs)
    // Note: Parser types (ParsedNote, Wikilink, Tag, etc.) are exported from parser:: module below
    ExecutionContext,
    FileHash,
    FileHashInfo,
    HashAlgorithm,
    HashError,
    ToolDefinition,
    ToolExample,
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
    ParserRequirements,
    // Error types (canonical definitions in crucible-core::parser::error)
    ParserError,
    ParserResult,
    Tag,
    Wikilink,
    // Note: MarkdownParser trait is exported from traits:: module above
};
pub use properties::{PropertyMap, PropertyValue};
pub use sink::{
    CircuitBreaker, CircuitBreakerConfig, CircuitState, OutputSink, SinkError, SinkHealth,
    SinkResult,
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
