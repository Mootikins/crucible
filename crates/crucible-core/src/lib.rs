pub mod agent;
pub mod canvas;
pub mod config;
pub mod controller;
pub mod crdt;
pub mod crucible_core;
pub mod database;
pub mod document;
pub mod orchestrator_simple;
pub mod properties;
pub mod parser;
pub mod router_simple;
pub mod sink;
pub mod state;
// pub mod task_router; // Temporarily disabled due to compilation issues

pub use agent::{AgentDefinition, AgentLoader, AgentMatch, AgentQuery, AgentRegistry, CapabilityMatcher};
pub use canvas::{CanvasEdge, CanvasNode};
pub use config::{
    ConfigManager, CrucibleConfig, ServiceConfig, DatabaseConfig, NetworkConfig,
    LoggingConfig, FeatureConfig, PerformanceConfig, ConfigChange,
};
pub use controller::{
    MasterController, ControllerState, ControllerStatus, ControllerMetrics,
    HealthStatus, ControllerEvent, ControllerBuilder,
};
pub use crucible_core::{
    CrucibleCore, CoreConfig, CoreState, CoreEvent, AlertLevel, CoreHealthData,
    CoreMetrics, CoreMetricsSnapshot, CrucibleCoreBuilder,
};
pub use database::{
    RelationalDB, GraphDB, DocumentDB,
    // Core types
    DbResult, DbError,
    // Relational types
    TableSchema, Record, RecordId, QueryResult, SelectQuery, FilterClause, OrderClause, UpdateClause,
    JoinQuery, AggregateQuery, TransactionId, ColumnDefinition, DataType, ForeignKey, IndexType,
    OrderDirection, AggregateType, ReferentialAction, IndexDefinition, JoinType, JoinClause, AggregateFunction,
    // Graph types
    NodeId, Node, EdgeId, Edge, NodeProperties, EdgeProperties, Direction, TraversalPattern,
    TraversalResult, Path, GraphAnalysis, AnalyticsResult, EdgeFilter, Subgraph, SubgraphPattern,
    NodePattern, EdgePattern, TraversalStep, CommunityAlgorithm,
    // Document types
    DocumentId, Document, DocumentMetadata, DocumentQuery, DocumentFilter, DocumentUpdates,
    SearchResult, AggregationPipeline, AggregationResult, BatchResult, DocumentSchema,
    FieldDefinition, DocumentFieldType, ValidationRules, DocumentSort, SearchOptions,
    SearchIndexOptions, AggregationStage, GroupOperation,
};
pub use document::{DocumentNode, ViewportState};
pub use orchestrator_simple::{SimpleServiceOrchestrator, ServiceInstance, ServiceEvent, OrchestrationMetrics, SimpleServiceRegistrationBuilder};
pub use properties::{PropertyMap, PropertyValue};
pub use parser::{
    ParsedDocument, Frontmatter, FrontmatterFormat, Wikilink, Tag,
    DocumentContent, Heading, CodeBlock, MarkdownParser, ParserCapabilities,
    ParserError, ParserResult,
};
pub use router_simple::{SimpleRequestRouter, RouterMetrics, RouterEvent};
pub use sink::{
    OutputSink, SinkHealth, SinkError, SinkResult,
    CircuitBreaker, CircuitState, CircuitBreakerConfig,
};
pub use state::{
    StateManager, ApplicationState, StateMetadata, UserPreferences,
    StateEvent, StateCommand, CacheEntry, CacheStats,
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
