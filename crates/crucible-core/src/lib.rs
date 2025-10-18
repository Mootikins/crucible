pub mod agent;
pub mod canvas;
pub mod crdt;
pub mod database;
pub mod document;
pub mod properties;
// pub mod task_router; // Temporarily disabled due to compilation issues

pub use agent::{AgentDefinition, AgentLoader, AgentMatch, AgentQuery, AgentRegistry, CapabilityMatcher};
pub use canvas::{CanvasEdge, CanvasNode};
pub use database::{
    RelationalDB, GraphDB, DocumentDB,
    // Core types
    DbResult, DbError,
    // Relational types
    TableSchema, Record, RecordId, QueryResult, SelectQuery, FilterClause, OrderClause, UpdateClause,
    JoinQuery, AggregateQuery, TransactionId, ColumnDefinition, DataType, ForeignKey, IndexType,
    // Graph types
    NodeId, Node, EdgeId, Edge, NodeProperties, EdgeProperties, Direction, TraversalPattern,
    TraversalResult, Path, GraphAnalysis, AnalyticsResult,
    // Document types
    DocumentId, Document, DocumentMetadata, DocumentQuery, DocumentFilter, DocumentUpdates,
    SearchResult, AggregationPipeline, AggregationResult, BatchResult,
};
pub use document::{DocumentNode, ViewportState};
pub use properties::{PropertyMap, PropertyValue};
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
