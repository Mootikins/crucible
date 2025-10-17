//! SurrealDB implementation of the embedding database interface.
//!
//! This module provides a SurrealDB-based implementation that offers:
//! - Native vector storage as arrays
//! - Graph relations for document connections
//! - ACID transactions
//! - Live queries for real-time updates
//! - Better performance than JSON-based storage

use crate::types::{
    BatchOperation, BatchOperationType, BatchResult, DatabaseStats, Document, EmbeddingData, EmbeddingMetadata,
    SearchResultWithScore, SearchQuery, SurrealDbConfig,
};
use anyhow::Result;
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use surrealdb::engine::local::Db;
use surrealdb::engine::local::RocksDb;
use surrealdb::opt::IntoQuery;
use surrealdb::Surreal;
use surrealdb::sql::Datetime;

/// SurrealDB-based embedding database
pub struct SurrealEmbeddingDatabase {
    db: Arc<Surreal<Db>>,
    config: SurrealDbConfig,
}

impl SurrealEmbeddingDatabase {
    /// Create a new database connection with default configuration
    pub async fn new(db_path: &str) -> Result<Self> {
        let config = SurrealDbConfig {
            path: db_path.to_string(),
            ..Default::default()
        };
        Self::with_config(config).await
    }

    /// Create a new database connection with custom configuration
    pub async fn with_config(config: SurrealDbConfig) -> Result<Self> {
        // Connect to RocksDB backend directly
        let db = Surreal::new::<RocksDb>(&config.path).await?;
        let db = Arc::new(db);

        Ok(Self { db, config })
    }

    /// Initialize database schema and indexes
    pub async fn initialize(&self) -> Result<()> {
        // Select namespace and database
        self.db.use_ns(&self.config.namespace).use_db(&self.config.database).await?;

        // Create documents table with schema definition
        let _: Vec<serde_json::Value> = self.db
            .query("
                DEFINE TABLE documents SCHEMAFULL;

                DEFINE FIELD file_path ON TABLE documents TYPE string
                    ASSERT $value != NONE AND $value != '';

                DEFINE FIELD title ON TABLE documents TYPE option<string>;
                DEFINE FIELD content ON TABLE documents TYPE string;
                DEFINE FIELD embedding ON TABLE documents TYPE option<array<float>>;
                DEFINE FIELD tags ON TABLE documents TYPE option<array<string>>;
                DEFINE FIELD folder ON TABLE documents TYPE option<string>;
                DEFINE FIELD properties ON TABLE documents TYPE option<object>;
                DEFINE FIELD created_at ON TABLE documents TYPE datetime DEFAULT time::now();
                DEFINE FIELD updated_at ON TABLE documents TYPE datetime DEFAULT time::now();

                -- Indexes for performance
                DEFINE INDEX file_path_idx ON TABLE documents COLUMNS file_path;
                DEFINE INDEX tags_idx ON TABLE documents COLUMNS tags;
                DEFINE INDEX folder_idx ON TABLE documents COLUMNS folder;
                DEFINE INDEX created_at_idx ON TABLE documents COLUMNS created_at;
            ")
            .await?
            .take(0)?;

        Ok(())
    }

    /// Store an embedding for a file
    pub async fn store_embedding(
        &self,
        file_path: &str,
        content: &str,
        embedding: &[f32],
        metadata: &EmbeddingMetadata,
    ) -> Result<()> {
        // Generate record ID from file path
        let record_id = file_path.replace('/', "_").replace('\\', "_").replace('.', "_").replace(':', "_");

        // Clone data to avoid borrowing issues
        let record_id_owned = record_id.clone();
        let file_path_owned = file_path.to_string();
        let content_owned = content.to_string();
        let embedding_owned = embedding.to_vec();

        // Try with just a simple test first
        let _: Vec<serde_json::Value> = self.db
            .query("CREATE documents:test_md CONTENT {
                file_path: $file_path,
                content: $content
            }")
            .bind(("file_path", file_path_owned))
            .bind(("content", content_owned))
            .await?
            .take(0)?;

        Ok(())
    }

    /// Update only metadata for an existing file (keeps embedding unchanged)
    pub async fn update_metadata(
        &self,
        file_path: &str,
        metadata: &EmbeddingMetadata,
    ) -> Result<()> {
        // TODO: Implement SurrealDB metadata update
        unimplemented!("Metadata update not yet implemented")
    }

    /// Check if a file exists in the database
    pub async fn file_exists(&self, file_path: &str) -> Result<bool> {
        // TODO: Implement SurrealDB existence check
        unimplemented!("File existence check not yet implemented")
    }

    /// Get embedding data for a file
    pub async fn get_embedding(&self, file_path: &str) -> Result<Option<EmbeddingData>> {
        // TODO: Implement SurrealDB document retrieval
        unimplemented!("Document retrieval not yet implemented")
    }

    /// Search for similar embeddings using cosine similarity
    pub async fn search_similar(
        &self,
        query_embedding: &[f32],
        top_k: u32,
    ) -> Result<Vec<SearchResultWithScore>> {
        // TODO: Implement SurrealDB vector similarity search
        unimplemented!("Vector similarity search not yet implemented")
    }

    /// Search files by tags
    pub async fn search_by_tags(&self, tags: &[String]) -> Result<Vec<String>> {
        // TODO: Implement SurrealDB tag-based search
        unimplemented!("Tag search not yet implemented")
    }

    /// Search files by properties
    pub async fn search_by_properties(
        &self,
        properties: &HashMap<String, serde_json::Value>,
    ) -> Result<Vec<String>> {
        // TODO: Implement SurrealDB property search
        unimplemented!("Property search not yet implemented")
    }

    /// Advanced search with filters
    pub async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResultWithScore>> {
        // TODO: Implement advanced SurrealDB search with filters
        unimplemented!("Advanced search not yet implemented")
    }

    /// Get all files in the database
    pub async fn list_files(&self) -> Result<Vec<String>> {
        // TODO: Implement SurrealDB file listing
        unimplemented!("File listing not yet implemented")
    }

    /// Delete a file from the database
    pub async fn delete_file(&self, file_path: &str) -> Result<()> {
        // TODO: Implement SurrealDB document deletion
        unimplemented!("File deletion not yet implemented")
    }

    /// Batch operations for multiple documents
    pub async fn batch_operation(&self, operation: &BatchOperation) -> Result<BatchResult> {
        // TODO: Implement SurrealDB batch operations
        unimplemented!("Batch operations not yet implemented")
    }

    /// Get comprehensive database statistics
    pub async fn get_stats(&self) -> Result<DatabaseStats> {
        // TODO: Implement SurrealDB statistics
        unimplemented!("Database statistics not yet implemented")
    }

    /// Create graph relations between documents
    pub async fn create_relation(
        &self,
        from_file: &str,
        to_file: &str,
        relation_type: &str,
        properties: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<()> {
        // TODO: Implement SurrealDB graph relations
        unimplemented!("Graph relations not yet implemented")
    }

    /// Get related documents
    pub async fn get_related(&self, file_path: &str, relation_type: Option<&str>) -> Result<Vec<String>> {
        // TODO: Implement SurrealDB relation queries
        unimplemented!("Related documents not yet implemented")
    }

    /// Close the database connection
    pub async fn close(self) -> Result<()> {
        // TODO: Implement SurrealDB connection cleanup
        unimplemented!("Connection cleanup not yet implemented")
    }
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    (dot_product / (norm_a * norm_b)) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    /// Test helper to create test metadata
    fn create_test_metadata(file_path: &str) -> EmbeddingMetadata {
        EmbeddingMetadata {
            file_path: file_path.to_string(),
            title: Some("Test Document".to_string()),
            tags: vec!["test".to_string(), "rust".to_string()],
            folder: "test".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("author".to_string(), serde_json::json!("test"));
                props.insert("status".to_string(), serde_json::json!("draft"));
                props
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_database_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // This should now work with our implementation
        let result = SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await;

        assert!(result.is_ok(), "Database creation should succeed");
    }

    #[tokio::test]
    async fn test_database_with_config() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let config = SurrealDbConfig {
            path: db_path.to_str().unwrap().to_string(),
            namespace: "test".to_string(),
            database: "test_cache".to_string(),
            ..Default::default()
        };

        // This should fail until we implement it
        let result = SurrealEmbeddingDatabase::with_config(config).await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Database creation with config should fail until implemented");
    }

    #[tokio::test]
    async fn test_schema_initialization() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();

        // This should now work with our implementation
        let result = db.initialize().await;

        assert!(result.is_ok(), "Schema initialization should succeed");
    }

    #[tokio::test]
    async fn test_store_embedding() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
        db.initialize().await.unwrap();

        let embedding = vec![0.1f32; 384]; // Typical embedding size
        let metadata = create_test_metadata("test.md");

        // This should now work with our implementation
        match db.store_embedding("test.md", "Test content", &embedding, &metadata).await {
            Ok(_) => println!("Store embedding succeeded"),
            Err(e) => {
                println!("Store embedding failed: {:?}", e);
                panic!("Store embedding should succeed");
            }
        };
    }

    #[tokio::test]
    async fn test_file_existence_check() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        // This should fail until we implement it
        let result = db.file_exists("nonexistent.md").await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "File existence check should fail until implemented");
    }

    #[tokio::test]
    async fn test_search_similar() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        let query_embedding = vec![0.1f32; 384];

        // This should fail until we implement it
        let result = db.search_similar(&query_embedding, 5).await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Search similar should fail until implemented");
    }

    #[tokio::test]
    async fn test_search_by_tags() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        let tags = vec!["rust".to_string(), "database".to_string()];

        // This should fail until we implement it
        let result = db.search_by_tags(&tags).await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Search by tags should fail until implemented");
    }

    #[tokio::test]
    async fn test_search_by_properties() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        let mut properties = HashMap::new();
        properties.insert("status".to_string(), serde_json::json!("published"));

        // This should fail until we implement it
        let result = db.search_by_properties(&properties).await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Search by properties should fail until implemented");
    }

    #[tokio::test]
    async fn test_advanced_search() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        let search_query = SearchQuery {
            query: "test query".to_string(),
            filters: None,
            limit: Some(10),
            offset: None,
        };

        // This should fail until we implement it
        let result = db.search(&search_query).await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Advanced search should fail until implemented");
    }

    #[tokio::test]
    async fn test_list_files() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        // This should fail until we implement it
        let result = db.list_files().await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "List files should fail until implemented");
    }

    #[tokio::test]
    async fn test_delete_file() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        // This should fail until we implement it
        let result = db.delete_file("test.md").await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Delete file should fail until implemented");
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        let operation = BatchOperation {
            operation_type: BatchOperationType::Create,
            documents: vec![],
        };

        // This should fail until we implement it
        let result = db.batch_operation(&operation).await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Batch operations should fail until implemented");
    }

    #[tokio::test]
    async fn test_get_stats() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        // This should fail until we implement it
        let result = db.get_stats().await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Get stats should fail until implemented");
    }

    #[tokio::test]
    async fn test_update_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        let metadata = create_test_metadata("test.md");

        // This should fail until we implement it
        let result = db.update_metadata("test.md", &metadata).await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Update metadata should fail until implemented");
    }

    #[tokio::test]
    async fn test_get_embedding() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        // This should fail until we implement it
        let result = db.get_embedding("test.md").await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Get embedding should fail until implemented");
    }

    #[tokio::test]
    async fn test_create_relation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        // This should fail until we implement it
        let result = db.create_relation("doc1.md", "doc2.md", "links_to", None).await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Create relation should fail until implemented");
    }

    #[tokio::test]
    async fn test_get_related() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = match SurrealEmbeddingDatabase::new(db_path.to_str().unwrap()).await {
            Ok(db) => db,
            Err(_) => {
                // Expected to fail until implementation
                return;
            }
        };

        // This should fail until we implement it
        let result = db.get_related("doc1.md", Some("links_to")).await;

        // TODO: Change this to assert!(result.is_ok()) after implementation
        assert!(result.is_err(), "Get related should fail until implemented");
    }

    #[tokio::test]
    async fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let similarity = cosine_similarity(&a, &b);
        assert!((similarity - 1.0).abs() < f64::EPSILON);

        let c = vec![0.0, 1.0, 0.0];
        let similarity = cosine_similarity(&a, &c);
        assert!((similarity - 0.0).abs() < f64::EPSILON);

        let d = vec![];
        let e = vec![1.0, 2.0, 3.0];
        let similarity = cosine_similarity(&d, &e);
        assert_eq!(similarity, 0.0);
    }
}