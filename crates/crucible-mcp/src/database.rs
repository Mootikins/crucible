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
