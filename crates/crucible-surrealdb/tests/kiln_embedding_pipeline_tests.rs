//! Test-Driven Development Tests for Kiln Embedding Pipeline
//!
//! This test file implements TDD methodology for Phase 2.2 Task 4:
//! Connect Phase 1 Parsed Documents to Embedding Pipeline.
//!
//! Tests are written to FAIL first, then implementation will make them pass.
//!
//! **Phase 1 Input**: ParsedDocument structures from kiln scanning
//! **Phase 2.1 Input Required**: (document_id: String, content: String) for embedding thread pool
//! **Goal**: End-to-end pipeline: ParsedDocument → transform → embed → store → search

use crucible_core::parser::types::*;
use crucible_core::parser::ParsedDocument;
use crucible_surrealdb::embedding_config::*;
use crucible_surrealdb::embedding_pool::*;
use crucible_surrealdb::kiln_pipeline_connector::*;
use crucible_surrealdb::kiln_scanner::*;
use crucible_surrealdb::SurrealClient;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;

/// Test kiln root for all tests
fn test_kiln_root() -> PathBuf {
    PathBuf::from("/tmp/test_kiln")
}

/// Create test ParsedDocument with realistic content
async fn create_test_parsed_document(
    file_path: PathBuf,
    title: &str,
    content: &str,
) -> ParsedDocument {
    let mut document = ParsedDocument::new(file_path.clone());

    // Set up frontmatter
    let frontmatter_raw = format!(
        r#"---
title: {}
tags: [test, example]
created: 2024-01-01
---
"#,
        title
    );

    document.frontmatter = Some(Frontmatter::new(frontmatter_raw, FrontmatterFormat::Yaml));

    // Set up content
    document.content.plain_text = content.to_string();

    // Add some structure to content
    document.content.headings.push(Heading {
        level: 1,
        text: title.to_string(),
        offset: 0,
        id: Some(format!("h1-{}", title.to_lowercase().replace(' ', "-"))),
    });

    // Add wikilinks
    document.wikilinks.push(Wikilink {
        target: "Related Document".to_string(),
        alias: None,
        offset: 100,
        is_embed: false,
        block_ref: None,
        heading_ref: None,
    });

    // Add tags
    document.tags.push(Tag {
        name: "test".to_string(),
        path: vec!["test".to_string()],
        offset: 150,
    });
    document.tags.push(Tag {
        name: "example".to_string(),
        path: vec!["example".to_string()],
        offset: 155,
    });

    // Set metadata
    document.content_hash = format!("hash_{}", title.to_lowercase().replace(' ', "_"));
    document.file_size = content.len() as u64;

    document
}

/// Create multiple test documents for batch processing
async fn create_test_document_batch() -> Vec<ParsedDocument> {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    vec![
        create_test_parsed_document(
            base_path.join("doc1.md"),
            "First Document",
            "This is the first test document with some content for embedding generation.",
        )
        .await,
        create_test_parsed_document(
            base_path.join("doc2.md"),
            "Second Document",
            "This is the second test document with different content for testing batch processing.",
        )
        .await,
        create_test_parsed_document(
            base_path.join("nested/doc3.md"),
            "Nested Document",
            "This document is in a subdirectory to test path normalization and ID generation.",
        )
        .await,
    ]
}

#[cfg(test)]
mod tdd_kiln_embedding_pipeline_tests {

    use super::*;

    /// **TDD TEST 1**: End-to-end pipeline should transform ParsedDocument to embeddings
    ///
    /// **Expected Failure**: No connector exists between Phase 1 and Phase 2.1
    #[tokio::test]
    async fn test_tdd_end_to_end_pipeline_single_document() {
        // Arrange
        let config = EmbeddingConfig::optimize_for_resources();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let client = SurrealClient::new_memory().await.unwrap();

        let test_doc = create_test_document_batch().await.pop().unwrap();

        // Act & Assert (This should fail initially)
        let kiln_root = test_kiln_root();
        let connector = KilnPipelineConnector::new(thread_pool.clone(), kiln_root);
        let result = connector
            .process_document_to_embedding(&client, &test_doc)
            .await;

        // TDD: This should fail because no pipeline connection exists yet
        assert!(result.is_ok(), "Pipeline connector should be implemented");

        let processing_result = result.unwrap();
        assert_eq!(
            processing_result.document_id,
            generate_document_id_from_path(&test_doc.path, &test_kiln_root())
        );
        assert!(
            processing_result.embeddings_generated > 0,
            "Should generate embeddings"
        );
        assert!(processing_result.processing_time > Duration::from_secs(0));

        thread_pool.shutdown().await.unwrap();
    }

    /// **TDD TEST 2**: Batch processing of multiple ParsedDocuments
    ///
    /// **Expected Failure**: No batch processing coordination exists
    #[tokio::test]
    async fn test_tdd_batch_processing_multiple_documents() {
        // Arrange
        let config = EmbeddingConfig::optimize_for_resources();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let client = SurrealClient::new_memory().await.unwrap();

        let test_documents = create_test_document_batch().await;

        // Act & Assert (This should fail initially)
        let kiln_root = test_kiln_root();
        let connector = KilnPipelineConnector::new(thread_pool.clone(), kiln_root);
        let result = connector
            .process_documents_to_embeddings(&client, &test_documents)
            .await;

        // TDD: This should fail because no batch processing exists yet
        assert!(result.is_ok(), "Batch processing should be implemented");

        let batch_result = result.unwrap();
        assert_eq!(batch_result.total_documents, test_documents.len());
        assert_eq!(batch_result.successfully_processed, test_documents.len());
        assert!(batch_result.total_embeddings_generated > 0);
        assert!(batch_result.total_processing_time > Duration::from_secs(0));

        thread_pool.shutdown().await.unwrap();
    }

    /// **TDD TEST 3**: Document ID generation from file paths
    ///
    /// **Expected Failure**: No document ID generation logic exists
    #[tokio::test]
    async fn test_tdd_document_id_generation() {
        // Arrange
        let test_paths = vec![
            PathBuf::from("/kiln/document.md"),
            PathBuf::from("/kiln/nested/document.md"),
            PathBuf::from("/kiln/with spaces/document.md"),
            PathBuf::from("/kiln/with-special-chars/document_1.md"),
        ];

        // Act & Assert (This should fail initially)
        let kiln_root = test_kiln_root();
        for path in &test_paths {
            let document_id = generate_document_id_from_path(path, &kiln_root);

            // TDD: This should fail because no ID generation exists yet
            assert!(!document_id.is_empty(), "Document ID should not be empty");
            assert!(
                !document_id.contains('/'),
                "Document ID should not contain path separators"
            );
            assert!(
                !document_id.contains('\\'),
                "Document ID should not contain backslashes"
            );
            assert!(
                document_id.len() <= 255,
                "Document ID should be reasonably short"
            );

            // Test consistency
            let id2 = generate_document_id_from_path(path, &kiln_root);
            assert_eq!(
                document_id, id2,
                "Document ID generation should be consistent"
            );
        }
    }

    /// **TDD TEST 4**: ParsedDocument to embedding input transformation
    ///
    /// **Expected Failure**: No transformation logic exists
    #[tokio::test]
    async fn test_tdd_document_transformation() {
        // Arrange
        let test_doc = create_test_document_batch().await.pop().unwrap();

        // Act & Assert (This should fail initially)
        let config = KilnPipelineConfig::default();
        let kiln_root = test_kiln_root();
        let embedding_inputs =
            transform_parsed_document_to_embedding_inputs(&test_doc, &config, &kiln_root);

        // TDD: This should fail because no transformation exists yet
        assert!(
            !embedding_inputs.is_empty(),
            "Should generate at least one embedding input"
        );

        for (document_id, content) in &embedding_inputs {
            assert_eq!(
                document_id,
                &generate_document_id_from_path(&test_doc.path, &kiln_root),
                "Document ID should match"
            );
            assert!(!content.is_empty(), "Content should not be empty");
            assert!(
                content.len() <= 8000,
                "Content chunks should be reasonably sized"
            );
        }
    }

    /// **TDD TEST 5**: Change detection integration with embedding updates
    ///
    /// **Expected Failure**: No change detection connection exists
    #[tokio::test]
    async fn test_tdd_change_detection_integration() {
        // Arrange
        let config = EmbeddingConfig::optimize_for_resources();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let client = SurrealClient::new_memory().await.unwrap();

        let mut test_doc = create_test_document_batch().await.pop().unwrap();
        let kiln_root = test_kiln_root();
        let connector = KilnPipelineConnector::new(thread_pool.clone(), kiln_root);

        // Act: Initial processing
        let initial_result = connector
            .process_document_to_embedding(&client, &test_doc)
            .await;
        assert!(initial_result.is_ok(), "Initial processing should work");

        // Simulate document change
        test_doc.content_hash = "changed_hash_12345".to_string();
        test_doc.content.plain_text =
            "Updated content with new information for embedding.".to_string();

        // Act & Assert (This should fail initially)
        let update_result = connector
            .process_document_to_embedding(&client, &test_doc)
            .await;

        // TDD: This should fail because no change detection integration exists yet
        assert!(
            update_result.is_ok(),
            "Update processing should be implemented"
        );

        let update_processing_result = update_result.unwrap();
        assert!(
            update_processing_result.embeddings_generated > 0,
            "Should generate new embeddings"
        );
        assert!(update_processing_result.processing_time > Duration::from_secs(0));

        thread_pool.shutdown().await.unwrap();
    }

    /// **TDD TEST 6**: Error handling for pipeline failures
    ///
    /// **Expected Failure**: No comprehensive error handling exists
    #[tokio::test]
    async fn test_tdd_error_handling() {
        // Arrange
        let config = EmbeddingConfig::optimize_for_resources();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let client = SurrealClient::new_memory().await.unwrap();

        // Create a problematic document (empty content)
        let temp_dir = TempDir::new().unwrap();
        let problematic_doc =
            create_test_parsed_document(temp_dir.path().join("empty.md"), "Empty Document", "")
                .await;

        let kiln_root = test_kiln_root();
        let connector = KilnPipelineConnector::new(thread_pool.clone(), kiln_root);

        // Act & Assert (This should fail initially)
        let result = connector
            .process_document_to_embedding(&client, &problematic_doc)
            .await;

        // TDD: This should fail because no error handling exists yet
        // Should handle empty content gracefully
        match result {
            Ok(_) => {} // Should handle empty content successfully
            Err(e) => {
                // Or return a specific error for empty content
                assert!(
                    e.to_string().contains("empty") || e.to_string().contains("content"),
                    "Error should mention content issue"
                );
            }
        }

        thread_pool.shutdown().await.unwrap();
    }

    /// **TDD TEST 7**: Performance testing with realistic document sets
    ///
    /// **Expected Failure**: No performance optimization exists
    #[tokio::test]
    async fn test_tdd_performance_with_realistic_documents() {
        // Arrange
        let config = EmbeddingConfig::optimize_for_resources();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let client = SurrealClient::new_memory().await.unwrap();

        // Create a larger set of test documents
        let temp_dir = TempDir::new().unwrap();
        let mut test_documents = Vec::new();

        for i in 0..10 {
            let content = format!(
                r#"Document {}:

This is a longer test document with multiple paragraphs and sections.
It contains enough content to test realistic embedding generation performance.

## Section 1

Some content here with multiple sentences. The content should be long enough
to test chunking behavior and embedding generation performance.

## Section 2

More content here to ensure we have enough text for meaningful testing.
This paragraph adds additional length to the document.

## Conclusion

Final section with concluding remarks for document {}.
"#,
                i + 1,
                i + 1
            );

            let doc = create_test_parsed_document(
                temp_dir.path().join(format!("document_{}.md", i + 1)),
                &format!("Document {}", i + 1),
                &content,
            )
            .await;

            test_documents.push(doc);
        }

        let kiln_root = test_kiln_root();
        let connector = KilnPipelineConnector::new(thread_pool.clone(), kiln_root);
        let start_time = std::time::Instant::now();

        // Act & Assert (This should fail initially)
        let result = connector
            .process_documents_to_embeddings(&client, &test_documents)
            .await;

        let total_time = start_time.elapsed();

        // TDD: This should fail because no performance optimization exists yet
        assert!(result.is_ok(), "Performance testing should work");

        let batch_result = result.unwrap();
        assert_eq!(batch_result.successfully_processed, test_documents.len());
        assert!(batch_result.total_embeddings_generated > 0);

        // Performance assertions (adjust based on requirements)
        assert!(
            total_time < Duration::from_secs(30),
            "Processing should complete within reasonable time"
        );
        assert!(batch_result.total_processing_time < Duration::from_secs(30));

        // Should process documents efficiently
        let avg_time_per_doc = batch_result.total_processing_time / test_documents.len() as u32;
        assert!(
            avg_time_per_doc < Duration::from_secs(5),
            "Average time per document should be reasonable"
        );

        thread_pool.shutdown().await.unwrap();
    }

    /// **TDD TEST 8**: Integration with existing kiln scanner
    ///
    /// **Expected Failure**: No integration with kiln scanner exists
    #[tokio::test]
    async fn test_tdd_kiln_scanner_integration() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_path_buf();

        // Create test files
        fs::write(kiln_path.join("doc1.md"), "# Doc 1\nContent 1")
            .await
            .unwrap();
        fs::write(kiln_path.join("doc2.md"), "# Doc 2\nContent 2")
            .await
            .unwrap();

        // Create subdirectory
        let subdir = kiln_path.join("subdir");
        fs::create_dir(&subdir).await.unwrap();
        fs::write(subdir.join("doc3.md"), "# Doc 3\nContent 3")
            .await
            .unwrap();

        // Set up components
        let config = EmbeddingConfig::optimize_for_resources();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let client = SurrealClient::new_memory().await.unwrap();
        let kiln_root = test_kiln_root();
        let connector = KilnPipelineConnector::new(thread_pool.clone(), kiln_root);

        // Scan kiln
        let scanner_config = KilnScannerConfig::default();
        let mut scanner =
            create_kiln_scanner_with_embeddings(scanner_config, &client, &thread_pool)
                .await
                .unwrap();
        let scan_result = scanner.scan_kiln_directory(&kiln_path).await.unwrap();

        // Act & Assert (This should fail initially)
        // Get the actual parsed documents (this would need to be implemented)
        let parsed_documents = get_parsed_documents_from_scan(&client, &scan_result).await;
        assert!(
            parsed_documents.len() > 0,
            "Should have parsed at least one document"
        );

        // Process through embedding pipeline
        let embedding_result = connector
            .process_documents_to_embeddings(&client, &parsed_documents)
            .await;

        // TDD: This should fail because no integration exists yet
        assert!(
            embedding_result.is_ok(),
            "Kiln scanner integration should be implemented"
        );

        let embedding_batch_result = embedding_result.unwrap();
        assert_eq!(
            embedding_batch_result.total_documents,
            parsed_documents.len()
        );
        assert!(embedding_batch_result.successfully_processed > 0);
        assert!(embedding_batch_result.total_embeddings_generated > 0);

        thread_pool.shutdown().await.unwrap();
    }

    /// **TDD TEST 9**: Document ID edge cases and special characters
    ///
    /// **Expected Failure**: No robust ID generation exists
    #[tokio::test]
    async fn test_tdd_document_id_edge_cases() {
        // Arrange: Test problematic paths
        let edge_cases = vec![
            // Special characters
            PathBuf::from("/kiln/file with spaces.md"),
            PathBuf::from("/kiln/file-with-dashes.md"),
            PathBuf::from("/kiln/file_with_underscores.md"),
            PathBuf::from("/kiln/file.with.dots.md"),
            PathBuf::from("/kiln/file(with)parentheses.md"),
            PathBuf::from("/kiln/file[with]brackets.md"),
            PathBuf::from("/kiln/file{with}braces.md"),
            PathBuf::from("/kiln/file'with'quotes.md"),
            PathBuf::from("/kiln/file\"with\"quotes.md"),
            PathBuf::from("/kiln/file#with#hashes.md"),
            PathBuf::from("/kiln/file@with@ats.md"),
            PathBuf::from("/kiln/file%with%percents.md"),
            PathBuf::from("/kiln/file+with+plus.md"),
            PathBuf::from("/kiln/file=with=equals.md"),
            PathBuf::from("/kiln/file&with&ampersands.md"),
            PathBuf::from("/kiln/file;with;semicolons.md"),
            PathBuf::from("/kiln/file:with:colons.md"),
            PathBuf::from("/kiln/file!with!exclamation.md"),
            PathBuf::from("/kiln/file?with?question.md"),
            PathBuf::from("/kiln/file*with*asterisks.md"),
            PathBuf::from("/kiln/file$with$dollars.md"),
            PathBuf::from("/kiln/file^with^carets.md"),
            PathBuf::from("/kiln/file`with`backticks.md"),
            PathBuf::from("/kiln/file~with~tildes.md"),
            PathBuf::from("/kiln/file|with|pipes.md"),
            PathBuf::from("/kiln/file<with>angles.md"),
            // Very long path
            PathBuf::from("/kiln/this/is/a/very/long/path/with/many/directories/to/test/id_generation/limits/document.md"),
            // Unicode characters
            PathBuf::from("/kiln/文档.md"), // Chinese
            PathBuf::from("/kiln/документ.md"), // Russian
            PathBuf::from("/kiln/ドキュメント.md"), // Japanese
            PathBuf::from("/kiln/مستند.md"), // Arabic
            // Mixed
            PathBuf::from("/kiln/mixed path with 中文 and русский and emoji 🚀.md"),
        ];

        // Act & Assert (This should fail initially)
        let kiln_root = test_kiln_root();
        for path in &edge_cases {
            let document_id = generate_document_id_from_path(path, &kiln_root);

            // TDD: This should fail because no robust ID generation exists yet
            assert!(
                !document_id.is_empty(),
                "ID should not be empty for path: {:?}",
                path
            );
            assert!(
                document_id.len() <= 255,
                "ID should be reasonable length for path: {:?}",
                path
            );

            // Should not contain filesystem path separators
            assert!(
                !document_id.contains('/'),
                "ID should not contain '/' for path: {:?}",
                path
            );
            assert!(
                !document_id.contains('\\'),
                "ID should not contain '\\' for path: {:?}",
                path
            );

            // Should be consistent
            let id2 = generate_document_id_from_path(path, &kiln_root);
            assert_eq!(
                document_id, id2,
                "ID generation should be consistent for path: {:?}",
                path
            );

            // Should be URL-safe (basic check)
            assert!(
                document_id.is_ascii(),
                "ID should be ASCII for URL safety: {:?}",
                path
            );
        }
    }

    /// **TDD TEST 10**: Metadata preservation through pipeline
    ///
    /// **Expected Failure**: No metadata preservation exists
    #[tokio::test]
    async fn test_tdd_metadata_preservation() {
        // Arrange
        let test_doc = create_test_document_batch().await.pop().unwrap();

        // Verify original document has metadata
        assert!(!test_doc.frontmatter.as_ref().unwrap().raw.is_empty());
        assert!(!test_doc.tags.is_empty());
        assert!(!test_doc.wikilinks.is_empty());
        assert!(!test_doc.content_hash.is_empty());

        // Act & Assert (This should fail initially)
        let config = KilnPipelineConfig::default();
        let kiln_root = test_kiln_root();
        let embedding_inputs =
            transform_parsed_document_to_embedding_inputs(&test_doc, &config, &kiln_root);

        // TDD: This should fail because no metadata preservation exists yet
        assert!(!embedding_inputs.is_empty());

        for (document_id, content) in &embedding_inputs {
            // The transformation should preserve essential metadata in some form
            assert!(!document_id.is_empty());
            assert!(!content.is_empty());

            // Should include title information
            assert!(content.contains(&test_doc.title()) || document_id.contains(&test_doc.title()));

            // Should preserve content hash for change detection
            // This might be stored alongside the embedding rather than in the content
        }
    }
}
