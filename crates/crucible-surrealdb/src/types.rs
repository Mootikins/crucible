//! Multi-Model Database Abstractions
//!
//! This module provides trait abstractions for different database models:
//! - RelationalDB: SQL-like operations with tables, joins, and aggregations
//! - GraphDB: Graph operations with nodes, edges, and traversals
//! - DocumentDB: Document operations with collections, search, and aggregations
//!
//! These traits allow the same underlying database (like SurrealDB) to be used
//! through different data access patterns, enabling evaluation of which model
//! works best for different Crucible use cases.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export common database types from crucible-core
pub use crucible_core::document::DocumentNode;
pub use crucible_core::properties::{PropertyMap, PropertyValue};

// ==============================================================================
// DATABASE CONFIGURATION
// ==============================================================================

/// Configuration for SurrealDB connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurrealDbConfig {
    /// Namespace to use
    pub namespace: String,
    /// Database name
    pub database: String,
    /// Path to database file (or ":memory:" for in-memory)
    pub path: String,
    /// Maximum number of connections in pool
    pub max_connections: Option<u32>,
    /// Timeout in seconds for operations
    pub timeout_seconds: Option<u32>,
}

impl Default for SurrealDbConfig {
    fn default() -> Self {
        Self {
            namespace: "crucible".to_string(),
            database: "kiln".to_string(),
            path: "./data/kiln.db".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        }
    }
}

// ==============================================================================
// ERROR TYPES
// ==============================================================================

/// Common result type for database operations
pub type DbResult<T> = Result<T, DbError>;

/// Database operation errors
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum DbError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Query error: {0}")]
    Query(String),

    #[error("Schema error: {0}")]
    Schema(String),

    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

// ==============================================================================
// RELATIONAL DATABASE TRAIT
// ==============================================================================

/// Relational database operations trait
///
/// Provides SQL-like operations for structured data with schemas, tables,
/// joins, and aggregations. Best for structured queries, analytics, and reporting.
#[async_trait]
pub trait RelationalDB: Send + Sync {
    /// Create a new table with specified schema
    async fn create_table(&self, name: &str, schema: TableSchema) -> DbResult<()>;

    /// Drop an existing table
    async fn drop_table(&self, name: &str) -> DbResult<()>;

    /// List all tables in the database
    async fn list_tables(&self) -> DbResult<Vec<String>>;

    /// Get table schema information
    async fn get_table_schema(&self, name: &str) -> DbResult<Option<TableSchema>>;

    /// Insert a record into a table
    async fn insert(&self, table: &str, record: Record) -> DbResult<QueryResult>;

    /// Insert multiple records in a batch
    async fn insert_batch(&self, table: &str, records: Vec<Record>) -> DbResult<QueryResult>;

    /// Select records with optional filtering and projection
    async fn select(&self, query: SelectQuery) -> DbResult<QueryResult>;

    /// Update records matching filter criteria
    async fn update(
        &self,
        table: &str,
        filter: FilterClause,
        updates: UpdateClause,
    ) -> DbResult<QueryResult>;

    /// Delete records matching filter criteria
    async fn delete(&self, table: &str, filter: FilterClause) -> DbResult<QueryResult>;

    /// Join multiple tables with specified conditions
    async fn join_tables(&self, query: JoinQuery) -> DbResult<QueryResult>;

    /// Perform aggregation operations (COUNT, SUM, AVG, etc.)
    async fn aggregate(&self, query: AggregateQuery) -> DbResult<QueryResult>;

    /// Create an index on specified columns
    async fn create_index(
        &self,
        table: &str,
        columns: Vec<String>,
        index_type: IndexType,
    ) -> DbResult<()>;

    /// Drop an existing index
    async fn drop_index(&self, table: &str, columns: Vec<String>) -> DbResult<()>;

    /// Begin a transaction
    async fn begin_transaction(&self) -> DbResult<TransactionId>;

    /// Commit a transaction
    async fn commit_transaction(&self, transaction_id: TransactionId) -> DbResult<()>;

    /// Rollback a transaction
    async fn rollback_transaction(&self, transaction_id: TransactionId) -> DbResult<()>;
}

// ==============================================================================
// GRAPH DATABASE TRAIT
// ==============================================================================

/// Graph database operations trait
///
/// Provides graph operations with nodes, edges, traversals, and analytics.
/// Best for relationship discovery, path finding, and network analysis.
#[async_trait]
pub trait GraphDB: Send + Sync {
    /// Create a node with specified label and properties
    async fn create_node(&self, label: &str, properties: NodeProperties) -> DbResult<NodeId>;

    /// Get a node by its ID
    async fn get_node(&self, node_id: &NodeId) -> DbResult<Option<Node>>;

    /// Update node properties
    async fn update_node(&self, node_id: &NodeId, properties: NodeProperties) -> DbResult<()>;

    /// Delete a node and all its edges
    async fn delete_node(&self, node_id: &NodeId) -> DbResult<()>;

    /// Create an edge between two nodes
    async fn create_edge(
        &self,
        from: &NodeId,
        to: &NodeId,
        label: &str,
        properties: EdgeProperties,
    ) -> DbResult<EdgeId>;

    /// Get an edge by its ID
    async fn get_edge(&self, edge_id: &EdgeId) -> DbResult<Option<Edge>>;

    /// Update edge properties
    async fn update_edge(&self, edge_id: &EdgeId, properties: EdgeProperties) -> DbResult<()>;

    /// Delete an edge
    async fn delete_edge(&self, edge_id: &NodeId) -> DbResult<()>;

    /// Get neighboring nodes (outgoing, incoming, or both)
    async fn get_neighbors(
        &self,
        node_id: &NodeId,
        direction: Direction,
        edge_filter: Option<EdgeFilter>,
    ) -> DbResult<Vec<Node>>;

    /// Traverse graph following a pattern
    async fn traverse(
        &self,
        start: &NodeId,
        pattern: TraversalPattern,
        max_depth: Option<u32>,
    ) -> DbResult<TraversalResult>;

    /// Find all paths between two nodes
    async fn find_paths(
        &self,
        from: &NodeId,
        to: &NodeId,
        max_depth: Option<u32>,
    ) -> DbResult<Vec<Path>>;

    /// Find shortest path between two nodes
    async fn find_shortest_path(&self, from: &NodeId, to: &NodeId) -> DbResult<Option<Path>>;

    /// Perform graph analytics (centrality, clustering, etc.)
    async fn graph_analytics(
        &self,
        nodes: Option<Vec<NodeId>>,
        analysis: GraphAnalysis,
    ) -> DbResult<AnalyticsResult>;

    /// Query for subgraphs matching a pattern
    async fn query_subgraph(&self, pattern: SubgraphPattern) -> DbResult<Vec<Subgraph>>;

    /// Create a graph index for faster traversals
    async fn create_graph_index(&self, label: &str, properties: Vec<String>) -> DbResult<()>;
}

// ==============================================================================
// DOCUMENT DATABASE TRAIT
// ==============================================================================

/// Document database operations trait
///
/// Provides document operations with collections, flexible schemas, search,
/// and aggregations. Best for content management, search, and analytics.
#[async_trait]
pub trait DocumentDB: Send + Sync {
    /// Create a new collection
    async fn create_collection(&self, name: &str, schema: Option<DocumentSchema>) -> DbResult<()>;

    /// Drop an existing collection
    async fn drop_collection(&self, name: &str) -> DbResult<()>;

    /// List all collections
    async fn list_collections(&self) -> DbResult<Vec<String>>;

    /// Create a document in a collection
    async fn create_document(&self, collection: &str, document: Document) -> DbResult<DocumentId>;

    /// Get a document by its ID
    async fn get_document(&self, collection: &str, id: &DocumentId) -> DbResult<Option<Document>>;

    /// Update a document (partial update)
    async fn update_document(
        &self,
        collection: &str,
        id: &DocumentId,
        updates: DocumentUpdates,
    ) -> DbResult<()>;

    /// Replace a document completely
    async fn replace_document(
        &self,
        collection: &str,
        id: &DocumentId,
        document: Document,
    ) -> DbResult<()>;

    /// Delete a document
    async fn delete_document(&self, collection: &str, id: &DocumentId) -> DbResult<()>;

    /// Query documents with filtering, sorting, and pagination
    async fn query_documents(
        &self,
        collection: &str,
        query: DocumentQuery,
    ) -> DbResult<QueryResult>;

    /// Full-text search within documents
    async fn full_text_search(
        &self,
        collection: &str,
        text: &str,
        options: SearchOptions,
    ) -> DbResult<Vec<SearchResult>>;

    /// Aggregate documents using pipeline operations
    async fn aggregate_documents(
        &self,
        collection: &str,
        pipeline: AggregationPipeline,
    ) -> DbResult<AggregationResult>;

    /// Create a text search index
    async fn create_search_index(
        &self,
        collection: &str,
        fields: Vec<String>,
        options: SearchIndexOptions,
    ) -> DbResult<()>;

    /// Bulk insert documents
    async fn insert_documents(
        &self,
        collection: &str,
        documents: Vec<Document>,
    ) -> DbResult<BatchResult>;

    /// Count documents matching a filter
    async fn count_documents(
        &self,
        collection: &str,
        filter: Option<DocumentFilter>,
    ) -> DbResult<u64>;
}

// ==============================================================================
// SHARED TYPES
// ==============================================================================

/// Table schema for relational operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<ColumnDefinition>,
    pub primary_key: Option<String>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<IndexDefinition>,
}

/// Column definition in a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub default_value: Option<serde_json::Value>,
    pub unique: bool,
}

/// Data types supported by the relational model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataType {
    String,
    Integer,
    Float,
    Boolean,
    DateTime,
    Json,
    Array(Box<DataType>),
    UUID,
    Text,
}

/// Foreign key relationship definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKey {
    pub column: String,
    pub references_table: String,
    pub references_column: String,
    pub on_delete: ReferentialAction,
    pub on_update: ReferentialAction,
}

/// Actions for foreign key constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReferentialAction {
    Cascade,
    Restrict,
    SetNull,
    NoAction,
}

/// Index definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDefinition {
    pub columns: Vec<String>,
    pub unique: bool,
    pub index_type: IndexType,
}

/// Index types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndexType {
    BTree,
    Hash,
    FullText,
    Spatial,
}

/// Database record (row)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: Option<RecordId>,
    pub data: HashMap<String, serde_json::Value>,
}

/// Record identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RecordId(pub String);

impl std::fmt::Display for RecordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Query result containing records and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub records: Vec<Record>,
    pub total_count: Option<u64>,
    pub execution_time_ms: Option<u64>,
    pub has_more: bool,
}

/// Select query with filtering, projection, sorting, and pagination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectQuery {
    pub table: String,
    pub columns: Option<Vec<String>>, // None means SELECT *
    pub filter: Option<FilterClause>,
    pub order_by: Option<Vec<OrderClause>>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub joins: Option<Vec<JoinClause>>,
}

/// Filter clause for WHERE conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterClause {
    And(Vec<FilterClause>),
    Or(Vec<FilterClause>),
    Not(Box<FilterClause>),
    Equals {
        column: String,
        value: serde_json::Value,
    },
    NotEquals {
        column: String,
        value: serde_json::Value,
    },
    GreaterThan {
        column: String,
        value: serde_json::Value,
    },
    GreaterThanOrEqual {
        column: String,
        value: serde_json::Value,
    },
    LessThan {
        column: String,
        value: serde_json::Value,
    },
    LessThanOrEqual {
        column: String,
        value: serde_json::Value,
    },
    Like {
        column: String,
        pattern: String,
    },
    In {
        column: String,
        values: Vec<serde_json::Value>,
    },
    IsNull {
        column: String,
    },
    IsNotNull {
        column: String,
    },
    Between {
        column: String,
        start: serde_json::Value,
        end: serde_json::Value,
    },
}

/// Order clause for sorting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderClause {
    pub column: String,
    pub direction: OrderDirection,
}

/// Sort direction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderDirection {
    Asc,
    Desc,
}

/// Update clause for SET operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateClause {
    pub assignments: HashMap<String, serde_json::Value>,
}

/// Join clause for table joins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinClause {
    pub join_type: JoinType,
    pub table: String,
    pub on: FilterClause,
}

/// Join types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

/// Join query with multiple tables
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinQuery {
    pub base_table: String,
    pub joins: Vec<JoinClause>,
    pub columns: Option<Vec<String>>,
    pub filter: Option<FilterClause>,
    pub order_by: Option<Vec<OrderClause>>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Aggregate query for analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateQuery {
    pub table: String,
    pub group_by: Vec<String>,
    pub aggregates: Vec<AggregateFunction>,
    pub filter: Option<FilterClause>,
    pub having: Option<FilterClause>,
    pub order_by: Option<Vec<OrderClause>>,
    pub limit: Option<u32>,
}

/// Aggregate function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateFunction {
    pub function: AggregateType,
    pub column: String,
    pub alias: Option<String>,
}

/// Aggregate function types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregateType {
    Count,
    Sum,
    Average,
    Min,
    Max,
}

impl std::fmt::Display for AggregateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AggregateType::Count => write!(f, "count"),
            AggregateType::Sum => write!(f, "sum"),
            AggregateType::Average => write!(f, "avg"),
            AggregateType::Min => write!(f, "min"),
            AggregateType::Max => write!(f, "max"),
        }
    }
}

/// Transaction identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TransactionId(pub String);

// ==============================================================================
// GRAPH TYPES
// ==============================================================================

/// Node identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeId(pub String);

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Node properties
pub type NodeProperties = HashMap<String, serde_json::Value>;

/// Node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub labels: Vec<String>,
    pub properties: NodeProperties,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Edge identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EdgeId(pub String);

impl std::fmt::Display for EdgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Edge properties
pub type EdgeProperties = HashMap<String, serde_json::Value>;

/// Edge in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub label: String,
    pub properties: EdgeProperties,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Direction for neighbor queries
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}

/// Filter for edges during neighbor queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeFilter {
    pub labels: Option<Vec<String>>,
    pub properties: Option<NodeProperties>,
}

/// Traversal pattern for graph traversals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalPattern {
    pub steps: Vec<TraversalStep>,
}

/// Single step in a traversal pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalStep {
    pub direction: Direction,
    pub edge_filter: Option<EdgeFilter>,
    pub node_filter: Option<NodeProperties>,
    pub min_hops: Option<u32>,
    pub max_hops: Option<u32>,
}

/// Result of a graph traversal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalResult {
    pub paths: Vec<Path>,
    pub total_paths: Option<u64>,
    pub execution_time_ms: Option<u64>,
}

/// Path through the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Path {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub weight: Option<f64>,
}

/// Graph analysis operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphAnalysis {
    DegreeCentrality {
        direction: Direction,
    },
    BetweennessCentrality,
    ClosenessCentrality,
    PageRank {
        damping_factor: Option<f64>,
        iterations: Option<u32>,
    },
    ConnectedComponents,
    StronglyConnectedComponents,
    CommunityDetection {
        algorithm: CommunityAlgorithm,
    },
}

/// Community detection algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommunityAlgorithm {
    Louvain,
    LabelPropagation,
    Infomap,
}

/// Result of graph analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsResult {
    pub analysis: GraphAnalysis,
    pub results: HashMap<NodeId, f64>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Subgraph pattern for matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgraphPattern {
    pub nodes: Vec<NodePattern>,
    pub edges: Vec<EdgePattern>,
}

/// Node pattern for subgraph matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePattern {
    pub variable: String,
    pub labels: Option<Vec<String>>,
    pub properties: Option<NodeProperties>,
}

/// Edge pattern for subgraph matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgePattern {
    pub variable: String,
    pub from_node: String,
    pub to_node: String,
    pub label: Option<String>,
    pub properties: Option<EdgeProperties>,
}

/// Matched subgraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subgraph {
    pub nodes: HashMap<String, Node>,
    pub edges: HashMap<String, Edge>,
}

// ==============================================================================
// DOCUMENT TYPES
// ==============================================================================

/// Document identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DocumentId(pub String);

impl std::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Document schema (optional)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSchema {
    pub fields: Vec<FieldDefinition>,
    pub validation: Option<ValidationRules>,
}

/// Field definition in document schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub name: String,
    pub field_type: DocumentFieldType,
    pub required: bool,
    pub index: Option<bool>,
}

/// Document field types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentFieldType {
    String,
    Integer,
    Float,
    Boolean,
    DateTime,
    Object,
    Array,
    Text,
}

/// Validation rules for documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRules {
    pub max_length: Option<u32>,
    pub min_length: Option<u32>,
    pub pattern: Option<String>,
    pub custom_rules: HashMap<String, serde_json::Value>,
}

/// Document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: Option<DocumentId>,
    pub content: serde_json::Value,
    pub metadata: DocumentMetadata,
}

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub version: u32,
    pub content_type: Option<String>,
    pub tags: Vec<String>,
    pub collection: Option<String>,
}

/// Document query operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentQuery {
    pub collection: String,
    pub filter: Option<DocumentFilter>,
    pub projection: Option<Vec<String>>,
    pub sort: Option<Vec<DocumentSort>>,
    pub limit: Option<u32>,
    pub skip: Option<u32>,
}

/// Document filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentFilter {
    And(Vec<DocumentFilter>),
    Or(Vec<DocumentFilter>),
    Not(Box<DocumentFilter>),
    Equals {
        field: String,
        value: serde_json::Value,
    },
    NotEquals {
        field: String,
        value: serde_json::Value,
    },
    GreaterThan {
        field: String,
        value: serde_json::Value,
    },
    GreaterThanOrEqual {
        field: String,
        value: serde_json::Value,
    },
    LessThan {
        field: String,
        value: serde_json::Value,
    },
    LessThanOrEqual {
        field: String,
        value: serde_json::Value,
    },
    Contains {
        field: String,
        value: serde_json::Value,
    },
    In {
        field: String,
        values: Vec<serde_json::Value>,
    },
    Exists {
        field: String,
    },
    ElementType {
        field: String,
        element_type: DocumentFieldType,
    },
}

/// Document sort specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSort {
    pub field: String,
    pub direction: OrderDirection,
}

/// Document updates (partial updates)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentUpdates {
    pub set: Option<HashMap<String, serde_json::Value>>,
    pub unset: Option<Vec<String>>,
    pub increment: Option<HashMap<String, serde_json::Value>>,
    pub push: Option<HashMap<String, Vec<serde_json::Value>>>,
    pub pull: Option<HashMap<String, Vec<serde_json::Value>>>,
}

/// Search options for full-text search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    pub fields: Option<Vec<String>>,
    pub fuzzy: Option<bool>,
    pub boost_fields: Option<HashMap<String, f64>>,
    pub limit: Option<u32>,
    pub highlight: Option<bool>,
}

/// Search index options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexOptions {
    pub analyzer: Option<String>,
    pub tokenizer: Option<String>,
    pub filters: Option<Vec<String>>,
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub document_id: DocumentId,
    pub score: f64,
    pub highlights: Option<Vec<String>>,
    pub snippet: Option<String>,
}

/// Aggregation pipeline for document analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationPipeline {
    pub stages: Vec<AggregationStage>,
}

/// Aggregation pipeline stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregationStage {
    Match {
        filter: DocumentFilter,
    },
    Group {
        id: serde_json::Value,
        operations: Vec<GroupOperation>,
    },
    Sort {
        sort: Vec<DocumentSort>,
    },
    Limit {
        limit: u32,
    },
    Skip {
        skip: u32,
    },
    Project {
        projection: Vec<String>,
    },
    Unwind {
        field: String,
    },
    Lookup {
        from: String,
        local_field: String,
        foreign_field: String,
        as_field: String,
    },
}

/// Group operation in aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupOperation {
    pub field: String,
    pub operation: AggregateType,
    pub alias: Option<String>,
}

/// Result of document aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationResult {
    pub results: Vec<serde_json::Value>,
    pub total_count: Option<u64>,
    pub execution_time_ms: Option<u64>,
}

/// Batch operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub successful: u64,
    pub failed: u64,
    pub errors: Vec<String>,
}

// ==============================================================================
// LEGACY TYPES FOR database.rs (SurrealEmbeddingDatabase)
// ==============================================================================
// These types support the old SurrealEmbeddingDatabase implementation
// and will be deprecated once we fully migrate to SurrealClient.

/// Embedding metadata for documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingMetadata {
    pub file_path: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub folder: String,
    pub properties: std::collections::HashMap<String, serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Embedding document with all fields needed for vector operations
/// This is the SurrealDB-specific document type that includes embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingDocument {
    pub id: String,
    pub file_path: String,
    pub title: Option<String>,
    pub content: String,
    pub embedding: Vec<f32>,
    pub tags: Vec<String>,
    pub folder: String,
    pub properties: std::collections::HashMap<String, serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Embedding data with content and vector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    pub file_path: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: EmbeddingMetadata,
}

impl From<EmbeddingDocument> for EmbeddingData {
    fn from(doc: EmbeddingDocument) -> Self {
        EmbeddingData {
            file_path: doc.file_path.clone(),
            content: doc.content,
            embedding: doc.embedding,
            metadata: EmbeddingMetadata {
                file_path: doc.file_path,
                title: doc.title,
                tags: doc.tags,
                folder: doc.folder,
                properties: doc.properties,
                created_at: doc.created_at,
                updated_at: doc.updated_at,
            },
        }
    }
}

/// Search query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub filters: Option<serde_json::Value>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Search result with similarity score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultWithScore {
    pub id: String,
    pub title: String,
    pub file_path: String,
    pub content: String,
    pub score: f64,
    pub metadata: EmbeddingMetadata,
}

/// Batch operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchOperationType {
    Create,
    Update,
    Delete,
}

/// Search filters for embedding queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFilters {
    pub tags: Option<Vec<String>>,
    pub folder: Option<String>,
    pub properties: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// Batch operation with embedding documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperation {
    pub operation_type: BatchOperationType,
    pub documents: Vec<EmbeddingDocument>,
}

/// Database statistics
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseStats {
    pub total_documents: u64,
    pub total_embeddings: u64,
    pub storage_size_bytes: u64,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}
