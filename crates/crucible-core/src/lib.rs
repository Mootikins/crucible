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
    CodeBlock, DocumentContent, Frontmatter, FrontmatterFormat, Heading, MarkdownParser,
    ParsedDocument, ParserCapabilities, ParserError, ParserResult, Tag, Wikilink,
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
}

pub type Result<T> = std::result::Result<T, CrucibleError>;
