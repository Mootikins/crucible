// crates/crucible-mcp/tests/test_multi_model_helpers.rs
//
// Test Helper Infrastructure for Multi-Model Database MCP Integration
//
// This module provides utilities for testing MCP service integration with
// the multi-model database (SurrealClient). It follows TDD RED-GREEN-REFACTOR
// approach for Phase 1B of the execution plan.

use anyhow::Result;
use async_trait::async_trait;
use crucible_core::{
    ColumnDefinition, DataType, DocumentDB, DocumentMetadata, GraphDB, NodeId, RecordId,
    RelationalDB, TableSchema,
};
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::embeddings::{EmbeddingProvider, EmbeddingResponse, EmbeddingResult};
use crucible_mcp::service::CrucibleMcpService;
use crucible_surrealdb::SurrealClient;
use rmcp::model::{CallToolResult, Content};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

// =============================================================================
// Test Context
// =============================================================================

/// Comprehensive test context for multi-model MCP testing
///
/// This struct encapsulates all dependencies needed for integration tests,
/// including temporary directory, databases, and the MCP service itself.
pub struct TestContext {
    pub temp_dir: TempDir,
    pub embedding_db: Arc<EmbeddingDatabase>,
    pub mock_provider: Arc<dyn EmbeddingProvider>,
    pub surreal_client: Arc<SurrealClient>,
    pub mcp_service: CrucibleMcpService,
}

impl TestContext {
    /// Create a new test context with in-memory SurrealDB
    ///
    /// This initializes all components needed for testing:
    /// - Temporary directory for file-based resources
    /// - EmbeddingDatabase for vault operations
    /// - In-memory SurrealClient for multi-model operations
    /// - CrucibleMcpService with multi-model support enabled
    pub async fn new() -> Result<Self> {
        // Create temp directory for embedding database
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("test_embeddings.db");

        // Initialize embedding database
        let embedding_db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await?);

        // Create in-memory SurrealClient
        let surreal_client = Arc::new(SurrealClient::new_memory().await?);

        // Create mock embedding provider
        let mock_provider = Arc::new(MockEmbeddingProvider::new());

        // Create MCP service with multi-model support
        let mcp_service = CrucibleMcpService::with_multi_model(
            Arc::clone(&embedding_db),
            Arc::clone(&mock_provider) as Arc<dyn EmbeddingProvider>,
            Arc::clone(&surreal_client),
        );

        Ok(Self {
            temp_dir,
            embedding_db,
            mock_provider,
            surreal_client,
            mcp_service,
        })
    }

    /// Create a test context pre-populated with realistic test data
    ///
    /// This sets up a full multi-model dataset with:
    /// - Relational tables (users, posts, tags)
    /// - Graph nodes and edges (User->Post->Tag relationships)
    /// - Document collections (profiles, settings)
    pub async fn with_test_data() -> Result<Self> {
        let ctx = Self::new().await?;
        ctx.setup_test_data().await?;
        Ok(ctx)
    }

    /// Setup comprehensive test data across all three database models
    ///
    /// Creates:
    /// - Relational: users, posts, tags tables with sample records
    /// - Graph: User, Post, Tag nodes with AUTHORED, TAGGED_WITH edges
    /// - Document: profiles, settings collections with JSON documents
    pub async fn setup_test_data(&self) -> Result<TestData> {
        // Create relational tables
        self.create_users_table().await?;
        self.create_posts_table().await?;
        self.create_tags_table().await?;

        // Insert relational data
        let user_id = self.insert_test_user("Alice", "alice@example.com", 30, "active").await?;
        let post_id = self.insert_test_post("First Post", "Content about Rust", &user_id).await?;
        let tag_id = self.insert_test_tag("rust").await?;

        // Create graph nodes
        let user_node_id = self
            .surreal_client
            .create_node(
                "User",
                HashMap::from([
                    ("name".to_string(), serde_json::json!("Alice")),
                    ("email".to_string(), serde_json::json!("alice@example.com")),
                ]),
            )
            .await?;

        let post_node_id = self
            .surreal_client
            .create_node(
                "Post",
                HashMap::from([
                    ("title".to_string(), serde_json::json!("First Post")),
                    ("content".to_string(), serde_json::json!("Content about Rust")),
                ]),
            )
            .await?;

        let tag_node_id = self
            .surreal_client
            .create_node(
                "Tag",
                HashMap::from([("name".to_string(), serde_json::json!("rust"))]),
            )
            .await?;

        // Create graph edges
        self.surreal_client
            .create_edge(
                &user_node_id,
                &post_node_id,
                "AUTHORED",
                HashMap::from([("created_at".to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()))]),
            )
            .await?;

        self.surreal_client
            .create_edge(
                &post_node_id,
                &tag_node_id,
                "TAGGED_WITH",
                HashMap::new(),
            )
            .await?;

        // Create document collections
        self.surreal_client
            .create_collection("profiles", None)
            .await?;

        // Create profile document
        let profile_doc = crucible_core::Document {
            id: None,
            content: serde_json::json!({
                "user_id": user_id.0,
                "bio": "Software engineer specializing in Rust",
                "skills": ["Rust", "Databases", "Distributed Systems"],
                "experience_years": 5
            }),
            metadata: DocumentMetadata {
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                version: 1,
                content_type: Some("application/json".to_string()),
                tags: vec!["profile".to_string(), "engineer".to_string()],
                collection: Some("profiles".to_string()),
            },
        };

        let profile_doc_id = self
            .surreal_client
            .create_document("profiles", profile_doc)
            .await?;

        Ok(TestData {
            user_id,
            post_id,
            tag_id,
            user_node_id,
            post_node_id,
            tag_node_id,
            profile_doc_id,
        })
    }

    // Helper methods for creating relational tables

    async fn create_users_table(&self) -> Result<()> {
        let schema = TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    data_type: DataType::Integer,
                    nullable: false,
                    default_value: None,
                    unique: true,
                },
                ColumnDefinition {
                    name: "name".to_string(),
                    data_type: DataType::String,
                    nullable: false,
                    default_value: None,
                    unique: false,
                },
                ColumnDefinition {
                    name: "email".to_string(),
                    data_type: DataType::String,
                    nullable: false,
                    default_value: None,
                    unique: true,
                },
                ColumnDefinition {
                    name: "age".to_string(),
                    data_type: DataType::Integer,
                    nullable: true,
                    default_value: None,
                    unique: false,
                },
                ColumnDefinition {
                    name: "status".to_string(),
                    data_type: DataType::String,
                    nullable: false,
                    default_value: Some(serde_json::json!("active")),
                    unique: false,
                },
            ],
            primary_key: Some("id".to_string()),
            foreign_keys: vec![],
            indexes: vec![],
        };

        self.surreal_client.create_table("users", schema).await?;
        Ok(())
    }

    async fn create_posts_table(&self) -> Result<()> {
        let schema = TableSchema {
            name: "posts".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    data_type: DataType::Integer,
                    nullable: false,
                    default_value: None,
                    unique: true,
                },
                ColumnDefinition {
                    name: "title".to_string(),
                    data_type: DataType::String,
                    nullable: false,
                    default_value: None,
                    unique: false,
                },
                ColumnDefinition {
                    name: "content".to_string(),
                    data_type: DataType::String,
                    nullable: false,
                    default_value: None,
                    unique: false,
                },
                ColumnDefinition {
                    name: "user_id".to_string(),
                    data_type: DataType::Integer,
                    nullable: false,
                    default_value: None,
                    unique: false,
                },
            ],
            primary_key: Some("id".to_string()),
            foreign_keys: vec![],
            indexes: vec![],
        };

        self.surreal_client.create_table("posts", schema).await?;
        Ok(())
    }

    async fn create_tags_table(&self) -> Result<()> {
        let schema = TableSchema {
            name: "tags".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    data_type: DataType::Integer,
                    nullable: false,
                    default_value: None,
                    unique: true,
                },
                ColumnDefinition {
                    name: "name".to_string(),
                    data_type: DataType::String,
                    nullable: false,
                    default_value: None,
                    unique: true,
                },
            ],
            primary_key: Some("id".to_string()),
            foreign_keys: vec![],
            indexes: vec![],
        };

        self.surreal_client.create_table("tags", schema).await?;
        Ok(())
    }

    // Helper methods for inserting test data

    async fn insert_test_user(
        &self,
        name: &str,
        email: &str,
        age: i32,
        status: &str,
    ) -> Result<RecordId> {
        let record = crucible_core::Record {
            id: None,
            data: HashMap::from([
                ("name".to_string(), serde_json::json!(name)),
                ("email".to_string(), serde_json::json!(email)),
                ("age".to_string(), serde_json::json!(age)),
                ("status".to_string(), serde_json::json!(status)),
            ]),
        };

        let result = self.surreal_client.insert("users", record).await?;
        Ok(result.records[0].id.clone().unwrap())
    }

    async fn insert_test_post(&self, title: &str, content: &str, user_id: &RecordId) -> Result<RecordId> {
        let record = crucible_core::Record {
            id: None,
            data: HashMap::from([
                ("title".to_string(), serde_json::json!(title)),
                ("content".to_string(), serde_json::json!(content)),
                ("user_id".to_string(), serde_json::json!(user_id.0.clone())),
            ]),
        };

        let result = self.surreal_client.insert("posts", record).await?;
        Ok(result.records[0].id.clone().unwrap())
    }

    async fn insert_test_tag(&self, name: &str) -> Result<RecordId> {
        let record = crucible_core::Record {
            id: None,
            data: HashMap::from([("name".to_string(), serde_json::json!(name))]),
        };

        let result = self.surreal_client.insert("tags", record).await?;
        Ok(result.records[0].id.clone().unwrap())
    }
}

// =============================================================================
// Test Data Structure
// =============================================================================

/// Test data identifiers for cross-model scenarios
///
/// This struct holds IDs for test data created across all three models,
/// allowing tests to verify cross-model relationships and queries.
#[derive(Debug, Clone)]
pub struct TestData {
    pub user_id: RecordId,
    pub post_id: RecordId,
    pub tag_id: RecordId,
    pub user_node_id: NodeId,
    pub post_node_id: NodeId,
    pub tag_node_id: NodeId,
    pub profile_doc_id: crucible_core::DocumentId,
}

// =============================================================================
// Mock Embedding Provider
// =============================================================================

/// Mock embedding provider for testing
///
/// Returns deterministic dummy embeddings suitable for testing.
/// Does not require actual ML models or external API calls.
pub struct MockEmbeddingProvider {
    dimensions: usize,
}

impl MockEmbeddingProvider {
    pub fn new() -> Self {
        Self { dimensions: 384 }
    }

    pub fn with_dimensions(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        // Generate deterministic embedding based on text length
        let mut embedding = vec![0.1; self.dimensions];
        let text_hash = text.len() as f32 / 100.0;
        for (i, val) in embedding.iter_mut().enumerate() {
            *val += (i as f32 * text_hash).sin() * 0.1;
        }
        Ok(EmbeddingResponse::new(embedding, "mock-model".to_string()))
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        let mut results = Vec::new();
        for text in texts {
            results.push(self.embed(&text).await?);
        }
        Ok(results)
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn provider_name(&self) -> &str {
        "MockProvider"
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        Ok(true)
    }
}

// =============================================================================
// MCP Tool Call Helpers
// =============================================================================

/// Helper to call an MCP tool and parse the result
///
/// This function wraps the MCP service call_tool method with proper
/// request context creation for testing.
///
/// NOTE: This is a placeholder for Phase 1B (RED phase).
/// The actual implementation will be completed in Phase 1C (GREEN phase)
/// when we have actual tools to test against.
pub async fn call_tool(
    _service: &CrucibleMcpService,
    _tool_name: &str,
    _params: serde_json::Value,
) -> Result<CallToolResult> {
    // TODO: Implement in Phase 1C when tools are available
    // For now, return a placeholder error to make tests fail as expected in RED phase
    anyhow::bail!("Tool calling not yet implemented - placeholder for Phase 1B (RED phase)")
}

// =============================================================================
// Assertion Helpers
// =============================================================================

pub mod assertions {
    use super::*;

    /// Assert that a tool call succeeded
    pub fn assert_tool_success(result: &CallToolResult) {
        assert!(
            !result.is_error.unwrap_or(false),
            "Tool call failed: {:?}",
            result
        );
    }

    /// Assert that a tool call failed
    pub fn assert_tool_error(result: &CallToolResult) {
        assert!(
            result.is_error.unwrap_or(false),
            "Expected tool to fail but it succeeded: {:?}",
            result
        );
    }

    /// Extract text content from tool result
    pub fn extract_text_content(result: &CallToolResult) -> Option<String> {
        result.content.first().and_then(|content| {
            // Content is wrapped in Annotated<RawContent>
            // RawContent has a text field for TextContent variant
            match &content.raw {
                rmcp::model::RawContent::Text(text_content) => Some(text_content.text.clone()),
                _ => None,
            }
        })
    }

    /// Parse JSON from tool result
    pub fn parse_json_result<T: serde::de::DeserializeOwned>(
        result: &CallToolResult,
    ) -> Result<T> {
        let text = extract_text_content(result)
            .ok_or_else(|| anyhow::anyhow!("No text content in result"))?;

        serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("Failed to parse JSON: {}", e))
    }

    /// Assert that a record contains a specific field with value
    pub fn assert_record_contains(
        record: &serde_json::Value,
        field: &str,
        expected_value: &serde_json::Value,
    ) {
        let actual = record.get(field).expect(&format!("Field '{}' not found in record", field));
        assert_eq!(
            actual, expected_value,
            "Field '{}' has wrong value. Expected {:?}, got {:?}",
            field, expected_value, actual
        );
    }

    /// Assert that a node has a specific label
    pub fn assert_node_has_label(node: &serde_json::Value, expected_label: &str) {
        let label = node
            .get("label")
            .and_then(|v| v.as_str())
            .expect("Node has no label field");
        assert_eq!(
            label, expected_label,
            "Node has wrong label. Expected {}, got {}",
            expected_label, label
        );
    }

    /// Assert that a document has a specific field
    pub fn assert_document_has_field(doc: &serde_json::Value, field: &str) {
        assert!(
            doc.get(field).is_some(),
            "Document missing field '{}'",
            field
        );
    }

    /// Assert that a result contains a specific number of records
    pub fn assert_record_count(result: &serde_json::Value, expected_count: usize) {
        let records = result
            .get("records")
            .and_then(|v| v.as_array())
            .expect("Result has no 'records' array");
        assert_eq!(
            records.len(),
            expected_count,
            "Expected {} records, got {}",
            expected_count,
            records.len()
        );
    }
}

// =============================================================================
// Performance Helpers
// =============================================================================

/// Performance timer for measuring operation duration
///
/// Used for performance testing and benchmarking cross-model operations.
pub struct PerformanceTimer {
    start: std::time::Instant,
    label: String,
}

impl PerformanceTimer {
    /// Start a new performance timer with a label
    pub fn start(label: impl Into<String>) -> Self {
        Self {
            start: std::time::Instant::now(),
            label: label.into(),
        }
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }

    /// Assert that operation completed within expected duration
    pub fn assert_duration_ms(&self, max_ms: u128) {
        let elapsed = self.elapsed_ms();
        assert!(
            elapsed <= max_ms,
            "{} took {}ms, expected <= {}ms",
            self.label,
            elapsed,
            max_ms
        );
    }

    /// Print elapsed time (useful for baseline measurements)
    pub fn print_elapsed(&self) {
        println!("{}: {}ms", self.label, self.elapsed_ms());
    }
}

// =============================================================================
// Tests for Test Helpers
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_creation() {
        let ctx = TestContext::new().await.unwrap();
        // Verify SurrealClient is accessible through test context
        assert!(Arc::strong_count(&ctx.surreal_client) > 0);
    }

    #[tokio::test]
    async fn test_context_with_test_data() {
        let ctx = TestContext::with_test_data().await.unwrap();

        // Verify relational data exists
        let user_exists = ctx
            .surreal_client
            .select(crucible_core::SelectQuery {
                table: "users".to_string(),
                columns: None,
                filter: None,
                order_by: None,
                limit: None,
                offset: None,
                joins: None,
            })
            .await
            .unwrap();

        assert_eq!(user_exists.records.len(), 1);
    }

    #[tokio::test]
    async fn test_mock_embedding_provider() {
        let provider = MockEmbeddingProvider::new();
        let result = provider.embed("test text").await.unwrap();
        assert_eq!(result.embedding.len(), 384);
    }

    #[tokio::test]
    async fn test_performance_timer() {
        let timer = PerformanceTimer::start("test operation");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 10);
    }

    #[tokio::test]
    async fn test_assertion_helpers() {
        let success_result = CallToolResult::success(vec![Content::text("test".to_string())]);
        assertions::assert_tool_success(&success_result);

        let error_result = CallToolResult::error(vec![Content::text("error".to_string())]);
        assertions::assert_tool_error(&error_result);
    }
}
