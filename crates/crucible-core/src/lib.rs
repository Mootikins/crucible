pub mod agent;
pub mod canvas;
pub mod config;
pub mod crdt;
pub mod crucible_core;
pub mod database;
pub mod document;
pub mod parser;
pub mod properties;
pub mod sink;
pub mod test_support;
pub mod traits;
pub mod types;
// pub mod task_router; // Temporarily disabled due to compilation issues

pub use agent::{
    AgentDefinition, AgentLoader, AgentMatch, AgentQuery, AgentRegistry, CapabilityMatcher,
};
pub use canvas::{CanvasEdge, CanvasNode};
pub use config::{
    ConfigChange, ConfigManager, CrucibleConfig, FeatureConfig, LoggingConfig, NetworkConfig,
    PerformanceConfig, ServiceConfig, ServiceDatabaseConfig,
};
pub use crucible_core::CrucibleCore;
// Note: CrucibleCoreConfig is deprecated - use CrucibleCore::builder() instead

// Re-export core traits (abstractions for Dependency Inversion)
pub use traits::{
    AgentProvider, MarkdownParser, Storage, ToolExecutor,
};

// Re-export key types used across module boundaries
pub use types::{
    // Storage trait types (from traits/storage.rs)
    // Note: Parser types (ParsedDocument, Wikilink, Tag, etc.) are exported from parser:: module below
    ExecutionContext, ToolDefinition, ToolExample,
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
    Document,
    DocumentDB,
    DocumentFieldType,
    DocumentFilter,
    // Document types
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
pub use document::{DocumentNode, ViewportState};
pub use parser::{
    CodeBlock, DocumentContent, Frontmatter, FrontmatterFormat, Heading,
    ParsedDocument, ParserCapabilities, ParserError, ParserResult, Tag, Wikilink,
    // Note: MarkdownParser trait is exported from traits:: module above
};
pub use properties::{PropertyMap, PropertyValue};
pub use sink::{
    CircuitBreaker, CircuitBreakerConfig, CircuitState, OutputSink, SinkError, SinkHealth,
    SinkResult,
};
// pub use task_router::{
//     TaskRouter, TaskAnalyzer, IntelligentRouter, TaskQueueManager, ExecutionEngine,
//     ResultAggregator, ErrorHandler, PerformanceMonitor, UserRequest, TaskResult,
//     TaskAnalysis, RoutingDecision, TaskExecutionResult, SystemStatus
// };

#[derive(Debug, thiserror::Error)]
pub enum CrucibleError {
    #[error("Document not found: {0}")]
    DocumentNotFound(uuid::Uuid),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("CRDT error: {0}")]
    CrdtError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),
}

pub type Result<T> = std::result::Result<T, CrucibleError>;
