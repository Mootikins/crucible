use anyhow::Result;
use crucible_cli::config::{CliConfig, KilnConfig};
use crucible_core::parser::ParsedDocument;
use crucible_surrealdb::{
    kiln_integration::{self, store_document_embedding, store_parsed_document},
    DocumentEmbedding, SurrealClient, SurrealDbConfig,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

/// Test kiln with temporary directory, database, and CLI configuration
pub struct TestKiln {
    pub _temp_dir: TempDir,
    pub kiln_path: PathBuf,
    pub config: CliConfig,
    pub client: Option<SurrealClient>,
}

impl TestKiln {
    /// Create a new test kiln with temporary directory
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let kiln_path = temp_dir.path().join("kiln");

        std::fs::create_dir_all(&kiln_path)?;

        // Create test configuration
        let config = CliConfig {
            kiln: KilnConfig {
                path: kiln_path.clone(),
                embedding_url: "http://localhost:11434".to_string(),
                embedding_model: Some("nomic-embed-text".to_string()),
            },
            ..Default::default()
        };

        Ok(Self {
            _temp_dir: temp_dir,
            kiln_path,
            config,
            client: None,
        })
    }

    /// Initialize database for this test kiln
    pub async fn init_database(&mut self) -> Result<&SurrealClient> {
        // Initialize database
        let db_path = self.config.database_path();
        std::fs::create_dir_all(db_path.parent().unwrap())?;

        // Create SurrealDbConfig
        let db_config = SurrealDbConfig {
            namespace: "test".to_string(),
            database: "test".to_string(),
            path: self.config.database_path_str()?,
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };
        let client = SurrealClient::new(db_config).await?;

        // Initialize kiln schema
        kiln_integration::initialize_kiln_schema(&client).await?;

        self.client = Some(client);
        Ok(self.client.as_ref().unwrap())
    }

    /// Create a test kiln with database initialized and sample documents
    pub async fn with_sample_documents() -> Result<Self> {
        let mut test_kiln = Self::new()?;
        test_kiln.init_database().await?;

        // Create test documents with controlled content for predictable similarity
        let test_docs = vec![
            (
                "machine-learning-basics.md",
                "Introduction to machine learning algorithms and neural networks",
                vec![0.8, 0.6, 0.1, 0.2],
            ),
            (
                "rust-programming.md",
                "Systems programming with Rust language memory safety",
                vec![0.3, 0.2, 0.8, 0.4],
            ),
            (
                "database-systems.md",
                "SQL and NoSQL database management vector embeddings",
                vec![0.2, 0.9, 0.3, 0.1],
            ),
            (
                "web-development.md",
                "HTML CSS JavaScript frontend backend development",
                vec![0.1, 0.3, 0.2, 0.9],
            ),
            (
                "ai-research.md",
                "Artificial intelligence deep learning transformer models",
                vec![0.7, 0.7, 0.2, 0.3],
            ),
        ];

        // Store documents and their embeddings
        for (filename, content, embedding_vector) in test_docs {
            test_kiln.add_document_with_embedding(filename, content, &embedding_vector).await?;
        }

        Ok(test_kiln)
    }

    /// Add a document with embedding to the kiln (requires database initialized)
    pub async fn add_document_with_embedding(
        &self,
        filename: &str,
        content: &str,
        embedding_pattern: &[f32],
    ) -> Result<String> {
        let client = self.client.as_ref().expect("Database not initialized");

        // Create ParsedDocument
        let mut doc = ParsedDocument::new(self.kiln_path.join(filename));
        doc.content.plain_text = content.to_string();
        doc.parsed_at = chrono::Utc::now();
        doc.content_hash = format!("hash_{}", filename);
        doc.file_size = content.len() as u64;

        // Store document
        let doc_id = store_parsed_document(client, &doc, &self.kiln_path).await?;

        // Create and store embedding
        let mut embedding = DocumentEmbedding::new(
            doc_id.clone(),
            create_full_embedding_vector(embedding_pattern),
            "nomic-embed-text".to_string(),
        );
        embedding.chunk_size = content.len();
        embedding.created_at = chrono::Utc::now();

        store_document_embedding(client, &embedding).await?;

        Ok(doc_id)
    }
    
    /// Create a single note in the kiln
    pub fn create_note(&self, relative_path: &str, content: &str) -> Result<PathBuf> {
        let full_path = self.kiln_path.join(relative_path);
        
        // Create parent directories
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        std::fs::write(&full_path, content)?;
        Ok(full_path)
    }
    
    /// Create multiple notes at once
    pub fn create_notes_batch(&self, notes: Vec<(&str, &str)>) -> Result<()> {
        for (path, content) in notes {
            self.create_note(path, content)?;
        }
        Ok(())
    }
    
    /// Get kiln path as string
    pub fn kiln_path_str(&self) -> &str {
        self.kiln_path.to_str().unwrap()
    }
    
    /// Get database path as string
    pub fn db_path_str(&self) -> &str {
        self.db_path.to_str().unwrap()
    }
}


/// Helper function to assert output contains expected string
pub fn assert_output_contains(output: &str, expected: &str) {
    assert!(
        output.contains(expected),
        "Expected output to contain '{}', but got:\n{}",
        expected,
        output
    );
}

/// Helper function to validate JSON output
pub fn assert_json_valid(output: &str) -> serde_json::Value {
    serde_json::from_str(output).expect("Output should be valid JSON")
}

/// Helper to create a full 768-dimensional embedding vector from a pattern
///
/// This function takes a small pattern (e.g., [0.8, 0.6, 0.1, 0.2]) and
/// expands it to a full 768-dimensional vector by repeating the pattern
/// and adding controlled variation.
fn create_full_embedding_vector(pattern: &[f32]) -> Vec<f32> {
    let dimensions = 768;
    let mut vector = Vec::with_capacity(dimensions);

    for i in 0..dimensions {
        let pattern_idx = i % pattern.len();
        let base_value = pattern[pattern_idx];
        // Add controlled variation while maintaining pattern
        let variation = (i as f32 * 0.01).sin() * 0.1;
        vector.push((base_value + variation).clamp(-1.0, 1.0));
    }

    vector
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kiln_creation() {
        let kiln = TestKiln::new().unwrap();
        assert!(kiln.kiln_path.exists());
    }

    #[test]
    fn test_create_note() {
        let kiln = TestKiln::new().unwrap();
        let note_path = kiln.create_note("test.md", "# Test").unwrap();

        assert!(note_path.exists());
        let content = std::fs::read_to_string(&note_path).unwrap();
        assert_eq!(content, "# Test");
    }

    #[test]
    fn test_create_notes_batch() {
        let kiln = TestKiln::new().unwrap();
        kiln.create_notes_batch(vec![
            ("note1.md", "Content 1"),
            ("folder/note2.md", "Content 2"),
        ]).unwrap();

        assert!(kiln.kiln_path.join("note1.md").exists());
        assert!(kiln.kiln_path.join("folder/note2.md").exists());
    }

    #[tokio::test]
    async fn test_kiln_with_database() {
        let mut kiln = TestKiln::new().unwrap();
        let client = kiln.init_database().await.unwrap();

        // Verify database is initialized
        let result = client.query("INFO FOR DB", &[]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_kiln_with_sample_documents() {
        let kiln = TestKiln::with_sample_documents().await.unwrap();

        // Verify client is initialized
        assert!(kiln.client.is_some());

        // Verify documents were stored
        let client = kiln.client.as_ref().unwrap();
        let result = client
            .query("SELECT count() FROM notes GROUP ALL", &[])
            .await
            .unwrap();

        // Should have 5 sample documents
        assert!(!result.records.is_empty());
    }
}
