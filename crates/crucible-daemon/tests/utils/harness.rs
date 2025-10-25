//! Test harness for embedding integration tests
//!
//! Provides a high-level test harness for integration testing of embedding
//! generation, storage, and semantic search with a realistic vault environment.
//!
//! ## DaemonEmbeddingHarness
//!
//! The main test harness that provides:
//! - Temporary vault directory management
//! - In-memory SurrealDB database
//! - Configurable embedding providers (mock/Ollama/auto)
//! - Note creation with automatic or manual embeddings
//! - Semantic search and metadata queries
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use crucible_daemon::tests::utils::DaemonEmbeddingHarness;
//!
//! #[tokio::test]
//! async fn test_semantic_search() -> Result<()> {
//!     let harness = DaemonEmbeddingHarness::new_default().await?;
//!
//!     // Create notes with automatic embeddings
//!     harness.create_note("note1.md", "Rust programming").await?;
//!     harness.create_note("note2.md", "Python scripting").await?;
//!
//!     // Search for similar notes
//!     let results = harness.semantic_search("coding in Rust", 5).await?;
//!     assert!(!results.is_empty());
//!
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use crucible_core::parser::{MarkdownParser, PulldownParser, SurrealDBAdapter};
use crucible_llm::embeddings::EmbeddingProvider;
use crucible_surrealdb::{DatabaseStats, EmbeddingMetadata, SurrealEmbeddingDatabase};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;

use super::embedding_helpers::EmbeddingStrategy;

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the DaemonEmbeddingHarness
///
/// Controls embedding provider selection, validation, and storage behavior.
#[derive(Debug, Clone)]
pub struct EmbeddingHarnessConfig {
    /// Strategy for selecting embedding provider
    pub strategy: EmbeddingStrategy,

    /// Expected embedding dimensions (used for validation)
    pub dimensions: usize,

    /// Whether to validate embedding dimensions match expected
    pub validate_dimensions: bool,

    /// Whether to store full content in database (vs just metadata)
    pub store_full_content: bool,
}

impl EmbeddingHarnessConfig {
    /// Create config for mock provider testing (fast, deterministic)
    ///
    /// Use this for unit tests and fast integration tests that don't need
    /// real embeddings.
    pub fn mock() -> Self {
        Self {
            strategy: EmbeddingStrategy::Mock,
            dimensions: 768,
            validate_dimensions: true,
            store_full_content: true,
        }
    }

    /// Create config for Ollama provider testing (requires running server)
    ///
    /// Use this for integration tests that need real embeddings from Ollama.
    /// Requires Ollama server running on localhost:11434.
    pub fn ollama() -> Self {
        Self {
            strategy: EmbeddingStrategy::Ollama,
            dimensions: 768,
            validate_dimensions: true,
            store_full_content: true,
        }
    }

    /// Create config with auto-detection (Ollama if available, fallback to mock)
    ///
    /// Use this for integration tests that prefer real embeddings but can
    /// fallback to mock if Ollama is unavailable.
    pub fn auto() -> Self {
        Self {
            strategy: EmbeddingStrategy::Auto,
            dimensions: 768,
            validate_dimensions: false, // Don't validate since dimensions may vary
            store_full_content: true,
        }
    }
}

impl Default for EmbeddingHarnessConfig {
    fn default() -> Self {
        Self::mock()
    }
}

// ============================================================================
// Test Harness
// ============================================================================

/// Integration test harness for embedding workflows
///
/// Provides a complete test environment with:
/// - Temporary vault directory (auto-cleaned on drop)
/// - In-memory SurrealDB database
/// - Configurable embedding provider (mock/Ollama/auto)
/// - Markdown parser and adapter
/// - Note creation with automatic/manual embeddings
/// - Semantic search and metadata queries
///
/// Compatible with VaultTestHarness API for easy migration.
pub struct DaemonEmbeddingHarness {
    vault_dir: TempDir,
    db: SurrealEmbeddingDatabase,
    parser: PulldownParser,
    adapter: SurrealDBAdapter,
    provider: Arc<dyn EmbeddingProvider>,
    config: EmbeddingHarnessConfig,
}

impl Clone for DaemonEmbeddingHarness {
    fn clone(&self) -> Self {
        // Note: This creates a new temporary directory, which is the best we can do
        // since TempDir doesn't implement Clone. The database and provider are shared.
        let new_vault_dir = tempfile::TempDir::new().expect("Failed to create temporary directory");

        Self {
            vault_dir: new_vault_dir,
            db: self.db.clone(),
            parser: self.parser.clone(),
            adapter: self.adapter.clone(),
            provider: self.provider.clone(),
            config: self.config.clone(),
        }
    }
}

impl DaemonEmbeddingHarness {
    /// Create a new test harness with custom configuration
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = EmbeddingHarnessConfig::ollama();
    /// let harness = DaemonEmbeddingHarness::new(config).await?;
    /// ```
    pub async fn new(config: EmbeddingHarnessConfig) -> Result<Self> {
        // Create temporary vault directory
        let vault_dir = TempDir::new()?;

        // Create in-memory database
        let db = SurrealEmbeddingDatabase::new_memory();
        db.initialize().await?;

        // Create embedding provider
        let provider = config.strategy.create_provider(config.dimensions).await?;

        // Create parser and adapter
        let parser = PulldownParser::new();
        let adapter = if config.store_full_content {
            SurrealDBAdapter::new().with_full_content()
        } else {
            SurrealDBAdapter::new()
        };

        Ok(Self {
            vault_dir,
            db,
            parser,
            adapter,
            provider,
            config,
        })
    }

    /// Create a new test harness with default configuration (mock provider)
    ///
    /// Equivalent to `DaemonEmbeddingHarness::new(EmbeddingHarnessConfig::default())`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let harness = DaemonEmbeddingHarness::new_default().await?;
    /// ```
    pub async fn new_default() -> Result<Self> {
        Self::new(EmbeddingHarnessConfig::default()).await
    }

    // ========================================================================
    // Note Creation
    // ========================================================================

    /// Create a note in the vault with automatic embedding generation
    ///
    /// This is the primary method for creating test notes. It:
    /// 1. Writes the file to the vault directory
    /// 2. Parses the markdown content
    /// 3. Generates an embedding using the configured provider
    /// 4. Stores the note with embedding in the database
    ///
    /// # Arguments
    ///
    /// * `path` - Relative path from vault root (e.g., "Projects/note.md")
    /// * `content` - Markdown content (with or without frontmatter)
    ///
    /// # Returns
    ///
    /// Absolute path to the created file
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let path = harness.create_note(
    ///     "rust-guide.md",
    ///     r#"---
    /// title: Rust Programming Guide
    /// tags: [rust, programming]
    /// ---
    ///
    /// # Rust Guide
    ///
    /// Learn Rust programming language.
    /// "#,
    /// ).await?;
    /// ```
    pub async fn create_note(&self, path: &str, content: &str) -> Result<PathBuf> {
        let note_path = self.vault_dir.path().join(path);

        // Create parent directories
        if let Some(parent) = note_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write file
        fs::write(&note_path, content).await?;

        // Parse markdown
        let doc = self.parser.parse_file(&note_path).await?;

        // Validate with adapter
        let _record = self.adapter.to_note_record(&doc)?;

        // Generate embedding
        let response = self.provider.embed(&doc.content.plain_text).await?;

        // Validate dimensions if configured
        if self.config.validate_dimensions {
            if response.dimensions != self.config.dimensions {
                anyhow::bail!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    self.config.dimensions,
                    response.dimensions
                );
            }
        }

        // Prepare metadata
        let path_str = note_path.to_string_lossy().to_string();
        let folder = note_path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();

        let properties = doc
            .frontmatter
            .as_ref()
            .map(|fm| fm.properties().clone())
            .unwrap_or_default();

        let metadata = EmbeddingMetadata {
            file_path: path_str.clone(),
            title: Some(doc.title()),
            tags: doc.all_tags(),
            folder,
            properties,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Store in database
        self.db
            .store_embedding(
                &path_str,
                &doc.content.plain_text,
                &response.embedding,
                &metadata,
            )
            .await?;

        Ok(note_path)
    }

    /// Create a note without generating an embedding
    ///
    /// Use this when you want to test edge cases like missing embeddings
    /// or when you'll add the embedding later.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let path = harness.create_note_no_embedding(
    ///     "draft.md",
    ///     "# Draft Note\n\nNot ready for embedding yet.",
    /// ).await?;
    /// ```
    pub async fn create_note_no_embedding(&self, path: &str, content: &str) -> Result<PathBuf> {
        let note_path = self.vault_dir.path().join(path);

        // Create parent directories
        if let Some(parent) = note_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write file
        fs::write(&note_path, content).await?;

        // Parse markdown
        let doc = self.parser.parse_file(&note_path).await?;

        // Validate with adapter
        let _record = self.adapter.to_note_record(&doc)?;

        // Prepare metadata
        let path_str = note_path.to_string_lossy().to_string();
        let folder = note_path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();

        let properties = doc
            .frontmatter
            .as_ref()
            .map(|fm| fm.properties().clone())
            .unwrap_or_default();

        let metadata = EmbeddingMetadata {
            file_path: path_str.clone(),
            title: Some(doc.title()),
            tags: doc.all_tags(),
            folder,
            properties,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Store with dummy zero-vector embedding
        let dummy_embedding = vec![0.0; self.config.dimensions];
        self.db
            .store_embedding(
                &path_str,
                &doc.content.plain_text,
                &dummy_embedding,
                &metadata,
            )
            .await?;

        Ok(note_path)
    }

    /// Create a note with a pre-computed embedding
    ///
    /// Use this when you want to test with specific embeddings (e.g., from
    /// the semantic corpus or for deterministic tests).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let corpus = load_semantic_corpus()?;
    /// let doc = get_corpus_document(&corpus, "rust_fn_add").unwrap();
    /// let embedding = doc.embedding.clone().unwrap();
    ///
    /// let path = harness.create_note_with_embedding(
    ///     "rust_fn.md",
    ///     &doc.content,
    ///     embedding,
    /// ).await?;
    /// ```
    pub async fn create_note_with_embedding(
        &self,
        path: &str,
        content: &str,
        embedding: Vec<f32>,
    ) -> Result<PathBuf> {
        let note_path = self.vault_dir.path().join(path);

        // Create parent directories
        if let Some(parent) = note_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write file
        fs::write(&note_path, content).await?;

        // Parse markdown
        let doc = self.parser.parse_file(&note_path).await?;

        // Validate with adapter
        let _record = self.adapter.to_note_record(&doc)?;

        // Validate dimensions if configured
        if self.config.validate_dimensions {
            if embedding.len() != self.config.dimensions {
                anyhow::bail!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    self.config.dimensions,
                    embedding.len()
                );
            }
        }

        // Prepare metadata
        let path_str = note_path.to_string_lossy().to_string();
        let folder = note_path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();

        let properties = doc
            .frontmatter
            .as_ref()
            .map(|fm| fm.properties().clone())
            .unwrap_or_default();

        let metadata = EmbeddingMetadata {
            file_path: path_str.clone(),
            title: Some(doc.title()),
            tags: doc.all_tags(),
            folder,
            properties,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Store in database
        self.db
            .store_embedding(&path_str, &doc.content.plain_text, &embedding, &metadata)
            .await?;

        Ok(note_path)
    }

    // ========================================================================
    // Semantic Search
    // ========================================================================

    /// Perform semantic search on vault notes
    ///
    /// Generates an embedding for the query and finds the most similar notes
    /// using cosine similarity.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query text
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// Vector of (file_path, similarity_score) pairs, sorted by descending similarity
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let results = harness.semantic_search("Rust async programming", 5).await?;
    /// for (path, score) in results {
    ///     println!("{}: {:.3}", path, score);
    /// }
    /// ```
    pub async fn semantic_search(&self, query: &str, limit: usize) -> Result<Vec<(String, f32)>> {
        // Generate query embedding
        let response = self.provider.embed(query).await?;

        // Search for similar embeddings
        let results = self
            .db
            .search_similar(query, &response.embedding, limit as u32)
            .await?;

        // Convert to (path, score) pairs
        Ok(results
            .into_iter()
            .map(|r| (r.id, r.score as f32))
            .collect())
    }

    /// Check if a note has an embedding (non-zero vector)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// assert!(harness.has_embedding("note.md").await?);
    /// ```
    pub async fn has_embedding(&self, path: &str) -> Result<bool> {
        let full_path = self.vault_dir.path().join(path).to_string_lossy().to_string();
        let data = self.db.get_embedding(&full_path).await?;

        Ok(data.map(|d| !d.embedding.iter().all(|&v| v == 0.0)).unwrap_or(false))
    }

    /// Get the embedding vector for a note
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let embedding = harness.get_embedding("note.md").await?.unwrap();
    /// assert_eq!(embedding.len(), 768);
    /// ```
    pub async fn get_embedding(&self, path: &str) -> Result<Option<Vec<f32>>> {
        let full_path = self.vault_dir.path().join(path).to_string_lossy().to_string();
        let data = self.db.get_embedding(&full_path).await?;

        Ok(data.map(|d| d.embedding))
    }

    // ========================================================================
    // Direct Embedding Generation
    // ========================================================================

    /// Generate an embedding for the given text
    ///
    /// This is a convenience method that directly generates an embedding
    /// without storing it in the database. Tests expect this method to exist.
    ///
    /// # Arguments
    ///
    /// * `text` - Text to generate embedding for
    ///
    /// # Returns
    ///
    /// Embedding vector as `Vec<f32>`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let embedding = harness.generate_embedding("Hello world").await?;
    /// assert_eq!(embedding.len(), 768);
    /// ```
    pub async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let response = self.provider.embed(text).await?;
        Ok(response.embedding)
    }

    /// Generate embeddings for multiple texts (batch processing)
    ///
    /// This is a convenience method that generates embeddings for multiple
    /// texts at once, useful for performance testing.
    ///
    /// # Arguments
    ///
    /// * `texts` - Vector of texts to generate embeddings for
    ///
    /// # Returns
    ///
    /// Vector of embedding vectors as `Vec<Vec<f32>>`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let texts = vec!["Hello".to_string(), "World".to_string()];
    /// let embeddings = harness.generate_batch_embeddings(&texts).await?;
    /// assert_eq!(embeddings.len(), 2);
    /// ```
    pub async fn generate_batch_embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut embeddings = Vec::with_capacity(texts.len());

        for text in texts {
            let response = self.provider.embed(text).await?;
            embeddings.push(response.embedding);
        }

        Ok(embeddings)
    }

    /// Create a new harness with custom embedding configuration
    ///
    /// This is a convenience constructor that takes an EmbeddingConfig
    /// instead of EmbeddingHarnessConfig, making it compatible with tests
    /// that have embedding configurations.
    ///
    /// # Arguments
    ///
    /// * `config` - Embedding configuration
    ///
    /// # Returns
    ///
    /// New DaemonEmbeddingHarness instance
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use crucible_llm::embeddings::EmbeddingConfig;
    /// let config = EmbeddingConfig::default();
    /// let harness = DaemonEmbeddingHarness::new_with_config(config).await?;
    /// ```
    pub async fn new_with_config(config: crucible_llm::embeddings::EmbeddingConfig) -> Result<Self> {
        // Convert EmbeddingConfig to EmbeddingHarnessConfig
        let harness_config = EmbeddingHarnessConfig {
            strategy: EmbeddingStrategy::Mock, // Default to mock for testing
            dimensions: config.dimensions.unwrap_or(768),
            validate_dimensions: false, // Don't validate by default
            store_full_content: true,
        };

        Self::new(harness_config).await
    }

    // ========================================================================
    // VaultTestHarness Compatibility
    // ========================================================================

    /// Check if a file exists in the database
    ///
    /// Compatible with VaultTestHarness::file_exists()
    pub async fn file_exists(&self, path: &str) -> Result<bool> {
        let full_path = self.vault_dir.path().join(path).to_string_lossy().to_string();
        self.db.file_exists(&full_path).await
    }

    /// Get metadata for a note
    ///
    /// Compatible with VaultTestHarness::get_metadata()
    pub async fn get_metadata(&self, path: &str) -> Result<Option<EmbeddingMetadata>> {
        let full_path = self.vault_dir.path().join(path).to_string_lossy().to_string();
        let data = self.db.get_embedding(&full_path).await?;
        Ok(data.map(|d| d.metadata))
    }

    /// Get database statistics
    ///
    /// Compatible with VaultTestHarness::get_stats()
    pub async fn get_stats(&self) -> Result<DatabaseStats> {
        self.db.get_stats().await
    }

    /// Get the vault directory path
    ///
    /// Returns the temporary directory where notes are stored.
    pub fn vault_path(&self) -> &Path {
        self.vault_dir.path()
    }

    /// Get reference to the embedding provider
    ///
    /// Useful for direct embedding operations in tests.
    pub fn provider(&self) -> &Arc<dyn EmbeddingProvider> {
        &self.provider
    }

    /// Get reference to the database
    ///
    /// Useful for advanced database operations in tests.
    pub fn db(&self) -> &SurrealEmbeddingDatabase {
        &self.db
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_harness_creation_mock() -> Result<()> {
        let config = EmbeddingHarnessConfig::mock();
        let harness = DaemonEmbeddingHarness::new(config).await?;

        // Verify harness components initialized
        assert!(harness.vault_path().exists());

        let stats = harness.get_stats().await?;
        assert_eq!(stats.total_documents, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_harness_creation_default() -> Result<()> {
        let harness = DaemonEmbeddingHarness::new_default().await?;

        // Verify harness components initialized
        assert!(harness.vault_path().exists());

        let stats = harness.get_stats().await?;
        assert_eq!(stats.total_documents, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_create_note_with_embedding() -> Result<()> {
        let harness = DaemonEmbeddingHarness::new_default().await?;

        let path = harness
            .create_note(
                "test.md",
                r#"---
title: Test Note
tags: [test, rust]
---

# Test Note

This is a test note for embedding generation.
"#,
            )
            .await?;

        // Verify file created
        assert!(path.exists());

        // Verify indexed in database
        assert!(harness.file_exists("test.md").await?);

        // Verify metadata
        let metadata = harness
            .get_metadata("test.md")
            .await?
            .expect("Metadata should exist");
        assert_eq!(metadata.title, Some("Test Note".to_string()));
        assert!(metadata.tags.contains(&"test".to_string()));
        assert!(metadata.tags.contains(&"rust".to_string()));

        // Verify embedding exists
        assert!(harness.has_embedding("test.md").await?);

        let embedding = harness
            .get_embedding("test.md")
            .await?
            .expect("Embedding should exist");
        assert_eq!(embedding.len(), 768); // Mock provider uses 768 dimensions

        Ok(())
    }

    #[tokio::test]
    async fn test_create_note_no_embedding() -> Result<()> {
        let harness = DaemonEmbeddingHarness::new_default().await?;

        let path = harness
            .create_note_no_embedding("draft.md", "# Draft\n\nNo embedding yet.")
            .await?;

        // Verify file created
        assert!(path.exists());

        // Verify indexed in database
        assert!(harness.file_exists("draft.md").await?);

        // Verify has zero-vector embedding
        assert!(!harness.has_embedding("draft.md").await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_create_note_with_precomputed_embedding() -> Result<()> {
        let harness = DaemonEmbeddingHarness::new_default().await?;

        // Create custom embedding
        let custom_embedding = vec![0.5; 768];

        let path = harness
            .create_note_with_embedding(
                "custom.md",
                "# Custom\n\nNote with custom embedding.",
                custom_embedding.clone(),
            )
            .await?;

        // Verify file created
        assert!(path.exists());

        // Verify embedding matches
        let stored_embedding = harness
            .get_embedding("custom.md")
            .await?
            .expect("Embedding should exist");
        assert_eq!(stored_embedding, custom_embedding);

        Ok(())
    }

    #[tokio::test]
    async fn test_semantic_search() -> Result<()> {
        let harness = DaemonEmbeddingHarness::new_default().await?;

        // Create test notes
        harness
            .create_note("rust.md", "# Rust Programming\n\nLearn Rust language.")
            .await?;
        harness
            .create_note("python.md", "# Python Scripting\n\nLearn Python language.")
            .await?;
        harness
            .create_note("cooking.md", "# Cooking Recipe\n\nHow to cook pasta.")
            .await?;

        // Search for programming-related notes
        let results = harness.semantic_search("programming languages", 5).await?;

        // Verify we got results
        assert!(!results.is_empty(), "Should find similar notes");

        // With mock provider, results are deterministic but may vary
        // Just verify structure is correct
        for (path, score) in results {
            assert!(!path.is_empty());
            assert!(score >= 0.0 && score <= 1.0, "Score should be normalized");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_nested_folder_structure() -> Result<()> {
        let harness = DaemonEmbeddingHarness::new_default().await?;

        // Create notes in nested folders
        harness
            .create_note(
                "Projects/Rust/guide.md",
                "# Rust Guide\n\nRust programming guide.",
            )
            .await?;
        harness
            .create_note(
                "Daily/2025-01/2025-01-15.md",
                "# Daily Note\n\nToday's work log.",
            )
            .await?;

        // Verify both files exist
        assert!(harness.file_exists("Projects/Rust/guide.md").await?);
        assert!(harness.file_exists("Daily/2025-01/2025-01-15.md").await?);

        // Verify folder metadata
        let guide_meta = harness
            .get_metadata("Projects/Rust/guide.md")
            .await?
            .expect("Guide should exist");
        assert!(guide_meta.folder.contains("Projects/Rust"));

        let daily_meta = harness
            .get_metadata("Daily/2025-01/2025-01-15.md")
            .await?
            .expect("Daily note should exist");
        assert!(daily_meta.folder.contains("Daily/2025-01"));

        Ok(())
    }

    #[tokio::test]
    async fn test_vault_test_harness_compatibility() -> Result<()> {
        let harness = DaemonEmbeddingHarness::new_default().await?;

        // Test VaultTestHarness-compatible methods
        harness.create_note("test.md", "# Test").await?;

        // file_exists
        assert!(harness.file_exists("test.md").await?);
        assert!(!harness.file_exists("nonexistent.md").await?);

        // get_metadata
        let metadata = harness.get_metadata("test.md").await?;
        assert!(metadata.is_some());

        // get_stats
        let stats = harness.get_stats().await?;
        assert_eq!(stats.total_documents, 1);

        // vault_path
        let vault_path = harness.vault_path();
        assert!(vault_path.exists());

        // provider
        let provider = harness.provider();
        let response = provider.embed("test").await?;
        assert!(response.dimensions > 0);

        // db
        let db = harness.db();
        let full_path = harness.vault_path().join("test.md").to_string_lossy().to_string();
        let exists = db.file_exists(&full_path).await?;
        assert!(exists);

        Ok(())
    }

    #[tokio::test]
    async fn test_embedding_dimension_validation() -> Result<()> {
        let config = EmbeddingHarnessConfig {
            strategy: EmbeddingStrategy::Mock,
            dimensions: 768,
            validate_dimensions: true,
            store_full_content: true,
        };
        let harness = DaemonEmbeddingHarness::new(config).await?;

        // Should succeed with correct dimensions
        let result = harness
            .create_note("valid.md", "# Valid\n\nCorrect dimensions.")
            .await;
        assert!(result.is_ok());

        // Test with pre-computed embedding of wrong dimensions
        let wrong_dims = vec![0.5; 512]; // Wrong dimension
        let result = harness
            .create_note_with_embedding("wrong.md", "# Wrong", wrong_dims)
            .await;
        assert!(result.is_err(), "Should fail with wrong dimensions");

        Ok(())
    }
}
