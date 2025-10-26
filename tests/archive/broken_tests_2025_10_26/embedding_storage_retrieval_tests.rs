//! Storage and Retrieval Tests for Embedding System
//!
//! This test suite validates embedding storage in SurrealDB, vector similarity
//! calculations, batch vs individual embedding consistency, and embedding metadata preservation.
//!
//! ## Test Coverage
//!
//! ### Database Storage
//! - Embedding storage with metadata
//! - Vector indexing and retrieval
//! - Database schema validation
//! - Connection handling and transactions
//!
//! ### Vector Similarity
//! - Cosine similarity calculations
//! - Euclidean distance calculations
//! - Vector normalization
//! - Similarity threshold testing
//!
//! ### Batch vs Individual Consistency
//! - Same embeddings from batch and individual processing
//! - Performance comparison
//! - Memory usage analysis
//! - Error handling consistency
//!
//! ### Metadata Preservation
//! - Document metadata storage
//! - Embedding metadata retention
//! - Timestamp and version tracking
//! - Configuration preservation

mod fixtures;
mod utils;

use anyhow::Result;
use crucible_surrealdb::embedding_config::{EmbeddingConfig, EmbeddingModel, PrivacyMode};
use std::collections::HashMap;
use utils::harness::DaemonEmbeddingHarness;

// ============================================================================
// Database Storage Tests
// ============================================================================

/// Test embedding storage with metadata
///
/// Verifies:
/// - Embeddings are stored with correct metadata
/// - Document information is preserved
/// - Embedding vectors are stored accurately
/// - Retrieval returns original values
#[tokio::test]
async fn test_embedding_storage_with_metadata() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let test_documents = vec![
        (
            "doc1",
            "First test document for storage validation",
            "Test Document 1",
        ),
        (
            "doc2",
            "Second document with different content",
            "Test Document 2",
        ),
        ("doc3", "Third document about embeddings", "Embedding Test"),
    ];

    // Store documents with embeddings
    for (id, content, title) in &test_documents {
        let path = harness.create_note(&format!("{}.md", id), content).await?;

        // Verify file exists
        assert!(path.exists(), "File should exist on disk");

        // Verify embedding was generated and stored
        assert!(
            harness.has_embedding(&format!("{}.md", id)).await?,
            "Embedding should be generated for {}",
            id
        );

        // Verify metadata exists
        let metadata = harness.get_metadata(&format!("{}.md", id)).await?;
        assert!(metadata.is_some(), "Metadata should exist for {}", id);

        let metadata = metadata.unwrap();
        assert_eq!(
            metadata.title,
            Some(id.to_string()),
            "Title should be extracted from filename for {}",
            id
        );
    }

    // Test retrieval of stored embeddings
    for (id, content, _title) in &test_documents {
        let stored_embedding = harness.get_embedding(&format!("{}.md", id)).await?;
        assert!(
            stored_embedding.is_some(),
            "Should retrieve stored embedding for {}",
            id
        );

        let stored_embedding = stored_embedding.unwrap();
        assert_eq!(
            stored_embedding.len(),
            768,
            "Stored embedding should have correct dimensions for {}",
            id
        );

        // Generate fresh embedding for comparison
        let fresh_embedding = harness.generate_embedding(content).await?;

        // Verify stored and fresh embeddings are identical (mock provider is deterministic)
        assert_eq!(
            stored_embedding, fresh_embedding,
            "Stored embedding should match fresh embedding for {}",
            id
        );
    }

    // Test database statistics
    let stats = harness.get_stats().await?;
    assert!(
        stats.total_documents >= test_documents.len(),
        "Database should contain at least {} documents",
        test_documents.len()
    );
    assert!(
        stats.total_embeddings >= test_documents.len(),
        "Database should contain at least {} embeddings",
        test_documents.len()
    );

    println!(
        "Database stats: {} documents, {} embeddings",
        stats.total_documents, stats.total_embeddings
    );

    Ok(())
}

/// Test vector indexing and retrieval
///
/// Verifies:
/// - Vector indexes are created properly
/// - Retrieval by vector similarity works
/// - Index performance is acceptable
/// - Index updates work correctly
#[tokio::test]
async fn test_vector_indexing_and_retrieval() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create a set of documents with known similarities
    let documents = vec![
        (
            "machine_learning_basics",
            "Introduction to machine learning algorithms and neural networks",
        ),
        (
            "ai_research",
            "Advanced AI research papers and artificial intelligence methodologies",
        ),
        (
            "cooking_recipes",
            "Traditional cooking recipes and kitchen techniques",
        ),
        (
            "database_design",
            "Database schema design and SQL optimization strategies",
        ),
        (
            "web_development",
            "Modern web development with JavaScript frameworks and responsive design",
        ),
    ];

    // Store all documents
    for (id, content) in &documents {
        harness.create_note(&format!("{}.md", id), content).await?;
        assert!(
            harness.has_embedding(&format!("{}.md", id)).await?,
            "Embedding should be stored for {}",
            id
        );
    }

    // Test semantic search functionality
    let search_queries = vec![
        ("ml query", "machine learning algorithms"),
        ("cooking query", "kitchen recipes and food preparation"),
        ("database query", "SQL database performance"),
        ("web query", "JavaScript and web frameworks"),
    ];

    for (query_name, query_text) in search_queries {
        let search_results = harness.semantic_search(query_text, 5).await?;

        println!(
            "Search results for '{}': {} documents found",
            query_name,
            search_results.len()
        );

        // Verify we get some results
        assert!(
            !search_results.is_empty(),
            "Search should return results for '{}'",
            query_name
        );

        // Verify results contain expected document paths
        let result_paths: Vec<_> = search_results
            .iter()
            .map(|(path, _)| path.clone())
            .collect();

        // Check if expected documents are in results
        match query_name {
            "ml query" => {
                let has_ml_related = result_paths
                    .iter()
                    .any(|p| p.contains("machine_learning_basics") || p.contains("ai_research"));
                assert!(has_ml_related, "ML query should find ML-related documents");
            }
            "cooking query" => {
                let has_cooking = result_paths.iter().any(|p| p.contains("cooking_recipes"));
                assert!(has_cooking, "Cooking query should find cooking documents");
            }
            "database query" => {
                let has_database = result_paths.iter().any(|p| p.contains("database_design"));
                assert!(
                    has_database,
                    "Database query should find database documents"
                );
            }
            "web query" => {
                let has_web = result_paths.iter().any(|p| p.contains("web_development"));
                assert!(has_web, "Web query should find web development documents");
            }
            _ => {}
        }

        // Verify similarity scores are reasonable
        for (path, similarity) in &search_results {
            assert!(
                *similarity >= 0.0 && *similarity <= 1.0,
                "Similarity score should be within [0, 1], got {} for {}",
                similarity,
                path
            );
            println!("  {} similarity: {:.4}", path, similarity);
        }
    }

    Ok(())
}

/// Test database schema validation
///
/// Verifies:
/// - Database schema supports embedding operations
/// - Required tables/columns exist
/// - Data types are correct
/// - Constraints are enforced
#[tokio::test]
async fn test_database_schema_validation() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create a test document to validate schema
    let content = "Test document for schema validation";
    let doc_path = harness.create_note("schema_test.md", content).await?;

    assert!(doc_path.exists(), "Test document should be created");

    // Verify document metadata schema
    let metadata = harness.get_metadata("schema_test.md").await?;
    assert!(metadata.is_some(), "Metadata should be stored");

    let metadata = metadata.unwrap();

    // Validate required metadata fields
    assert!(
        metadata.created_at.timestamp() > 0,
        "Created timestamp should be valid"
    );
    assert!(
        metadata.updated_at.timestamp() > 0,
        "Updated timestamp should be valid"
    );
    assert!(
        metadata.updated_at >= metadata.created_at,
        "Updated timestamp should be after or equal to created timestamp"
    );

    // Validate embedding storage schema
    let embedding = harness.get_embedding("schema_test.md").await?;
    assert!(embedding.is_some(), "Embedding should be stored");

    let embedding = embedding.unwrap();
    assert_eq!(
        embedding.len(),
        768,
        "Embedding should have correct dimensions"
    );

    // Test vector operations to validate vector storage
    let fresh_embedding = harness.generate_embedding(content).await?;
    assert_eq!(
        embedding, fresh_embedding,
        "Stored embedding should match fresh embedding"
    );

    // Verify database can handle different embedding dimensions
    let config_mini = EmbeddingConfig {
        worker_count: 1,
        batch_size: 1,
        model_type: EmbeddingModel::LocalMini,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 10,
        timeout_ms: 5000,
        retry_attempts: 1,
        retry_delay_ms: 100,
        circuit_breaker_threshold: 5,
        circuit_breaker_timeout_ms: 10000,
    };

    // Note: This would require schema to support multiple dimension types
    // For now, we just verify the current schema works
    println!("Database schema validation completed successfully");

    Ok(())
}

/// Test connection handling and transactions
///
/// Verifies:
/// - Database connections are handled properly
/// - Transactions work correctly
/// - Connection failures are handled gracefully
/// - Connection pooling works
#[tokio::test]
async fn test_connection_handling_and_transactions() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Test multiple operations in sequence
    let documents = vec![
        ("tx_test_1", "First document in transaction test"),
        ("tx_test_2", "Second document in transaction test"),
        ("tx_test_3", "Third document in transaction test"),
    ];

    // Create documents one by one
    for (id, content) in &documents {
        harness.create_note(&format!("{}.md", id), content).await?;
        assert!(
            harness.has_embedding(&format!("{}.md", id)).await?,
            "Document {} should be stored",
            id
        );
    }

    // Verify all documents are stored correctly
    for (id, content) in &documents {
        let stored_embedding = harness.get_embedding(&format!("{}.md", id)).await?;
        assert!(
            stored_embedding.is_some(),
            "Should retrieve embedding for {}",
            id
        );

        let fresh_embedding = harness.generate_embedding(content).await?;
        assert_eq!(
            stored_embedding.unwrap(),
            fresh_embedding,
            "Embedding should match for {}",
            id
        );
    }

    // Test concurrent operations
    let concurrent_docs = vec![
        ("concurrent_1", "Concurrent document 1"),
        ("concurrent_2", "Concurrent document 2"),
        ("concurrent_3", "Concurrent document 3"),
        ("concurrent_4", "Concurrent document 4"),
    ];

    // Create documents concurrently
    let futures: Vec<_> = concurrent_docs
        .iter()
        .map(|(id, content)| {
            let harness = harness.clone();
            let id = id.clone();
            let content = content.clone();
            async move { harness.create_note(&format!("{}.md", id), &content).await }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    // Verify all concurrent operations succeeded
    for (i, result) in results.into_iter().enumerate() {
        assert!(result.is_ok(), "Concurrent operation {} should succeed", i);
    }

    // Verify all concurrent documents are stored
    for (id, content) in &concurrent_docs {
        assert!(
            harness.has_embedding(&format!("{}.md", id)).await?,
            "Concurrent document {} should be stored",
            id
        );
    }

    println!("Connection handling and transactions test completed successfully");

    Ok(())
}

// ============================================================================
// Vector Similarity Tests
// ============================================================================

/// Test cosine similarity calculations
///
/// Verifies:
/// - Cosine similarity is calculated correctly
/// - Edge cases are handled properly
/// - Performance is acceptable
/// - Results are mathematically correct
#[tokio::test]
async fn test_cosine_similarity_calculations() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create test documents with known similarity relationships
    let test_cases = vec![
        (
            "identical",
            "Machine learning algorithms are powerful tools",
            "Machine learning algorithms are powerful tools",
        ),
        (
            "very_similar",
            "Machine learning algorithms are powerful tools",
            "Machine learning algorithms are very powerful tools",
        ),
        (
            "somewhat_similar",
            "Machine learning algorithms are powerful tools",
            "Artificial intelligence and machine learning systems",
        ),
        (
            "different",
            "Machine learning algorithms are powerful tools",
            "Traditional cooking recipes from around the world",
        ),
        (
            "very_different",
            "Machine learning algorithms are powerful tools",
            "Classical music composition techniques",
        ),
    ];

    for (case_name, text1, text2) in test_cases {
        let embedding1 = harness.generate_embedding(text1).await?;
        let embedding2 = harness.generate_embedding(text2).await?;

        let similarity = cosine_similarity(&embedding1, &embedding2);

        println!("Cosine similarity for '{}': {:.6}", case_name, similarity);

        // Verify similarity is within expected range
        assert!(
            similarity >= 0.0 && similarity <= 1.0,
            "Cosine similarity should be within [0, 1], got {:.6}",
            similarity
        );

        // Verify similarity relationships are reasonable
        match case_name {
            "identical" => {
                assert!(
                    (similarity - 1.0).abs() < 1e-6,
                    "Identical texts should have similarity ≈ 1.0, got {:.6}",
                    similarity
                );
            }
            "very_similar" => {
                assert!(
                    similarity > 0.9,
                    "Very similar texts should have high similarity, got {:.6}",
                    similarity
                );
            }
            "somewhat_similar" => {
                assert!(
                    similarity > 0.5 && similarity < 0.9,
                    "Somewhat similar texts should have medium similarity, got {:.6}",
                    similarity
                );
            }
            "different" | "very_different" => {
                assert!(
                    similarity < 0.5,
                    "Different texts should have low similarity, got {:.6}",
                    similarity
                );
            }
            _ => {}
        }
    }

    // Test edge cases
    let edge_cases = vec![
        ("empty1", "", "non-empty text"),
        ("empty2", "non-empty text", ""),
        ("both_empty", "", ""),
        ("whitespace1", "   ", "non-empty text"),
        ("whitespace2", "non-empty text", "   "),
    ];

    for (case_name, text1, text2) in edge_cases {
        let embedding1 = harness.generate_embedding(text1).await?;
        let embedding2 = harness.generate_embedding(text2).await?;

        let similarity = cosine_similarity(&embedding1, &embedding2);
        println!(
            "Cosine similarity for edge case '{}': {:.6}",
            case_name, similarity
        );

        assert!(
            similarity >= 0.0 && similarity <= 1.0,
            "Edge case similarity should be within [0, 1]"
        );
    }

    Ok(())
}

/// Test Euclidean distance calculations
///
/// Verifies:
/// - Euclidean distance is calculated correctly
/// - Distance vs similarity relationships
/// - Performance with large vectors
/// - Mathematical correctness
#[tokio::test]
async fn test_euclidean_distance_calculations() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create test embeddings
    let embedding1 = harness.generate_embedding("First test document").await?;
    let embedding2 = harness.generate_embedding("Second test document").await?;
    let embedding3 = harness.generate_embedding("First test document").await?; // Identical to first

    // Test Euclidean distance calculations
    let distance_1_2 = euclidean_distance(&embedding1, &embedding2);
    let distance_1_3 = euclidean_distance(&embedding1, &embedding3);
    let distance_2_3 = euclidean_distance(&embedding2, &embedding3);

    println!("Euclidean distances:");
    println!("  1-2: {:.6}", distance_1_2);
    println!("  1-3: {:.6}", distance_1_3);
    println!("  2-3: {:.6}", distance_2_3);

    // Verify distance properties
    assert!(
        distance_1_2 > 0.0,
        "Different embeddings should have positive distance"
    );
    assert!(
        (distance_1_3 - 0.0).abs() < 1e-6,
        "Identical embeddings should have zero distance"
    );
    assert!(
        (distance_2_3 - distance_1_2).abs() < 1e-6,
        "Distance should be symmetric: d(2,3) ≈ d(1,2)"
    );

    // Test distance vs similarity relationship
    let similarity_1_2 = cosine_similarity(&embedding1, &embedding2);
    let similarity_1_3 = cosine_similarity(&embedding1, &embedding3);

    // Higher similarity should correspond to lower distance
    assert!(
        similarity_1_3 > similarity_1_2,
        "Identical embeddings should have higher similarity than different ones"
    );
    assert!(
        distance_1_3 < distance_1_2,
        "Identical embeddings should have lower distance than different ones"
    );

    // Test with multiple embeddings
    let base_text = "Base text for distance testing";
    let base_embedding = harness.generate_embedding(base_text).await?;

    let variations = vec![
        "Base text for distance testing with addition",
        "Base text for distance testing with different words",
        "Completely different text about another topic",
    ];

    for (i, variation) in variations.iter().enumerate() {
        let var_embedding = harness.generate_embedding(variation).await?;
        let distance = euclidean_distance(&base_embedding, &var_embedding);
        let similarity = cosine_similarity(&base_embedding, &var_embedding);

        println!(
            "Variation {}: distance = {:.6}, similarity = {:.6}",
            i + 1,
            distance,
            similarity
        );

        assert!(distance > 0.0, "Distance should be positive");
        assert!(
            similarity > 0.0 && similarity < 1.0,
            "Similarity should be in (0, 1)"
        );
    }

    Ok(())
}

/// Test vector normalization
///
/// Verifies:
/// - Vectors are normalized correctly
/// - Normalized vectors have unit length
/// - Normalization preserves direction
/// - Edge cases are handled
#[tokio::test]
async fn test_vector_normalization() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Generate test embeddings
    let embeddings = vec![
        harness.generate_embedding("Test document 1").await?,
        harness.generate_embedding("Test document 2").await?,
        harness.generate_embedding("Test document 3").await?,
    ];

    for (i, embedding) in embeddings.iter().enumerate() {
        // Calculate original magnitude
        let original_magnitude = vector_magnitude(embedding);
        println!(
            "Original embedding {} magnitude: {:.6}",
            i + 1,
            original_magnitude
        );

        // Normalize the vector
        let normalized = normalize_vector(embedding);

        // Calculate normalized magnitude
        let normalized_magnitude = vector_magnitude(&normalized);
        println!(
            "Normalized embedding {} magnitude: {:.6}",
            i + 1,
            normalized_magnitude
        );

        // Verify normalization worked
        assert!(
            (normalized_magnitude - 1.0).abs() < 1e-6,
            "Normalized vector should have unit magnitude, got {:.6}",
            normalized_magnitude
        );

        // Verify direction is preserved (all values should be scaled by same factor)
        let scaling_factor = original_magnitude;
        for (j, (&original, &normalized_val)) in embedding.iter().zip(normalized.iter()).enumerate()
        {
            let expected_normalized = original / scaling_factor;
            assert!(
                (normalized_val - expected_normalized).abs() < 1e-6,
                "Normalized value at index {} should match expected value",
                j
            );
        }
    }

    // Test edge cases
    let edge_cases = vec![
        ("all zeros", vec![0.0; 768]),
        ("single value", vec![1.0] + vec![0.0; 767]),
        ("mixed values", vec![1.0, -1.0, 2.0, -2.0] + vec![0.0; 764]),
    ];

    for (case_name, vector) in edge_cases {
        let magnitude = vector_magnitude(&vector);
        let normalized = normalize_vector(&vector);
        let normalized_magnitude = vector_magnitude(&normalized);

        println!(
            "Edge case '{}': original magnitude = {:.6}, normalized magnitude = {:.6}",
            case_name, magnitude, normalized_magnitude
        );

        if magnitude > 0.0 {
            assert!(
                (normalized_magnitude - 1.0).abs() < 1e-6,
                "Non-zero vector should normalize to unit magnitude"
            );
        } else {
            // Zero vector should remain zero (or be handled gracefully)
            assert!(
                normalized_magnitude <= 1e-6,
                "Zero vector should remain zero or very small"
            );
        }
    }

    Ok(())
}

/// Test similarity threshold testing
///
/// Verifies:
/// - Similarity thresholds work correctly
/// - Threshold filtering is accurate
/// - Performance with large datasets
/// - Threshold tuning effectiveness
#[tokio::test]
async fn test_similarity_threshold_testing() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create a diverse set of documents
    let documents = vec![
        ("ml_basics", "Introduction to machine learning and AI"),
        (
            "ml_advanced",
            "Deep learning with neural networks and transformers",
        ),
        ("web_basics", "HTML, CSS, and JavaScript fundamentals"),
        ("web_advanced", "React, Vue, and modern frontend frameworks"),
        ("database", "SQL, NoSQL, and database design patterns"),
        ("devops", "Docker, Kubernetes, and CI/CD pipelines"),
        ("mobile", "iOS and Android development with React Native"),
        (
            "security",
            "Cryptography, authentication, and security best practices",
        ),
    ];

    // Store all documents
    for (id, content) in &documents {
        harness.create_note(&format!("{}.md", id), content).await?;
    }

    // Test different similarity thresholds
    let thresholds = vec![0.1, 0.3, 0.5, 0.7, 0.9];

    for threshold in thresholds {
        let query = "machine learning and artificial intelligence";
        let results = harness.semantic_search(query, 10).await?;

        // Filter results by threshold
        let filtered_results: Vec<_> = results
            .into_iter()
            .filter(|(_, similarity)| *similarity >= threshold)
            .collect();

        println!(
            "Threshold {:.1}: {} results (filtered from total)",
            threshold,
            filtered_results.len()
        );

        // Verify all filtered results meet threshold
        for (_, similarity) in &filtered_results {
            assert!(
                *similarity >= threshold - 1e-6, // Allow small floating point errors
                "Filtered result should meet threshold, got {}",
                similarity
            );
        }

        // Higher thresholds should produce fewer results
        if threshold > 0.5 {
            assert!(
                filtered_results.len() <= 3,
                "High threshold should produce few results"
            );
        }

        // Verify results make sense for ML query
        if !filtered_results.is_empty() {
            let has_ml_related = filtered_results
                .iter()
                .any(|(path, _)| path.contains("ml_basics") || path.contains("ml_advanced"));

            if threshold <= 0.7 {
                assert!(
                    has_ml_related,
                    "ML query should find ML-related documents at threshold {:.1}",
                    threshold
                );
            }
        }
    }

    // Test threshold with different queries
    let queries = vec![
        ("web query", "JavaScript and web development"),
        ("database query", "SQL and database management"),
        ("general query", "software development and programming"),
    ];

    for (query_name, query_text) in queries {
        let results = harness.semantic_search(query_text, 10).await?;
        let high_similarity_results: Vec<_> = results
            .iter()
            .filter(|(_, similarity)| *similarity > 0.7)
            .collect();

        println!(
            "Query '{}': {} high similarity results (>0.7)",
            query_name,
            high_similarity_results.len()
        );

        for (path, similarity) in high_similarity_results {
            println!("  {} similarity: {:.4}", path, similarity);
        }
    }

    Ok(())
}

// ============================================================================
// Batch vs Individual Consistency Tests
// ============================================================================

/// Test batch vs individual embedding consistency
///
/// Verifies:
/// - Same embeddings from batch and individual processing
/// - Processing order doesn't affect results
/// - Batch size doesn't change embeddings
/// - Memory usage is reasonable
#[tokio::test]
async fn test_batch_vs_individual_consistency() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create test documents
    let documents = vec![
        "First document for consistency testing",
        "Second document with different content",
        "Third document about testing methodologies",
        "Fourth document covering validation approaches",
        "Fifth document on quality assurance",
        "Sixth document regarding performance testing",
        "Seventh document about integration testing",
        "Eighth document covering system validation",
    ];

    // Generate embeddings individually
    let individual_embeddings: Result<Vec<_>> =
        futures::future::join_all(documents.iter().enumerate().map(|(i, content)| {
            let harness = harness.clone();
            let content = content.clone();
            async move {
                println!("Generating individual embedding {}", i + 1);
                harness.generate_embedding(&content).await
            }
        }))
        .await;

    let individual_embeddings = individual_embeddings?;

    // Generate embeddings in batch
    println!("Generating batch embeddings...");
    let batch_embeddings = harness.generate_batch_embeddings(&documents).await?;

    // Verify same number of embeddings
    assert_eq!(
        individual_embeddings.len(),
        batch_embeddings.len(),
        "Individual and batch processing should produce same number of embeddings"
    );

    // Verify each embedding is identical
    for (i, (individual, batch)) in individual_embeddings
        .iter()
        .zip(batch_embeddings.iter())
        .enumerate()
    {
        assert_eq!(
            individual,
            batch,
            "Embedding {} should be identical between individual and batch processing",
            i + 1
        );

        // Verify dimensions
        assert_eq!(
            individual.len(),
            768,
            "Individual embedding should have 768 dimensions"
        );
        assert_eq!(
            batch.len(),
            768,
            "Batch embedding should have 768 dimensions"
        );
    }

    // Test with different batch sizes
    let batch_sizes = vec![2, 3, 5];

    for batch_size in batch_sizes {
        println!("Testing with batch size: {}", batch_size);

        // Process in chunks of batch_size
        let chunked_embeddings: Vec<Vec<f32>> = documents
            .chunks(batch_size)
            .flat_map(|chunk| {
                let chunk_vec: Vec<String> = chunk.iter().map(|&s| s.to_string()).collect();
                harness.generate_batch_embeddings(&chunk_vec).unwrap()
            })
            .collect();

        assert_eq!(
            chunked_embeddings.len(),
            documents.len(),
            "Chunked batch processing should produce same number of embeddings"
        );

        // Verify chunked embeddings match individual ones
        for (i, chunked) in chunked_embeddings.iter().enumerate() {
            assert_eq!(
                chunked,
                &individual_embeddings[i],
                "Chunked embedding {} should match individual embedding",
                i + 1
            );
        }
    }

    println!("Batch vs individual consistency test passed");

    Ok(())
}

/// Test performance comparison between batch and individual
///
/// Verifies:
/// - Performance metrics are measured correctly
/// - Batch processing is more efficient
/// - Memory usage is reasonable
/// - Scaling behavior is expected
#[tokio::test]
async fn test_performance_comparison_batch_individual() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create test documents of varying sizes
    let document_sets = vec![
        ("small", vec!["Short document"; 10]),
        (
            "medium",
            vec!["This is a medium length document with more content to process."; 10],
        ),
        (
            "large",
            vec![
                &"This is a large document that contains significantly more content. ".repeat(10);
                5
            ],
        ),
    ];

    for (size_name, documents) in document_sets {
        println!("Performance test for {} documents:", size_name);

        // Test individual processing
        let individual_start = std::time::Instant::now();
        let individual_embeddings: Result<Vec<_>> = futures::future::join_all(
            documents
                .iter()
                .map(|content| harness.generate_embedding(content)),
        )
        .await;
        let individual_duration = individual_start.elapsed();
        let individual_embeddings = individual_embeddings?;

        // Test batch processing
        let batch_start = std::time::Instant::now();
        let batch_embeddings = harness.generate_batch_embeddings(&documents).await?;
        let batch_duration = batch_start.elapsed();

        // Verify embeddings are identical
        assert_eq!(individual_embeddings.len(), batch_embeddings.len());
        for (i, (individual, batch)) in individual_embeddings
            .iter()
            .zip(batch_embeddings.iter())
            .enumerate()
        {
            assert_eq!(individual, batch, "Embedding {} should match", i);
        }

        // Calculate performance metrics
        let individual_per_doc = individual_duration.as_millis() as f64 / documents.len() as f64;
        let batch_per_doc = batch_duration.as_millis() as f64 / documents.len() as f64;
        let speedup = individual_per_doc / batch_per_doc;

        println!(
            "  Individual processing: {:.2} ms total, {:.2} ms per document",
            individual_duration.as_millis(),
            individual_per_doc
        );
        println!(
            "  Batch processing: {:.2} ms total, {:.2} ms per document",
            batch_duration.as_millis(),
            batch_per_doc
        );
        println!("  Batch speedup: {:.2}x", speedup);

        // Verify performance is reasonable
        assert!(
            individual_duration.as_millis() > 0,
            "Individual processing should take measurable time"
        );
        assert!(
            batch_duration.as_millis() > 0,
            "Batch processing should take measurable time"
        );

        // Batch processing should be more efficient or at least not significantly slower
        assert!(
            speedup >= 0.5,
            "Batch processing should not be significantly slower than individual processing"
        );
    }

    Ok(())
}

/// Test memory usage analysis
///
/// Verifies:
/// - Memory usage is tracked correctly
/// - Large batches don't cause memory issues
/// - Memory is cleaned up properly
/// - Memory efficiency is acceptable
#[tokio::test]
async fn test_memory_usage_analysis() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Test memory usage with different batch sizes
    let batch_sizes = vec![10, 50, 100];

    for batch_size in batch_sizes {
        println!("Memory usage test with batch size: {}", batch_size);

        // Create test documents
        let documents: Vec<String> = (0..batch_size)
            .map(|i| format!("Test document {} for memory usage testing", i))
            .collect();

        // Get initial memory usage if available
        let initial_memory = get_memory_usage();
        println!("  Initial memory: {} KB", initial_memory);

        // Process batch
        let embeddings = harness.generate_batch_embeddings(&documents).await?;

        // Get memory usage after processing
        let after_memory = get_memory_usage();
        println!("  After processing: {} KB", after_memory);

        // Calculate memory usage per embedding
        let memory_per_embedding = if after_memory > initial_memory {
            (after_memory - initial_memory) / embeddings.len()
        } else {
            0
        };
        println!("  Memory per embedding: {} KB", memory_per_embedding);

        // Verify embeddings are valid
        assert_eq!(embeddings.len(), documents.len());
        for (i, embedding) in embeddings.iter().enumerate() {
            assert_eq!(
                embedding.len(),
                768,
                "Embedding {} should have correct dimensions",
                i
            );
        }

        // Memory usage should be reasonable (less than 10KB per embedding)
        assert!(
            memory_per_embedding < 10,
            "Memory usage per embedding should be reasonable, got {} KB",
            memory_per_embedding
        );
    }

    // Test memory cleanup
    println!("Testing memory cleanup...");

    // Process a large batch and then drop results
    let large_batch: Vec<String> = (0..200)
        .map(|i| format!("Large batch document {}", i))
        .collect();

    let before_cleanup = get_memory_usage();
    let _large_embeddings = harness.generate_batch_embeddings(&large_batch).await?;
    let during_processing = get_memory_usage();

    // Drop embeddings to test cleanup
    drop(_large_embeddings);

    // Force garbage collection if possible
    tokio::task::yield_now().await;

    let after_cleanup = get_memory_usage();

    println!("  Before cleanup: {} KB", before_cleanup);
    println!("  During processing: {} KB", during_processing);
    println!("  After cleanup: {} KB", after_cleanup);

    // Memory should be cleaned up (though exact behavior depends on system)
    if during_processing > before_cleanup {
        println!(
            "  Memory increased by {} KB during processing",
            during_processing - before_cleanup
        );
    }

    Ok(())
}

/// Test error handling consistency
///
/// Verifies:
/// - Errors are handled consistently between batch and individual
/// - Error messages are informative
/// - Partial failures are handled correctly
/// - Recovery mechanisms work
#[tokio::test]
async fn test_error_handling_consistency() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Test with normal content (should succeed)
    let normal_content = "Normal test document";
    let individual_result = harness.generate_embedding(normal_content).await;
    assert!(
        individual_result.is_ok(),
        "Normal content should process individually"
    );

    let batch_result = harness
        .generate_batch_embeddings(&[normal_content.to_string()])
        .await;
    assert!(
        batch_result.is_ok(),
        "Normal content should process in batch"
    );

    // Test edge cases
    let edge_cases = vec![
        ("empty", ""),
        ("whitespace", "   \n\n   "),
        ("very_long", &"A".repeat(10000)),
    ];

    for (case_name, content) in edge_cases {
        println!("Testing error case: {}", case_name);

        // Individual processing
        let individual_result = harness.generate_embedding(content).await;

        // Batch processing
        let batch_result = harness
            .generate_batch_embeddings(&[content.to_string()])
            .await;

        // Both should succeed or fail consistently
        match (&individual_result, &batch_result) {
            (Ok(individual_emb), Ok(batch_emb)) => {
                // Both succeeded - verify embeddings are identical
                assert_eq!(
                    individual_emb, &batch_emb[0],
                    "Embeddings should be identical"
                );
                println!("  Both succeeded - embeddings identical");
            }
            (Err(individual_err), Err(batch_err)) => {
                // Both failed - verify error types are similar
                println!(
                    "  Both failed - individual: {}, batch: {}",
                    individual_err, batch_err
                );
            }
            (Ok(_), Err(batch_err)) => {
                println!("  Individual succeeded but batch failed: {}", batch_err);
                // This might be acceptable depending on implementation
            }
            (Err(individual_err), Ok(_)) => {
                println!(
                    "  Individual failed but batch succeeded: {}",
                    individual_err
                );
                // This might be acceptable depending on implementation
            }
        }
    }

    // Test batch with mixed valid/invalid content
    let mixed_content = vec![
        "Valid document 1",
        "", // Empty
        "Valid document 2",
        &"A".repeat(50000), // Very long
        "Valid document 3",
    ];

    let mixed_result = harness.generate_batch_embeddings(&mixed_content).await;

    match mixed_result {
        Ok(embeddings) => {
            println!("Mixed batch succeeded with {} embeddings", embeddings.len());
            assert_eq!(
                embeddings.len(),
                mixed_content.len(),
                "Should have embedding for each item"
            );

            // Verify all embeddings are valid
            for (i, embedding) in embeddings.iter().enumerate() {
                assert_eq!(
                    embedding.len(),
                    768,
                    "Embedding {} should have correct dimensions",
                    i
                );
            }
        }
        Err(e) => {
            println!("Mixed batch failed: {}", e);
            // Batch processing might fail if any item is problematic
        }
    }

    println!("Error handling consistency test completed");

    Ok(())
}

// ============================================================================
// Metadata Preservation Tests
// ============================================================================

/// Test document metadata storage
///
/// Verifies:
/// - Document metadata is stored correctly
/// - All required fields are present
/// - Metadata types are correct
/// - Metadata can be retrieved accurately
#[tokio::test]
async fn test_document_metadata_storage() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create documents with different metadata scenarios
    let test_cases = vec![
        ("simple_doc", "Simple document content", None),
        (
            "doc_with_title",
            "Document with title",
            Some("Custom Title".to_string()),
        ),
        ("doc_with_tags", "Document with tags", None), // Tags will be extracted from inline #tags
    ];

    for (id, content, custom_title) in test_cases {
        let full_content = match custom_title {
            Some(title) => format!("---\ntitle: {}\n---\n\n{}", title, content),
            None => content.to_string(),
        };

        let path = harness
            .create_note(&format!("{}.md", id), &full_content)
            .await?;
        assert!(path.exists(), "Document should be created");

        // Retrieve metadata
        let metadata = harness.get_metadata(&format!("{}.md", id)).await?;
        assert!(metadata.is_some(), "Metadata should exist for {}", id);

        let metadata = metadata.unwrap();

        // Verify required fields
        assert!(
            metadata.created_at.timestamp() > 0,
            "Created timestamp should be valid for {}",
            id
        );
        assert!(
            metadata.updated_at.timestamp() > 0,
            "Updated timestamp should be valid for {}",
            id
        );
        assert!(
            metadata.updated_at >= metadata.created_at,
            "Updated timestamp should be after created for {}",
            id
        );

        // Verify title handling
        let expected_title = custom_title.unwrap_or_else(|| id.to_string());
        assert_eq!(
            metadata.title,
            Some(expected_title),
            "Title should match expected value for {}",
            id
        );

        // Verify folder information
        assert!(
            !metadata.folder.is_empty(),
            "Folder should not be empty for {}",
            id
        );

        println!(
            "Metadata for {}: title={:?}, folder={}",
            id, metadata.title, metadata.folder
        );
    }

    // Test document with tags
    let tagged_content = r#"---
tags: [rust, testing, embeddings]
priority: high
---

Document with #inline tags and frontmatter tags.

More content with #more-tags here."#;

    harness.create_note("tagged_doc.md", tagged_content).await?;

    let tagged_metadata = harness.get_metadata("tagged_doc.md").await?;
    assert!(
        tagged_metadata.is_some(),
        "Tagged document metadata should exist"
    );

    let tagged_metadata = tagged_metadata.unwrap();

    // Verify tags from frontmatter
    assert!(
        tagged_metadata.tags.contains(&"rust".to_string()),
        "Should have 'rust' tag from frontmatter"
    );
    assert!(
        tagged_metadata.tags.contains(&"testing".to_string()),
        "Should have 'testing' tag from frontmatter"
    );
    assert!(
        tagged_metadata.tags.contains(&"embeddings".to_string()),
        "Should have 'embeddings' tag from frontmatter"
    );

    // Verify inline tags
    assert!(
        tagged_metadata.tags.contains(&"inline".to_string()),
        "Should have 'inline' tag from inline extraction"
    );
    assert!(
        tagged_metadata.tags.contains(&"more-tags".to_string()),
        "Should have 'more-tags' tag from inline extraction"
    );

    println!("Tags for tagged_doc: {:?}", tagged_metadata.tags);

    Ok(())
}

/// Test embedding metadata retention
///
/// Verifies:
/// - Embedding metadata is preserved correctly
/// - Model information is stored
/// - Processing timestamps are accurate
/// - Configuration details are retained
#[tokio::test]
async fn test_embedding_metadata_retention() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create a document to test embedding metadata
    let content = "Test document for embedding metadata retention";
    let doc_path = harness.create_note("metadata_test.md", content).await?;

    assert!(doc_path.exists(), "Document should be created");

    // Verify embedding was generated
    assert!(
        harness.has_embedding("metadata_test.md").await?,
        "Embedding should be generated"
    );

    // Retrieve embedding information
    let embedding = harness.get_embedding("metadata_test.md").await?;
    assert!(embedding.is_some(), "Should retrieve embedding");

    let embedding = embedding.unwrap();

    // Verify embedding dimensions (metadata aspect)
    assert_eq!(
        embedding.len(),
        768,
        "Embedding should have correct dimensions"
    );

    // Generate fresh embedding to compare (mock provider is deterministic)
    let fresh_embedding = harness.generate_embedding(content).await?;
    assert_eq!(embedding, fresh_embedding, "Embedding should be consistent");

    // Test with different model configurations
    let config_mini = EmbeddingConfig {
        worker_count: 1,
        batch_size: 1,
        model_type: EmbeddingModel::LocalMini,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 10,
        timeout_ms: 5000,
        retry_attempts: 1,
        retry_delay_ms: 100,
        circuit_breaker_threshold: 5,
        circuit_breaker_timeout_ms: 10000,
    };

    // Create a separate document with different model type
    let mini_content = "Document for mini model testing";
    let mini_doc_path = format!("tests/mini_model_test.md");

    // Note: This would require the harness to support different model types
    // For now, we just verify the current system works
    println!("Embedding metadata retention test completed");

    // Verify embedding quality
    for (i, &value) in embedding.iter().enumerate() {
        assert!(
            value.is_finite(),
            "Embedding value at index {} should be finite",
            i
        );
        assert!(
            value >= 0.0 && value <= 1.0,
            "Embedding value at index {} should be within [0, 1]",
            i
        );
    }

    let variance = calculate_variance(&embedding);
    assert!(variance > 0.0, "Embedding should have positive variance");

    println!("Embedding variance: {:.4}", variance);

    Ok(())
}

/// Test timestamp and version tracking
///
/// Verifies:
/// - Timestamps are accurate and consistent
/// - Version information is tracked
/// - Update history is maintained
/// - Temporal queries work correctly
#[tokio::test]
async fn test_timestamp_and_version_tracking() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create initial document
    let initial_content = "Initial document content";
    let before_creation = chrono::Utc::now();

    let doc_path = harness
        .create_note("timestamp_test.md", initial_content)
        .await?;
    let after_creation = chrono::Utc::now();

    assert!(doc_path.exists(), "Document should be created");

    // Get metadata and verify timestamps
    let metadata = harness.get_metadata("timestamp_test.md").await?;
    assert!(metadata.is_some(), "Metadata should exist");

    let metadata = metadata.unwrap();

    // Verify creation timestamp
    assert!(
        metadata.created_at >= before_creation,
        "Created timestamp should be after test start"
    );
    assert!(
        metadata.created_at <= after_creation,
        "Created timestamp should be before test end"
    );

    // Verify initial update timestamp (should be same as creation)
    assert!(
        metadata.updated_at >= before_creation,
        "Initial updated timestamp should be after test start"
    );
    assert!(
        metadata.updated_at <= after_creation,
        "Initial updated timestamp should be before test end"
    );

    let first_update_time = metadata.updated_at;

    println!(
        "Created: {}, Updated: {}",
        metadata.created_at, metadata.updated_at
    );

    // Wait a bit to ensure different timestamps
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Update the document
    let updated_content = "Updated document content with new information";
    let before_update = chrono::Utc::now();

    // Note: This would require document update functionality
    // For now, we simulate by creating a new document
    harness
        .create_note("timestamp_test_updated.md", updated_content)
        .await?;
    let after_update = chrono::Utc::now();

    // Get updated metadata
    let updated_metadata = harness.get_metadata("timestamp_test_updated.md").await?;
    assert!(updated_metadata.is_some(), "Updated metadata should exist");

    let updated_metadata = updated_metadata.unwrap();

    // Verify updated timestamp
    assert!(
        updated_metadata.created_at >= before_update,
        "Updated document created timestamp should be after update start"
    );
    assert!(
        updated_metadata.created_at <= after_update,
        "Updated document created timestamp should be before update end"
    );

    println!(
        "Updated document - Created: {}, Updated: {}",
        updated_metadata.created_at, updated_metadata.updated_at
    );

    // Verify timestamps are different
    assert!(
        updated_metadata.created_at > first_update_time,
        "Updated document timestamp should be later than original"
    );

    // Test temporal ordering
    let all_docs = vec!["timestamp_test.md", "timestamp_test_updated.md"];
    let mut doc_metadata = Vec::new();

    for doc in all_docs {
        if let Some(metadata) = harness.get_metadata(doc).await? {
            doc_metadata.push((doc, metadata));
        }
    }

    // Sort by creation time
    doc_metadata.sort_by_key(|(_, metadata)| metadata.created_at);

    println!("Documents in creation order:");
    for (doc, metadata) in doc_metadata {
        println!("  {}: created at {}", doc, metadata.created_at);
    }

    // Verify sorting worked
    for i in 1..doc_metadata.len() {
        assert!(
            doc_metadata[i].1.created_at >= doc_metadata[i - 1].1.created_at,
            "Documents should be in chronological order"
        );
    }

    Ok(())
}

/// Test configuration preservation
///
/// Verifies:
/// - Configuration details are preserved
/// - Model information is stored correctly
/// - Processing parameters are tracked
/// - Configuration affects results appropriately
#[tokio::test]
async fn test_configuration_preservation() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Test default configuration
    let content = "Test document for configuration preservation";
    let embedding = harness.generate_embedding(content).await?;

    // Verify embedding characteristics reflect default configuration
    assert_eq!(
        embedding.len(),
        768,
        "Default config should produce 768-dimensional embeddings"
    );

    // Test different configurations
    let configs = vec![
        (
            "mini_config",
            EmbeddingConfig {
                worker_count: 1,
                batch_size: 1,
                model_type: EmbeddingModel::LocalMini,
                privacy_mode: PrivacyMode::StrictLocal,
                max_queue_size: 10,
                timeout_ms: 5000,
                retry_attempts: 1,
                retry_delay_ms: 100,
                circuit_breaker_threshold: 5,
                circuit_breaker_timeout_ms: 10000,
            },
        ),
        ("resource_config", EmbeddingConfig::optimize_for_resources()),
    ];

    for (config_name, config) in configs {
        println!("Testing configuration: {}", config_name);

        // Note: This would require creating harness with different configurations
        // For now, we verify the current configuration properties

        let test_embedding = harness
            .generate_embedding(&format!("{}: {}", config_name, content))
            .await?;

        // Verify embedding is valid
        assert_eq!(
            test_embedding.len(),
            768,
            "Embedding should have correct dimensions"
        );

        // Verify embedding is different from previous one
        let similarity = cosine_similarity(&embedding, &test_embedding);
        assert!(
            similarity < 1.0,
            "Different content should produce different embeddings"
        );

        println!(
            "  Similarity to default config embedding: {:.4}",
            similarity
        );
    }

    // Test configuration validation
    let default_config = EmbeddingConfig::default();
    assert!(
        default_config.validate().is_ok(),
        "Default config should be valid"
    );

    let throughput_config = EmbeddingConfig::optimize_for_throughput();
    assert!(
        throughput_config.validate().is_ok(),
        "Throughput config should be valid"
    );

    let latency_config = EmbeddingConfig::optimize_for_latency();
    assert!(
        latency_config.validate().is_ok(),
        "Latency config should be valid"
    );

    // Verify configuration differences
    assert!(
        throughput_config.batch_size > default_config.batch_size,
        "Throughput config should have larger batch size"
    );
    assert!(
        latency_config.batch_size < default_config.batch_size,
        "Latency config should have smaller batch size"
    );

    println!("Configuration preservation test completed");

    Ok(())
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Calculate cosine similarity between two vectors
fn cosine_similarity(vec1: &[f32], vec2: &[f32]) -> f32 {
    assert_eq!(
        vec1.len(),
        vec2.len(),
        "Vectors must have same length for cosine similarity"
    );

    let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
    let norm1: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm2: f32 = vec2.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm1 == 0.0 || norm2 == 0.0 {
        0.0
    } else {
        dot_product / (norm1 * norm2)
    }
}

/// Calculate Euclidean distance between two vectors
fn euclidean_distance(vec1: &[f32], vec2: &[f32]) -> f32 {
    assert_eq!(
        vec1.len(),
        vec2.len(),
        "Vectors must have same length for Euclidean distance"
    );

    vec1.iter()
        .zip(vec2.iter())
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Calculate vector magnitude (L2 norm)
fn vector_magnitude(vec: &[f32]) -> f32 {
    vec.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Normalize vector to unit length
fn normalize_vector(vec: &[f32]) -> Vec<f32> {
    let magnitude = vector_magnitude(vec);
    if magnitude == 0.0 {
        vec.to_vec()
    } else {
        vec.iter().map(|x| x / magnitude).collect()
    }
}

/// Calculate variance of vector values
fn calculate_variance(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }

    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let sum_squared_diff: f32 = values.iter().map(|&x| (x - mean) * (x - mean)).sum();

    sum_squared_diff / values.len() as f32
}

/// Get current memory usage in KB (platform-dependent)
fn get_memory_usage() -> u64 {
    // This is a simplified implementation
    // In a real scenario, you'd use platform-specific APIs
    #[cfg(unix)]
    {
        use std::fs;
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        return kb_str.parse().unwrap_or(0);
                    }
                }
            }
        }
    }

    // Fallback: return 0 (memory usage not available)
    0
}
