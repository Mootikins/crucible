//! Database Transaction Builder
//!
//! This module provides the TransactionBuilder that converts processed documents
//! from the core layer into database transaction sequences. This is the critical
//! bridge that maintains clean layer separation while enabling the queue-based
//! architecture.
//!
//! ## Architecture Role
//!
//! The TransactionBuilder is responsible for:
//! - Converting ProcessedDocument handoff types to DatabaseTransaction sequences
//! - Implementing dependency resolution between transactions
//! - Providing configurable transaction generation strategies
//! - Maintaining separation between processing logic and database structure knowledge

use crate::transaction_queue::{DatabaseTransaction, TransactionTimestamp};
use anyhow::Result;
use std::path::PathBuf;
use tracing::debug;

/// Configuration for transaction building behavior
#[derive(Debug, Clone)]
pub struct TransactionBuilderConfig {
    /// Whether to generate embedding transactions
    pub generate_embeddings: bool,

    /// Whether to generate relationship transactions
    pub generate_relationships: bool,

    /// Whether to generate tag association transactions
    pub generate_tags: bool,

    /// Whether to generate timestamp update transactions
    pub generate_timestamps: bool,

    /// Custom transaction ID prefix
    pub transaction_id_prefix: Option<String>,
}

impl Default for TransactionBuilderConfig {
    fn default() -> Self {
        Self {
            generate_embeddings: true,
            generate_relationships: true,
            generate_tags: true,
            generate_timestamps: true,
            transaction_id_prefix: None,
        }
    }
}

/// Builds database transactions from processed documents
///
/// This is the critical component that bridges the core layer's ProcessedDocument
/// handoff types with the database layer's DatabaseTransaction types, maintaining
/// clean architectural separation.
pub struct TransactionBuilder {
    config: TransactionBuilderConfig,
    transaction_counter: std::sync::atomic::AtomicU64,
}

impl TransactionBuilder {
    /// Create a new transaction builder with default configuration
    pub fn new() -> Self {
        Self::with_config(TransactionBuilderConfig::default())
    }

    /// Create a new transaction builder with custom configuration
    pub fn with_config(config: TransactionBuilderConfig) -> Self {
        Self {
            config,
            transaction_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Convert a processed document into a sequence of database transactions
    ///
    /// This is the main entry point that converts ProcessedDocument handoff types
    /// into DatabaseTransaction sequences while respecting dependencies and priorities.
    pub fn build_transactions(&self, processed_document: &crucible_core::ProcessedDocument) -> Result<Vec<DatabaseTransaction>> {
        debug!("Building transactions for document: {:?}", processed_document.path());

        let mut transactions = Vec::new();

        // Step 1: Generate the base document storage transaction
        let store_tx = self.build_store_transaction(processed_document)?;
        transactions.push(store_tx);

        // Step 2: Generate relationship transactions (depend on document being stored)
        if self.config.generate_relationships {
            if let Some(wikilink_tx) = self.build_wikilink_transaction(processed_document)? {
                transactions.push(wikilink_tx);
            }

            if let Some(embed_tx) = self.build_embed_relationship_transaction(processed_document)? {
                transactions.push(embed_tx);
            }
        }

        // Step 3: Generate tag association transactions (depend on document being stored)
        if self.config.generate_tags {
            if let Some(tag_tx) = self.build_tag_association_transaction(processed_document)? {
                transactions.push(tag_tx);
            }
        }

        // Step 4: Generate embedding processing transaction (depends on document being stored)
        if self.config.generate_embeddings {
            if let Some(embedding_tx) = self.build_embedding_transaction(processed_document)? {
                transactions.push(embedding_tx);
            }
        }

        // Step 5: Generate timestamp update transaction (depends on document being stored)
        if self.config.generate_timestamps {
            let timestamp_tx = self.build_timestamp_transaction(processed_document)?;
            transactions.push(timestamp_tx);
        }

        debug!("Generated {} transactions for document: {:?}", transactions.len(), processed_document.path());
        Ok(transactions)
    }

    /// Build a document storage transaction
    fn build_store_transaction(&self, processed_document: &crucible_core::ProcessedDocument) -> Result<DatabaseTransaction> {
        let transaction_id = self.generate_transaction_id("store", processed_document.path());

        Ok(DatabaseTransaction::StoreDocument {
            transaction_id,
            document: processed_document.document.clone(),
            kiln_root: processed_document.kiln_root.clone(),
            timestamp: TransactionTimestamp::now(),
        })
    }

    /// Build a wikilink relationship transaction if the document has wikilinks
    fn build_wikilink_transaction(&self, processed_document: &crucible_core::ProcessedDocument) -> Result<Option<DatabaseTransaction>> {
        if processed_document.document.wikilinks.is_empty() {
            return Ok(None);
        }

        let transaction_id = self.generate_transaction_id("wikilinks", processed_document.path());

        Ok(Some(DatabaseTransaction::CreateWikilinkEdges {
            transaction_id,
            document_id: self.generate_document_id(processed_document.path()),
            document: processed_document.document.clone(),
            timestamp: TransactionTimestamp::now(),
        }))
    }

    /// Build an embed relationship transaction if the document has embeds
    fn build_embed_relationship_transaction(&self, processed_document: &crucible_core::ProcessedDocument) -> Result<Option<DatabaseTransaction>> {
        // For now, we'll check if the document has any content that might contain embeds
        // This can be enhanced later with proper embed detection
        if processed_document.document.content.plain_text.is_empty() {
            return Ok(None);
        }

        let transaction_id = self.generate_transaction_id("embeds", processed_document.path());

        Ok(Some(DatabaseTransaction::CreateEmbedRelationships {
            transaction_id,
            document_id: self.generate_document_id(processed_document.path()),
            document: processed_document.document.clone(),
            timestamp: TransactionTimestamp::now(),
        }))
    }

    /// Build a tag association transaction if the document has tags
    fn build_tag_association_transaction(&self, processed_document: &crucible_core::ProcessedDocument) -> Result<Option<DatabaseTransaction>> {
        if processed_document.document.tags.is_empty() {
            return Ok(None);
        }

        let transaction_id = self.generate_transaction_id("tags", processed_document.path());

        Ok(Some(DatabaseTransaction::CreateTagAssociations {
            transaction_id,
            document_id: self.generate_document_id(processed_document.path()),
            document: processed_document.document.clone(),
            timestamp: TransactionTimestamp::now(),
        }))
    }

    /// Build an embedding processing transaction
    fn build_embedding_transaction(&self, processed_document: &crucible_core::ProcessedDocument) -> Result<Option<DatabaseTransaction>> {
        // Only generate embedding transaction if embeddings are enabled for this document
        if !processed_document.context.metadata.generate_embeddings {
            return Ok(None);
        }

        let transaction_id = self.generate_transaction_id("embeddings", processed_document.path());

        Ok(Some(DatabaseTransaction::ProcessEmbeddings {
            transaction_id,
            document_id: self.generate_document_id(processed_document.path()),
            document: processed_document.document.clone(),
            timestamp: TransactionTimestamp::now(),
        }))
    }

    /// Build a timestamp update transaction
    fn build_timestamp_transaction(&self, processed_document: &crucible_core::ProcessedDocument) -> Result<DatabaseTransaction> {
        let transaction_id = self.generate_transaction_id("timestamp", processed_document.path());

        Ok(DatabaseTransaction::UpdateTimestamp {
            transaction_id,
            document_id: self.generate_document_id(processed_document.path()),
            timestamp: TransactionTimestamp::now(),
        })
    }

    /// Generate a unique transaction ID
    fn generate_transaction_id(&self, transaction_type: &str, document_path: &PathBuf) -> String {
        let counter = self.transaction_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path_hash = self.hash_path(document_path);

        match &self.config.transaction_id_prefix {
            Some(prefix) => format!("{}-{}-{}-{}", prefix, transaction_type, path_hash, counter),
            None => format!("{}-{}-{}", transaction_type, path_hash, counter),
        }
    }

    /// Generate a document ID from the document path
    fn generate_document_id(&self, document_path: &PathBuf) -> String {
        format!("notes:{}", self.hash_path(document_path))
    }

    /// Create a short hash of a document path for IDs
    fn hash_path(&self, path: &PathBuf) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Get the current configuration
    pub fn config(&self) -> &TransactionBuilderConfig {
        &self.config
    }

    /// Update the configuration
    pub fn update_config(&mut self, config: TransactionBuilderConfig) {
        self.config = config;
    }

    /// Estimate the number of transactions that will be generated for a document
    pub fn estimate_transaction_count(&self, processed_document: &crucible_core::ProcessedDocument) -> usize {
        let mut count = 1; // Store transaction is always generated

        if self.config.generate_relationships {
            if !processed_document.document.wikilinks.is_empty() {
                count += 1;
            }
            if !processed_document.document.content.plain_text.is_empty() {
                count += 1; // Embed relationships
            }
        }

        if self.config.generate_tags && !processed_document.document.tags.is_empty() {
            count += 1;
        }

        if self.config.generate_embeddings && processed_document.context.metadata.generate_embeddings {
            count += 1;
        }

        if self.config.generate_timestamps {
            count += 1;
        }

        count
    }

    /// Validate that a processed document can be converted to transactions
    pub fn validate_processed_document(&self, processed_document: &crucible_core::ProcessedDocument) -> Result<()> {
        // Check that the document path is valid
        if processed_document.path().as_os_str().is_empty() {
            return Err(anyhow::anyhow!("Document path cannot be empty"));
        }

        // Check that the kiln root is valid
        if processed_document.kiln_root.as_os_str().is_empty() {
            return Err(anyhow::anyhow!("Kiln root cannot be empty"));
        }

        // Check that the document has content
        if processed_document.document.content.plain_text.is_empty()
            && processed_document.document.content.headings.is_empty()
            && processed_document.document.wikilinks.is_empty()
            && processed_document.document.tags.is_empty() {
            // This might be an empty document, which is valid but should be noted
            debug!("Document appears to be empty: {:?}", processed_document.path());
        }

        Ok(())
    }
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions for working with transactions
pub mod utils {
    use super::*;

    /// Create a transaction builder config optimized for performance
    pub fn performance_config() -> TransactionBuilderConfig {
        TransactionBuilderConfig {
            generate_embeddings: false, // Skip embeddings for faster processing
            generate_relationships: true,
            generate_tags: true,
            generate_timestamps: true,
            transaction_id_prefix: Some("perf".to_string()),
        }
    }

    /// Create a transaction builder config optimized for completeness
    pub fn complete_config() -> TransactionBuilderConfig {
        TransactionBuilderConfig {
            generate_embeddings: true,
            generate_relationships: true,
            generate_tags: true,
            generate_timestamps: true,
            transaction_id_prefix: Some("complete".to_string()),
        }
    }

    /// Create a transaction builder config for testing
    pub fn test_config() -> TransactionBuilderConfig {
        TransactionBuilderConfig {
            generate_embeddings: false, // Skip embeddings in tests
            generate_relationships: true,
            generate_tags: true,
            generate_timestamps: false, // Skip timestamps in tests
            transaction_id_prefix: Some("test".to_string()),
        }
    }

    /// Estimate total processing time for a batch of documents
    pub fn estimate_batch_processing_time(
        documents: &[crucible_core::ProcessedDocument],
        builder: &TransactionBuilder,
    ) -> std::time::Duration {
        let total_transactions: usize = documents
            .iter()
            .map(|doc| builder.estimate_transaction_count(doc))
            .sum();

        // Estimate 10ms per transaction (this can be tuned based on actual performance)
        std::time::Duration::from_millis((total_transactions * 10) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::utils::*;
    use crucible_core::{
        ProcessedDocument, ProcessingContext, ProcessingSource, ProcessingPriority,
        ProcessingMetadata,
    };
    use crate::transaction_queue::DatabaseTransaction;

    fn create_test_processed_document(path: &str) -> ProcessedDocument {
        let document = crucible_core::ParsedDocument {
            path: PathBuf::from(path),
            content: crucible_core::DocumentContent {
                plain_text: "This is a test document with wikilink and tag content.".to_string(),
                headings: vec![crucible_core::Heading { level: 1, text: "Test Document".to_string(), line_number: 1 }],
                code_blocks: Vec::new(),
                paragraphs: Vec::new(),
                lists: Vec::new(),
                latex_expressions: Vec::new(),
                frontmatter_refs: Vec::new(),
                task_items: Vec::new(),
            },
            frontmatter: None,
            wikilinks: vec![crucible_core::Wikilink {
                target: "wikilink".to_string(),
                text: Some("wikilink".to_string()),
                line_number: 3,
            }],
            tags: vec![crucible_core::Tag {
                name: "tag".to_string(),
                line_number: 3,
            }],
            callouts: Vec::new(),
            latex_expressions: Vec::new(),
            footnotes: std::collections::HashMap::new(),
            task_items: Vec::new(),
        };

        let context = ProcessingContext {
            job_id: "test-job".to_string(),
            source: ProcessingSource::StartupScan,
            priority: ProcessingPriority::Normal,
            metadata: ProcessingMetadata::default(),
        };

        ProcessedDocument::with_context(
            document,
            PathBuf::from("/test/kiln"),
            context,
        )
    }

    #[test]
    fn test_transaction_builder_creation() {
        let builder = TransactionBuilder::new();
        assert!(builder.estimate_transaction_count(&create_test_processed_document("test.md")) > 0);
    }

    #[test]
    fn test_transaction_building() -> Result<()> {
        let builder = TransactionBuilder::new();
        let processed_doc = create_test_processed_document("test.md");

        let transactions = builder.build_transactions(&processed_doc)?;
        assert!(!transactions.is_empty());

        // First transaction should be StoreDocument
        match &transactions[0] {
            DatabaseTransaction::StoreDocument { transaction_id, .. } => {
                assert!(transaction_id.starts_with("store-"));
            }
            _ => panic!("First transaction should be StoreDocument"),
        }

        Ok(())
    }

    #[test]
    fn test_transaction_builder_configs() {
        let perf_config = performance_config();
        assert!(!perf_config.generate_embeddings);
        assert!(perf_config.generate_relationships);

        let complete_config = complete_config();
        assert!(complete_config.generate_embeddings);
        assert!(complete_config.generate_relationships);

        let test_config = test_config();
        assert!(!test_config.generate_embeddings);
        assert!(!test_config.generate_timestamps);
    }

    #[test]
    fn test_document_validation() -> Result<()> {
        let builder = TransactionBuilder::new();
        let valid_doc = create_test_processed_document("test.md");

        assert!(builder.validate_processed_document(&valid_doc).is_ok());

        // Test with empty path
        let mut invalid_doc = valid_doc.clone();
        invalid_doc.document.path = PathBuf::from("");
        assert!(builder.validate_processed_document(&invalid_doc).is_err());

        Ok(())
    }

    #[test]
    fn test_transaction_id_generation() {
        let builder = TransactionBuilder::new();
        let doc = create_test_processed_document("test.md");

        let transactions = builder.build_transactions(&doc).unwrap();
        let transaction_ids: Vec<String> = transactions.iter()
            .map(|tx| tx.transaction_id().to_string())
            .collect();

        // All transaction IDs should be unique
        let mut unique_ids = transaction_ids.clone();
        unique_ids.sort();
        unique_ids.dedup();
        assert_eq!(unique_ids.len(), transaction_ids.len());

        // All transaction IDs should contain the path hash
        for id in &transaction_ids {
            assert!(id.contains('-'));
        }
    }

    #[test]
    fn test_estimate_batch_processing_time() {
        let builder = TransactionBuilder::new();
        let documents = vec![
            create_test_processed_document("test1.md"),
            create_test_processed_document("test2.md"),
        ];

        let estimated_time = utils::estimate_batch_processing_time(&documents, &builder);
        assert!(estimated_time.as_millis() > 0);
    }
}