use crate::types::{EmbeddingData, EmbeddingMetadata, SearchResultWithScore};
use anyhow::Result;
use duckdb::Connection;
use serde_json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Database for storing embeddings and metadata using DuckDB
pub struct EmbeddingDatabase {
    conn: Arc<Mutex<Connection>>,
}

impl EmbeddingDatabase {
    /// Create a new database connection
    pub async fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.initialize().await?;
        Ok(db)
    }

    /// Initialize database schema
    pub async fn initialize(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Load VSS extension for vector operations
        conn.execute_batch("INSTALL vss; LOAD vss;")?;

        // Create sequence for auto-incrementing IDs
        conn.execute_batch(
            r#"
            CREATE SEQUENCE IF NOT EXISTS embeddings_id_seq;
            "#,
        )?;

        // Create embeddings table with JSON for embeddings (temporary approach)
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS embeddings (
                id BIGINT PRIMARY KEY DEFAULT nextval('embeddings_id_seq'),
                file_path TEXT UNIQUE NOT NULL,
                content TEXT NOT NULL,
                embedding JSON NOT NULL,
                metadata JSON NOT NULL,
                created_at TIMESTAMP,
                updated_at TIMESTAMP
            )
            "#,
        )?;

        // Create indexes for performance
        conn.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_file_path ON embeddings(file_path);
            CREATE INDEX IF NOT EXISTS idx_created_at ON embeddings(created_at);
            "#,
        )?;

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
        let conn = self.conn.lock().unwrap();
        let metadata_json = serde_json::to_string(metadata)?;

        let now = chrono::Utc::now().to_rfc3339();

        // For now, use a simpler approach with JSON storage
        // TODO: Implement proper individual column storage
        let embedding_json = serde_json::to_string(embedding)?;

        conn.execute(
            r#"
            INSERT INTO embeddings (file_path, content, embedding, metadata, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT (file_path) 
            DO UPDATE SET 
                content = excluded.content,
                embedding = excluded.embedding,
                metadata = excluded.metadata,
                updated_at = excluded.updated_at
            "#,
            [
                file_path,
                content,
                &embedding_json,
                &metadata_json,
                &now,
                &now,
            ],
        )?;

        Ok(())
    }

    /// Update only metadata for an existing file (keeps embedding unchanged)
    pub async fn update_metadata(
        &self,
        file_path: &str,
        metadata: &EmbeddingMetadata,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let metadata_json = serde_json::to_string(metadata)?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            r#"
            UPDATE embeddings
            SET metadata = ?, updated_at = ?
            WHERE file_path = ?
            "#,
            [&metadata_json, &now, file_path],
        )?;

        Ok(())
    }

    /// Check if a file exists in the database
    pub async fn file_exists(&self, file_path: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT 1 FROM embeddings WHERE file_path = ?")?;
        let mut rows = stmt.query([file_path])?;

        Ok(rows.next()?.is_some())
    }

    /// Get embedding data for a file
    pub async fn get_embedding(&self, file_path: &str) -> Result<Option<EmbeddingData>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT file_path, content, embedding, metadata FROM embeddings WHERE file_path = ?",
        )?;
        let mut rows = stmt.query([file_path])?;

        if let Some(row) = rows.next()? {
            let file_path: String = row.get(0)?;
            let content: String = row.get(1)?;
            let embedding_json: String = row.get(2)?;
            let metadata_json: String = row.get(3)?;

            // Parse embedding from JSON
            let embedding: Vec<f32> = serde_json::from_str(&embedding_json)?;
            let metadata: EmbeddingMetadata = serde_json::from_str(&metadata_json)?;

            Ok(Some(EmbeddingData {
                file_path,
                content,
                embedding,
                metadata,
            }))
        } else {
            Ok(None)
        }
    }

    /// Search for similar embeddings using cosine similarity
    pub async fn search_similar(
        &self,
        query_embedding: &[f32],
        top_k: u32,
    ) -> Result<Vec<SearchResultWithScore>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt =
            conn.prepare("SELECT file_path, content, metadata, embedding FROM embeddings")?;
        let mut rows = stmt.query([])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            let file_path: String = row.get(0)?;
            let content: String = row.get(1)?;
            let _metadata_json: String = row.get(2)?;
            let embedding_json: String = row.get(3)?;

            // Parse embedding from JSON
            if let Ok(embedding) = serde_json::from_str::<Vec<f32>>(&embedding_json) {
                // Compute cosine similarity
                let similarity = cosine_similarity(query_embedding, &embedding);

                results.push(SearchResultWithScore {
                    id: file_path.clone(),
                    title: file_path,
                    content,
                    score: similarity,
                });
            }
        }

        // Sort by similarity and take top_k
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k as usize);

        Ok(results)
    }

    /// Search files by tags
    pub async fn search_by_tags(&self, tags: &[String]) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut results = Vec::new();

        for tag in tags {
            let mut stmt = conn.prepare(
                "SELECT file_path FROM embeddings WHERE json_extract(metadata, '$.tags') LIKE ?",
            )?;
            let tag_pattern = format!("%{}%", tag);
            let mut rows = stmt.query([&tag_pattern])?;

            while let Some(row) = rows.next()? {
                let file_path: String = row.get(0)?;
                results.push(file_path);
            }
        }

        Ok(results)
    }

    /// Search files by properties
    pub async fn search_by_properties(
        &self,
        properties: &HashMap<String, serde_json::Value>,
    ) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut results = Vec::new();

        for (key, value) in properties {
            let mut stmt = conn
                .prepare("SELECT file_path FROM embeddings WHERE json_extract(metadata, ?) = ?")?;
            let key_path = format!("$.properties.{}", key);
            let value_str = serde_json::to_string(value)?;
            let mut rows = stmt.query([&key_path, &value_str])?;

            while let Some(row) = rows.next()? {
                let file_path: String = row.get(0)?;
                results.push(file_path);
            }
        }

        Ok(results)
    }

    /// Get all files in the database
    pub async fn list_files(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT file_path FROM embeddings")?;
        let mut rows = stmt.query([])?;
        let mut files = Vec::new();

        while let Some(row) = rows.next()? {
            let file_path: String = row.get(0)?;
            files.push(file_path);
        }

        Ok(files)
    }

    /// Delete a file from the database
    pub async fn delete_file(&self, file_path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM embeddings WHERE file_path = ?", [file_path])?;
        Ok(())
    }

    /// Get database statistics
    pub async fn get_stats(&self) -> Result<HashMap<String, i64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT COUNT(*) as count FROM embeddings")?;
        let mut rows = stmt.query([])?;

        let mut stats = HashMap::new();
        if let Some(row) = rows.next()? {
            let count: i64 = row.get(0)?;
            stats.insert("total_files".to_string(), count);
        }

        Ok(stats)
    }

    /// Close the database connection
    pub async fn close(self) {
        // DuckDB connections are automatically closed when dropped
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
    use std::collections::HashMap;

    fn create_test_metadata(path: &str) -> EmbeddingMetadata {
        let mut properties = HashMap::new();
        properties.insert("status".to_string(), serde_json::json!("active"));
        properties.insert("type".to_string(), serde_json::json!("note"));

        EmbeddingMetadata {
            file_path: path.to_string(),
            title: Some(path.to_string()),
            tags: vec!["test".to_string(), "demo".to_string()],
            folder: "test_folder".to_string(),
            properties,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_database_new_and_initialize() {
        let db = EmbeddingDatabase::new(":memory:").await;
        assert!(db.is_ok());
    }

    #[tokio::test]
    async fn test_store_and_retrieve_embedding() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        let metadata = create_test_metadata("test.md");
        let embedding = vec![0.1, 0.2, 0.3, 0.4];

        // Store embedding
        let result = db.store_embedding("test.md", "test content", &embedding, &metadata).await;
        assert!(result.is_ok());

        // Retrieve embedding
        let retrieved = db.get_embedding("test.md").await.unwrap();
        assert!(retrieved.is_some());
        let data = retrieved.unwrap();
        assert_eq!(data.file_path, "test.md");
        assert_eq!(data.content, "test content");
        assert_eq!(data.embedding, embedding);
    }

    #[tokio::test]
    async fn test_file_exists_true() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        let metadata = create_test_metadata("exists.md");
        let embedding = vec![0.1; 384];

        db.store_embedding("exists.md", "content", &embedding, &metadata).await.unwrap();

        let exists = db.file_exists("exists.md").await.unwrap();
        assert!(exists);
    }

    #[tokio::test]
    async fn test_file_exists_false() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();

        let exists = db.file_exists("nonexistent.md").await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_get_embedding_not_found() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();

        let result = db.get_embedding("nonexistent.md").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update_metadata() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        let mut metadata = create_test_metadata("test.md");
        let embedding = vec![0.1; 384];

        // Store initial embedding
        db.store_embedding("test.md", "content", &embedding, &metadata).await.unwrap();

        // Update metadata
        metadata.properties.insert("status".to_string(), serde_json::json!("archived"));
        db.update_metadata("test.md", &metadata).await.unwrap();

        // Verify update
        let retrieved = db.get_embedding("test.md").await.unwrap().unwrap();
        assert_eq!(
            retrieved.metadata.properties.get("status"),
            Some(&serde_json::json!("archived"))
        );
    }

    #[tokio::test]
    async fn test_search_similar() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        let metadata1 = create_test_metadata("doc1.md");
        let metadata2 = create_test_metadata("doc2.md");
        let metadata3 = create_test_metadata("doc3.md");

        // Store embeddings with different similarities to query
        let embedding1 = vec![1.0, 0.0, 0.0, 0.0];
        let embedding2 = vec![0.9, 0.1, 0.0, 0.0];
        let embedding3 = vec![0.0, 1.0, 0.0, 0.0];

        db.store_embedding("doc1.md", "content1", &embedding1, &metadata1).await.unwrap();
        db.store_embedding("doc2.md", "content2", &embedding2, &metadata2).await.unwrap();
        db.store_embedding("doc3.md", "content3", &embedding3, &metadata3).await.unwrap();

        // Query with embedding similar to embedding1
        let query = vec![1.0, 0.0, 0.0, 0.0];
        let results = db.search_similar(&query, 2).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "doc1.md"); // Most similar
        assert!(results[0].score > results[1].score);
    }

    #[tokio::test]
    async fn test_search_similar_empty_db() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        let query = vec![1.0, 0.0, 0.0, 0.0];

        let results = db.search_similar(&query, 5).await.unwrap();
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_search_by_tags() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        let mut metadata1 = create_test_metadata("doc1.md");
        metadata1.tags = vec!["rust".to_string(), "programming".to_string()];
        let mut metadata2 = create_test_metadata("doc2.md");
        metadata2.tags = vec!["python".to_string(), "programming".to_string()];

        let embedding = vec![0.1; 384];
        db.store_embedding("doc1.md", "content1", &embedding, &metadata1).await.unwrap();
        db.store_embedding("doc2.md", "content2", &embedding, &metadata2).await.unwrap();

        let results = db.search_by_tags(&["rust".to_string()]).await.unwrap();
        assert!(results.contains(&"doc1.md".to_string()));
    }

    #[tokio::test]
    async fn test_search_by_properties() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        let mut metadata1 = create_test_metadata("doc1.md");
        metadata1.properties.insert("status".to_string(), serde_json::json!("active"));
        let mut metadata2 = create_test_metadata("doc2.md");
        metadata2.properties.insert("status".to_string(), serde_json::json!("archived"));

        let embedding = vec![0.1; 384];
        db.store_embedding("doc1.md", "content1", &embedding, &metadata1).await.unwrap();
        db.store_embedding("doc2.md", "content2", &embedding, &metadata2).await.unwrap();

        let mut search_props = HashMap::new();
        search_props.insert("status".to_string(), serde_json::json!("active"));

        let results = db.search_by_properties(&search_props).await.unwrap();
        assert!(results.contains(&"doc1.md".to_string()));
    }

    #[tokio::test]
    async fn test_list_files() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        let metadata = create_test_metadata("test.md");
        let embedding = vec![0.1; 384];

        db.store_embedding("doc1.md", "content1", &embedding, &metadata).await.unwrap();
        db.store_embedding("doc2.md", "content2", &embedding, &metadata).await.unwrap();
        db.store_embedding("doc3.md", "content3", &embedding, &metadata).await.unwrap();

        let files = db.list_files().await.unwrap();
        assert_eq!(files.len(), 3);
        assert!(files.contains(&"doc1.md".to_string()));
        assert!(files.contains(&"doc2.md".to_string()));
        assert!(files.contains(&"doc3.md".to_string()));
    }

    #[tokio::test]
    async fn test_delete_file() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        let metadata = create_test_metadata("test.md");
        let embedding = vec![0.1; 384];

        db.store_embedding("test.md", "content", &embedding, &metadata).await.unwrap();
        assert!(db.file_exists("test.md").await.unwrap());

        db.delete_file("test.md").await.unwrap();
        assert!(!db.file_exists("test.md").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_file_not_found() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();

        // Deleting non-existent file should not error
        let result = db.delete_file("nonexistent.md").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_stats() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        let metadata = create_test_metadata("test.md");
        let embedding = vec![0.1; 384];

        // Initial stats
        let stats = db.get_stats().await.unwrap();
        assert_eq!(stats.get("total_files"), Some(&0));

        // Add files
        db.store_embedding("doc1.md", "content1", &embedding, &metadata).await.unwrap();
        db.store_embedding("doc2.md", "content2", &embedding, &metadata).await.unwrap();

        let stats = db.get_stats().await.unwrap();
        assert_eq!(stats.get("total_files"), Some(&2));
    }

    #[tokio::test]
    async fn test_store_embedding_upsert() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        let metadata = create_test_metadata("test.md");
        let embedding1 = vec![0.1; 384];
        let embedding2 = vec![0.2; 384];

        // Store initial embedding
        db.store_embedding("test.md", "content1", &embedding1, &metadata).await.unwrap();

        // Store again with different content (should update, not insert)
        db.store_embedding("test.md", "content2", &embedding2, &metadata).await.unwrap();

        let stats = db.get_stats().await.unwrap();
        assert_eq!(stats.get("total_files"), Some(&1)); // Should still be 1 file

        let retrieved = db.get_embedding("test.md").await.unwrap().unwrap();
        assert_eq!(retrieved.content, "content2"); // Updated content
        assert_eq!(retrieved.embedding, embedding2); // Updated embedding
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let similarity = cosine_similarity(&a, &b);
        assert!((similarity - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let similarity = cosine_similarity(&a, &b);
        assert!((similarity - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let similarity = cosine_similarity(&a, &b);
        assert!((similarity - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0];
        let similarity = cosine_similarity(&a, &b);
        assert_eq!(similarity, 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![0.0, 0.0, 0.0];
        let similarity = cosine_similarity(&a, &b);
        assert_eq!(similarity, 0.0);
    }

    #[tokio::test]
    async fn test_close() {
        let db = EmbeddingDatabase::new(":memory:").await.unwrap();
        db.close().await;
        // If we get here without panic, close worked
    }
}
