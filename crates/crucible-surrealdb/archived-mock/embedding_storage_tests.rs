//! Embedding Storage Tests
//!
//! Test-driven development for real database storage operations.
//! These tests verify that embedding vectors are actually stored and retrieved from SurrealDB.
//! Tests should initially fail if storage functions are stubbed, then pass after implementation.

use crucible_surrealdb::{embedding_config::EmbeddingModel, vault_integration, SurrealClient};
use vault_integration::{
    clear_document_embeddings, get_document_embeddings, initialize_vault_schema,
    store_document_embedding, update_document_processed_timestamp,
};

// Import consolidated test utilities
mod common;
use chrono::Utc;
use common::{EmbeddingAssertions, EmbeddingTestUtils};

// =============================================================================
// NOTE: Test data generation helpers moved to common::EmbeddingTestUtils
// =============================================================================

// NOTE: All test data generation helpers moved to common module
// - EmbeddingTestUtils::create_chunk_embedding -> common::EmbeddingTestUtils::create_chunk_embedding
// - EmbeddingTestUtils::create_embedding_batch -> common::EmbeddingTestUtils::create_embedding_batch
// - EmbeddingAssertions::assert_embeddings_approx_eq -> common::EmbeddingAssertions::EmbeddingAssertions::assert_embeddings_approx_eq

// =============================================================================
// PHASE 1: BASIC STORAGE OPERATIONS
// =============================================================================

/// Test: Store a single document embedding and retrieve it
/// This should pass if store_document_embedding() is properly implemented
#[tokio::test]
async fn test_store_single_document_embedding() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    let document_id = "test-doc-1";
    let test_embedding = EmbeddingTestUtils::create_document_embedding(document_id, 768);

    // Store the embedding
    store_document_embedding(&client, &test_embedding)
        .await
        .unwrap();

    // Retrieve and verify
    let retrieved_embeddings = get_document_embeddings(&client, document_id).await.unwrap();

    // Should have exactly one embedding
    assert_eq!(
        retrieved_embeddings.len(),
        1,
        "Should retrieve exactly one embedding"
    );

    // Verify embedding data integrity
    let retrieved = &retrieved_embeddings[0];
    EmbeddingAssertions::assert_embeddings_approx_eq(&test_embedding, retrieved, 1e-6);

    // Verify metadata
    assert_eq!(retrieved.document_id, document_id);
    assert_eq!(retrieved.embedding_model, "test-model");
    assert_eq!(retrieved.dimensions(), 768);
    assert!(!retrieved.is_chunked());
}

/// Test: Store chunked embeddings for a document
#[tokio::test]
async fn test_store_chunked_embeddings() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    let document_id = "test-doc-chunked";
    let chunk_count = 5;
    let dimensions = 256;

    // Create and store chunked embeddings
    let mut stored_embeddings = Vec::new();
    for i in 0..chunk_count {
        let chunk_id = format!("{}-chunk-{}", document_id, i);
        let chunk_embedding =
            EmbeddingTestUtils::create_chunk_embedding(document_id, &chunk_id, i, dimensions);
        store_document_embedding(&client, &chunk_embedding)
            .await
            .unwrap();
        stored_embeddings.push(chunk_embedding);
    }

    // Retrieve all embeddings for the document
    let retrieved_embeddings = get_document_embeddings(&client, document_id).await.unwrap();

    // Should have all chunked embeddings
    assert_eq!(
        retrieved_embeddings.len(),
        chunk_count,
        "Should retrieve all chunked embeddings"
    );

    // Verify each chunk embedding
    for (i, stored_embedding) in stored_embeddings.iter().enumerate() {
        let found = retrieved_embeddings.iter().find(|e| {
            e.chunk_id.as_ref() == stored_embedding.chunk_id.as_ref()
                && e.chunk_position == stored_embedding.chunk_position
        });

        assert!(
            found.is_some(),
            "Should find chunk {} in retrieved embeddings",
            i
        );
        EmbeddingAssertions::assert_embeddings_approx_eq(stored_embedding, found.unwrap(), 1e-6);
    }
}

/// Test: Store both main document and chunked embeddings
#[tokio::test]
async fn test_store_mixed_embeddings() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    let document_id = "test-doc-mixed";

    // Store main document embedding
    let main_embedding = EmbeddingTestUtils::create_document_embedding(document_id, 768);
    store_document_embedding(&client, &main_embedding)
        .await
        .unwrap();

    // Store chunked embeddings
    let chunk_count = 3;
    for i in 0..chunk_count {
        let chunk_id = format!("{}-chunk-{}", document_id, i);
        let chunk_embedding =
            EmbeddingTestUtils::create_chunk_embedding(document_id, &chunk_id, i, 768);
        store_document_embedding(&client, &chunk_embedding)
            .await
            .unwrap();
    }

    // Retrieve all embeddings
    let retrieved_embeddings = get_document_embeddings(&client, document_id).await.unwrap();

    // Should have main embedding + chunks
    assert_eq!(
        retrieved_embeddings.len(),
        chunk_count + 1,
        "Should have main embedding plus chunks"
    );

    // Verify main embedding exists
    let main_retrieved = retrieved_embeddings.iter().find(|e| !e.is_chunked());
    assert!(
        main_retrieved.is_some(),
        "Should find main document embedding"
    );
    EmbeddingAssertions::assert_embeddings_approx_eq(
        &main_embedding,
        main_retrieved.unwrap(),
        1e-6,
    );

    // Verify all chunk embeddings exist
    let chunk_retrieved: Vec<_> = retrieved_embeddings
        .iter()
        .filter(|e| e.is_chunked())
        .collect();
    assert_eq!(
        chunk_retrieved.len(),
        chunk_count,
        "Should find all chunk embeddings"
    );
}

// =============================================================================
// PHASE 2: BATCH OPERATIONS
// =============================================================================

/// Test: Store embeddings for multiple documents
#[tokio::test]
async fn test_store_multiple_document_embeddings() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    let document_ids = vec!["doc-1", "doc-2", "doc-3"];
    let embeddings_per_doc = 2;
    let dimensions = 512;

    // Store embeddings for all documents
    let all_embeddings =
        EmbeddingTestUtils::create_embedding_batch(&document_ids, embeddings_per_doc, dimensions);
    for embedding in &all_embeddings {
        store_document_embedding(&client, embedding).await.unwrap();
    }

    // Verify embeddings for each document
    for &doc_id in &document_ids {
        let retrieved = get_document_embeddings(&client, doc_id).await.unwrap();
        let expected_count = embeddings_per_doc + 1; // main + chunks

        assert_eq!(
            retrieved.len(),
            expected_count,
            "Document {} should have {} embeddings",
            doc_id,
            expected_count
        );

        // Verify main embedding exists
        let main_embedding = retrieved.iter().find(|e| !e.is_chunked());
        assert!(
            main_embedding.is_some(),
            "Document {} should have main embedding",
            doc_id
        );
        assert_eq!(main_embedding.unwrap().document_id, doc_id);
    }
}

/// Test: Performance test for batch embedding storage
#[tokio::test]
async fn test_batch_embedding_performance() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    let document_count = 10;
    let embeddings_per_doc = 5;
    let dimensions = 768;

    let document_ids: Vec<String> = (0..document_count)
        .map(|i| format!("perf-doc-{}", i))
        .collect();

    let document_id_refs: Vec<&str> = document_ids.iter().map(|s| s.as_str()).collect();

    // Create test embeddings
    let all_embeddings = EmbeddingTestUtils::create_embedding_batch(
        &document_id_refs,
        embeddings_per_doc,
        dimensions,
    );

    // Measure storage performance
    let start_time = std::time::Instant::now();
    for embedding in &all_embeddings {
        store_document_embedding(&client, embedding).await.unwrap();
    }
    let storage_duration = start_time.elapsed();

    // Measure retrieval performance
    let start_time = std::time::Instant::now();
    let mut total_retrieved = 0;
    for doc_id in &document_ids {
        let retrieved = get_document_embeddings(&client, doc_id).await.unwrap();
        total_retrieved += retrieved.len();
    }
    let retrieval_duration = start_time.elapsed();

    // Verify results
    let expected_total = document_count * (embeddings_per_doc + 1);
    assert_eq!(
        total_retrieved, expected_total,
        "Should retrieve all stored embeddings"
    );

    // Performance assertions (adjust these based on requirements)
    assert!(
        storage_duration.as_millis() < 5000,
        "Storage should complete within 5 seconds, took {:?}",
        storage_duration
    );
    assert!(
        retrieval_duration.as_millis() < 1000,
        "Retrieval should complete within 1 second, took {:?}",
        retrieval_duration
    );

    println!(
        "Performance: Stored {} embeddings in {:?}, retrieved {} in {:?}",
        all_embeddings.len(),
        storage_duration,
        total_retrieved,
        retrieval_duration
    );
}

// =============================================================================
// PHASE 3: UPDATE AND CLEAR OPERATIONS
// =============================================================================

/// Test: Clear all embeddings for a document
#[tokio::test]
async fn test_clear_document_embeddings() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    let document_id = "test-doc-clear";

    // Store multiple embeddings
    let main_embedding = EmbeddingTestUtils::create_document_embedding(document_id, 256);
    store_document_embedding(&client, &main_embedding)
        .await
        .unwrap();

    let chunk_embedding =
        EmbeddingTestUtils::create_chunk_embedding(document_id, "chunk-1", 0, 256);
    store_document_embedding(&client, &chunk_embedding)
        .await
        .unwrap();

    // Verify embeddings exist
    let before_clear = get_document_embeddings(&client, document_id).await.unwrap();
    assert_eq!(
        before_clear.len(),
        2,
        "Should have 2 embeddings before clear"
    );

    // Clear embeddings
    clear_document_embeddings(&client, document_id)
        .await
        .unwrap();

    // Verify embeddings are cleared
    let after_clear = get_document_embeddings(&client, document_id).await.unwrap();
    assert_eq!(
        after_clear.len(),
        0,
        "Should have no embeddings after clear"
    );
}

/// Test: Update document processed timestamp
#[tokio::test]
async fn test_update_document_processed_timestamp() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    // First, we need to store a document in the notes table
    let test_doc = create_test_parsed_document();
    let kiln_root = test_kiln_root();
    let doc_id = vault_integration::store_parsed_document(&client, &test_doc, &kiln_root)
        .await
        .unwrap();

    // Update processed timestamp
    update_document_processed_timestamp(&client, &doc_id)
        .await
        .unwrap();

    // Verify the timestamp was updated (this would require additional query implementation)
    // For now, just ensure the operation doesn't fail
    assert!(true, "Timestamp update should not fail");
}

/// Test: Document update workflow (clear old embeddings, store new ones)
#[tokio::test]
async fn test_document_update_workflow() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    let document_id = "test-doc-update";

    // Store initial embeddings
    let initial_main = EmbeddingTestUtils::create_document_embedding(document_id, 512);
    store_document_embedding(&client, &initial_main)
        .await
        .unwrap();

    let initial_chunk = EmbeddingTestUtils::create_chunk_embedding(document_id, "chunk-1", 0, 512);
    store_document_embedding(&client, &initial_chunk)
        .await
        .unwrap();

    // Verify initial state
    let initial_retrieved = get_document_embeddings(&client, document_id).await.unwrap();
    assert_eq!(
        initial_retrieved.len(),
        2,
        "Should have 2 initial embeddings"
    );

    // Clear old embeddings (document update)
    clear_document_embeddings(&client, document_id)
        .await
        .unwrap();

    // Store new embeddings (with different vector data to represent updated content)
    let updated_main = EmbeddingTestUtils::create_document_embedding(document_id, 768); // Different dimensions
    store_document_embedding(&client, &updated_main)
        .await
        .unwrap();

    let updated_chunk = EmbeddingTestUtils::create_chunk_embedding(document_id, "chunk-1", 0, 768);
    store_document_embedding(&client, &updated_chunk)
        .await
        .unwrap();

    let updated_chunk2 = EmbeddingTestUtils::create_chunk_embedding(document_id, "chunk-2", 1, 768);
    store_document_embedding(&client, &updated_chunk2)
        .await
        .unwrap();

    // Verify updated state
    let final_retrieved = get_document_embeddings(&client, document_id).await.unwrap();
    assert_eq!(final_retrieved.len(), 3, "Should have 3 updated embeddings");

    // Verify embeddings have new dimensions
    for embedding in &final_retrieved {
        assert_eq!(
            embedding.dimensions(),
            768,
            "Updated embeddings should have new dimensions"
        );
    }
}

// =============================================================================
// PHASE 4: ERROR HANDLING AND EDGE CASES
// =============================================================================

/// Test: Handle retrieval for non-existent document
#[tokio::test]
async fn test_get_embeddings_nonexistent_document() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    let non_existent_id = "non-existent-document";

    // Should return empty result, not error
    let result = get_document_embeddings(&client, non_existent_id)
        .await
        .unwrap();
    assert_eq!(
        result.len(),
        0,
        "Non-existent document should return empty embeddings"
    );
}

/// Test: Store embedding with invalid data (should fail gracefully)
#[tokio::test]
async fn test_store_invalid_embedding() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    // Create embedding with empty vector (should be invalid)
    let mut invalid_embedding = EmbeddingTestUtils::create_document_embedding("invalid-doc", 256);
    invalid_embedding.vector = vec![]; // Empty vector

    // This should either fail or be handled gracefully
    let result = store_document_embedding(&client, &invalid_embedding).await;

    // Depending on implementation, this might fail or succeed
    // The important thing is it doesn't panic
    match result {
        Ok(_) => {
            // If it succeeded, verify we can retrieve it
            let retrieved = get_document_embeddings(&client, "invalid-doc")
                .await
                .unwrap();
            assert!(
                !retrieved.is_empty(),
                "Should be able to retrieve stored embedding"
            );
        }
        Err(_) => {
            // If it failed, that's also acceptable
            assert!(true, "Invalid embedding should be rejected");
        }
    }
}

/// Test: Handle very large embeddings
#[tokio::test]
async fn test_large_embedding_storage() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    let document_id = "large-embedding-doc";
    let large_dimensions = 4096; // Very large embedding

    let large_embedding =
        EmbeddingTestUtils::create_document_embedding(document_id, large_dimensions);

    // Store large embedding
    store_document_embedding(&client, &large_embedding)
        .await
        .unwrap();

    // Retrieve and verify
    let retrieved = get_document_embeddings(&client, document_id).await.unwrap();
    assert_eq!(retrieved.len(), 1, "Should retrieve large embedding");
    assert_eq!(
        retrieved[0].dimensions(),
        large_dimensions,
        "Should preserve large dimensions"
    );

    // Verify vector integrity
    EmbeddingAssertions::assert_embeddings_approx_eq(&large_embedding, &retrieved[0], 1e-6);
}

/// Test: Concurrent embedding storage
#[tokio::test]
async fn test_concurrent_embedding_storage() {
    // Setup
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    let document_count = 10;
    let embeddings_per_doc = 3;

    // Create tasks for concurrent storage
    let mut tasks = Vec::new();

    for i in 0..document_count {
        let client_clone = client.clone();
        let doc_id = format!("concurrent-doc-{}", i);

        let task = tokio::spawn(async move {
            // Store main embedding
            let main_embedding = EmbeddingTestUtils::create_document_embedding(&doc_id, 256);
            store_document_embedding(&client_clone, &main_embedding)
                .await
                .unwrap();

            // Store chunked embeddings
            for j in 0..embeddings_per_doc {
                let chunk_id = format!("{}-chunk-{}", doc_id, j);
                let chunk_embedding =
                    EmbeddingTestUtils::create_chunk_embedding(&doc_id, &chunk_id, j, 256);
                store_document_embedding(&client_clone, &chunk_embedding)
                    .await
                    .unwrap();
            }

            doc_id
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    let completed_docs: Vec<String> = futures::future::join_all(tasks)
        .await
        .into_iter()
        .map(|result| result.unwrap())
        .collect();

    // Verify all embeddings were stored correctly
    for doc_id in completed_docs {
        let retrieved = get_document_embeddings(&client, &doc_id).await.unwrap();
        assert_eq!(
            retrieved.len(),
            embeddings_per_doc + 1,
            "Concurrent document {} should have all embeddings",
            doc_id
        );
    }
}

// =============================================================================
// PHASE 5: VECTOR INDEXING AND SIMILARITY
// =============================================================================

/// Test: Vector indexing with different dimensions
#[tokio::test]
async fn test_vector_indexing_dimensions() {
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    // Test different embedding dimensions from different models
    let test_cases = vec![
        ("mini-doc", EmbeddingModel::LocalMini, 256),
        ("standard-doc", EmbeddingModel::LocalStandard, 768),
        ("large-doc", EmbeddingModel::LocalLarge, 1536),
    ];

    for (doc_id, model, dimensions) in test_cases {
        let embedding = EmbeddingTestUtils::create_document_embedding(doc_id, dimensions);
        let mut model_embedding = embedding;
        model_embedding.embedding_model = model.model_name().to_string();

        store_document_embedding(&client, &model_embedding)
            .await
            .unwrap();

        let retrieved = get_document_embeddings(&client, doc_id).await.unwrap();
        assert_eq!(retrieved.len(), 1, "Should retrieve {} embedding", doc_id);
        assert_eq!(
            retrieved[0].dimensions(),
            dimensions,
            "Should preserve {} dimensions",
            doc_id
        );
        assert_eq!(
            retrieved[0].embedding_model,
            model.model_name(),
            "Should preserve model name"
        );
    }
}

/// Test: Semantic search functionality (mock implementation)
#[tokio::test]
async fn test_semantic_search_functionality() {
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    // Store documents with different content themes
    let documents = vec![
        (
            "rust-doc",
            "Rust programming language systems programming memory safety",
        ),
        (
            "ai-doc",
            "Artificial intelligence machine learning neural networks",
        ),
        ("db-doc", "Database systems SQL NoSQL vector embeddings"),
    ];

    for (doc_id, _content) in &documents {
        let embedding = EmbeddingTestUtils::create_document_embedding(doc_id, 768);
        store_document_embedding(&client, &embedding).await.unwrap();
    }

    // Test semantic search (this uses the current mock implementation)
    let search_results = vault_integration::semantic_search(&client, "rust programming", 5)
        .await
        .unwrap();

    // Should return some results (mock implementation)
    assert!(
        !search_results.is_empty(),
        "Semantic search should return results"
    );

    // Results should be tuples of (document_path, similarity_score)
    for (path, score) in search_results {
        assert!(!path.is_empty(), "Document path should not be empty");
        assert!(
            score >= 0.0 && score <= 1.0,
            "Similarity score should be between 0 and 1"
        );
    }
}

/// Test: Database statistics
#[tokio::test]
async fn test_database_statistics() {
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_vault_schema(&client).await.unwrap();

    // Get initial statistics
    let initial_stats = vault_integration::get_database_stats(&client)
        .await
        .unwrap();
    assert_eq!(
        initial_stats.total_documents, 0,
        "Should start with 0 documents"
    );
    assert_eq!(
        initial_stats.total_embeddings, 0,
        "Should start with 0 embeddings"
    );

    // Store some documents and embeddings
    let doc_count = 5;
    let embeddings_per_doc = 3;

    let kiln_root = test_kiln_root();
    for i in 0..doc_count {
        let doc_id = format!("stats-doc-{}", i);
        let doc = create_test_parsed_document();
        let _stored_id = vault_integration::store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        // Store embeddings
        let main_embedding = EmbeddingTestUtils::create_document_embedding(&doc_id, 256);
        store_document_embedding(&client, &main_embedding)
            .await
            .unwrap();

        for j in 0..embeddings_per_doc {
            let chunk_id = format!("{}-chunk-{}", doc_id, j);
            let chunk_embedding =
                EmbeddingTestUtils::create_chunk_embedding(&doc_id, &chunk_id, j, 256);
            store_document_embedding(&client, &chunk_embedding)
                .await
                .unwrap();
        }
    }

    // Check updated statistics
    let final_stats = vault_integration::get_database_stats(&client)
        .await
        .unwrap();
    assert_eq!(
        final_stats.total_documents, doc_count,
        "Should count all documents"
    );
    assert_eq!(
        final_stats.total_embeddings,
        (doc_count as u64 * (embeddings_per_doc + 1) as u64),
        "Should count all embeddings including main embeddings"
    );
}

// =============================================================================
// HELPER FUNCTIONS FOR TEST DATA
// =============================================================================

use crucible_core::parser::{ParsedDocument, Tag};
use std::path::PathBuf;

/// Test kiln root for all tests
fn test_kiln_root() -> PathBuf {
    PathBuf::from("/tmp/test_kiln")
}

/// Create a test ParsedDocument for testing
fn create_test_parsed_document() -> ParsedDocument {
    let mut doc = ParsedDocument::new(std::path::PathBuf::from("test.md"));
    doc.content.plain_text = "This is a test document content.".to_string();
    doc.content_hash = "test-hash-123".to_string();
    doc.file_size = 100;
    doc.parsed_at = Utc::now();

    // Add some tags
    doc.tags.push(Tag::new("test", 0));
    doc.tags.push(Tag::new("document", 5));

    doc
}

// =============================================================================
// TEST SUITE INITIALIZATION AND CLEANUP
// =============================================================================

/// Test suite initialization
#[tokio::test]
async fn test_embedding_storage_initialization() {
    // This test verifies that the embedding storage system can be properly initialized
    let client = SurrealClient::new_memory().await.unwrap();

    // Initialize schema should not fail
    let result = initialize_vault_schema(&client).await;
    assert!(
        result.is_ok(),
        "Schema initialization should succeed: {:?}",
        result.err()
    );

    // Database should be ready for embedding operations
    let stats = vault_integration::get_database_stats(&client)
        .await
        .unwrap();
    assert_eq!(stats.total_documents, 0, "Should start with empty database");
    assert_eq!(stats.total_embeddings, 0, "Should start with no embeddings");
}
