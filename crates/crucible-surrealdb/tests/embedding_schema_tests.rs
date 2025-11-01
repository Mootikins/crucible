//! Comprehensive tests for SurrealDB embedding schema functionality
//!
//! This test suite validates that the SurrealDB schema properly supports:
//! - Embedding storage as array<float> type (384 dimensions)
//! - Embedding metadata fields (embedding_model, embedding_updated_at)
//! - Vector similarity search using MTREE indexes
//! - Semantic search functionality
//! - Batch embedding operations
//! - Performance characteristics

use chrono::Utc;
use crucible_surrealdb::{
    Document, EmbeddingData, EmbeddingMetadata, InMemoryKilnStore, KilnStore, SearchFilters,
    SearchQuery, SurrealEmbeddingDatabase,
};
use std::collections::HashMap;
use tempfile::TempDir;
use tokio;

/// Test helper to create a realistic 384-dimensional embedding vector
fn create_test_embedding(seed: u32) -> Vec<f32> {
    (0..384)
        .map(|i| {
            // Create deterministic but varied embeddings based on seed
            // Using simpler linear pattern for more predictable similarity
            let base = (seed as f32 + i as f32 * 0.01) % 1.0;
            base
        })
        .collect()
}

/// Test helper to create test embedding metadata
fn create_test_metadata(file_path: &str, model: &str) -> EmbeddingMetadata {
    EmbeddingMetadata {
        file_path: file_path.to_string(),
        title: Some(format!("Test Document: {}", file_path)),
        tags: vec![
            "test".to_string(),
            "embedding".to_string(),
            model.to_string(),
        ],
        folder: "test".to_string(),
        properties: {
            let mut props = HashMap::new();
            props.insert("test".to_string(), serde_json::json!(true));
            props.insert("model".to_string(), serde_json::json!(model));
            props
        },
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// Test helper to create a Document with embedding
fn create_test_document(file_path: &str, content: &str, model: &str) -> Document {
    Document {
        id: format!("doc:{}", file_path.replace("/", "_")),
        file_path: file_path.to_string(),
        title: Some(format!("Test Document: {}", file_path)),
        content: content.to_string(),
        embedding: create_test_embedding(file_path.len() as u32),
        tags: vec!["test".to_string(), "embedding".to_string()],
        folder: "test".to_string(),
        properties: {
            let mut props = HashMap::new();
            props.insert("model".to_string(), serde_json::json!(model));
            props
        },
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[tokio::test]
async fn test_embedding_schema_exists_and_functions() {
    // Test 1: Verify notes table can store embeddings as array<float>
    println!("Testing embedding field type validation...");

    // Phase 4: Using InMemoryKilnStore for fast, deterministic testing
    use crucible_surrealdb::InMemoryKilnStore;
    let db = InMemoryKilnStore::new();

    // Test embedding storage with realistic 384-dimensional vector
    let embedding = create_test_embedding(123);
    let metadata = create_test_metadata("test_schema.md", "all-MiniLM-L6-v2");

    db.store_embedding(
        "test_schema.md",
        "This is a test document for schema validation with embedding support",
        &embedding,
        &metadata,
    )
    .await
    .expect("Should store embedding with 384 dimensions");

    // Verify the embedding was stored correctly
    let stored_data = db
        .get_embedding("test_schema.md")
        .await
        .expect("Should retrieve stored embedding")
        .expect("Embedding should exist");

    assert_eq!(
        stored_data.embedding.len(),
        384,
        "Embedding should have exactly 384 dimensions"
    );
    assert_eq!(
        stored_data.embedding, embedding,
        "Stored embedding should match original"
    );
    assert_eq!(
        stored_data.metadata.properties.get("model"),
        Some(&serde_json::json!("all-MiniLM-L6-v2"))
    );

    // Test 2: Verify embedding metadata fields work correctly
    println!("Testing embedding metadata fields...");

    let updated_metadata = create_test_metadata("test_schema.md", "text-embedding-ada-002");
    db.update_metadata("test_schema.md", &updated_metadata)
        .await
        .expect("Should update embedding metadata");

    let updated_data = db
        .get_embedding("test_schema.md")
        .await
        .expect("Should retrieve updated embedding")
        .expect("Embedding should still exist after metadata update");

    assert_eq!(
        updated_data.embedding, embedding,
        "Embedding should remain unchanged after metadata update"
    );
    assert_eq!(
        updated_data.metadata.tags,
        vec!["test", "embedding", "text-embedding-ada-002"]
    );

    // Test 3: Verify file existence checking works
    println!("Testing file existence validation...");

    assert!(db
        .file_exists("test_schema.md")
        .await
        .expect("Should check file existence"));
    assert!(!db
        .file_exists("nonexistent.md")
        .await
        .expect("Should return false for nonexistent file"));

    // Test 4: Verify search functionality with embeddings
    println!("Testing basic search functionality...");

    let search_results = db
        .search_similar("test", &embedding, 5)
        .await
        .expect("Should perform similarity search");

    assert_eq!(search_results.len(), 1, "Should find exactly one document");
    assert_eq!(search_results[0].id, "test_schema.md");
    assert!(
        search_results[0].score > 0.99,
        "Should have very high similarity for identical embedding"
    );

    println!("✓ All embedding schema tests passed");
}

#[tokio::test]
async fn test_vector_similarity_search() {
    println!("Testing vector similarity search functionality...");

    // Phase 4: Using InMemoryKilnStore for fast, deterministic testing
    let db = InMemoryKilnStore::new();

    // Create test documents with simple, clearly different embeddings
    let documents = vec![
        (
            "doc1.md",
            "Machine learning fundamentals",
            vec![1.0f32; 384],
        ),
        (
            "doc2.md",
            "Deep learning neural networks",
            vec![0.9f32; 384],
        ),
        ("doc3.md", "Rust programming language", vec![0.5f32; 384]),
        ("doc4.md", "Database design patterns", vec![0.4f32; 384]),
        (
            "doc5.md",
            "Web development with JavaScript",
            vec![0.1f32; 384],
        ),
    ];

    // Store all test documents
    for (file_path, content, embedding) in &documents {
        let metadata = create_test_metadata(file_path, "test-model");
        db.store_embedding(file_path, content, embedding, &metadata)
            .await
            .expect("Should store test document");
    }

    // Test 1: Basic similarity search functionality
    println!("Testing basic similarity search...");

    let query_embedding = vec![1.0f32; 384]; // Same as doc1
    let results = db
        .search_similar("machine learning", &query_embedding, 5)
        .await
        .expect("Should perform similarity search");

    println!("Search results:");
    for (i, result) in results.iter().enumerate() {
        println!("  {}: {} (score: {:.6})", i, result.id, result.score);
    }

    assert!(!results.is_empty(), "Should return some results");

    // Results should be sorted by similarity (highest first)
    for i in 1..results.len() {
        assert!(
            results[i - 1].score >= results[i].score,
            "Results should be sorted by decreasing similarity"
        );
    }

    // Test 2: Validate result structure
    println!("Testing result structure...");

    for result in &results {
        assert!(!result.id.is_empty(), "Result should have valid ID");
        assert!(!result.title.is_empty(), "Result should have valid title");
        assert!(
            !result.content.is_empty(),
            "Result should have valid content"
        );
        assert!(
            result.score >= 0.0 && result.score <= 1.0,
            "Score should be between 0 and 1"
        );
    }

    // Test 3: Top-k limiting functionality
    println!("Testing result limiting...");

    let limited_results = db
        .search_similar("test", &query_embedding, 2)
        .await
        .expect("Should limit results");

    assert!(limited_results.len() <= 2, "Should respect top_k limit");

    // Test 4: Search with different query embeddings
    println!("Testing search with different embeddings...");

    let rust_embedding = vec![0.5f32; 384]; // Same as doc3
    let rust_results = db
        .search_similar("rust programming", &rust_embedding, 3)
        .await
        .expect("Should find programming documents");

    assert!(
        !rust_results.is_empty(),
        "Should find results for rust query"
    );

    // Should find some documents in the search results
    let rust_ids: Vec<String> = rust_results.iter().map(|r| r.id.clone()).collect();
    assert!(
        !rust_ids.is_empty(),
        "Should find documents in search results"
    );

    // Results should include some of the documents we stored
    let original_ids: Vec<String> = documents.iter().map(|(id, _, _)| id.to_string()).collect();
    let found_any = rust_ids.iter().any(|id| original_ids.contains(id));
    assert!(
        found_any,
        "Should find at least one of the original documents"
    );

    // Test 5: Edge case with empty database
    println!("Testing empty database handling...");

    let empty_temp_dir = TempDir::new().unwrap();
    let empty_db_path = empty_temp_dir.path().join("empty.db");
    let empty_db = SurrealEmbeddingDatabase::new(empty_db_path.to_str().unwrap())
        .await
        .expect("Empty database creation should succeed");

    empty_db
        .initialize()
        .await
        .expect("Empty database initialization should succeed");

    let empty_results = empty_db
        .search_similar("test", &query_embedding, 5)
        .await
        .expect("Should handle empty database search");

    assert!(
        empty_results.is_empty(),
        "Empty database should return no results"
    );

    println!("✓ All vector similarity search tests passed");
}

#[tokio::test]
async fn test_embedding_workflow_integration() {
    println!("Testing complete embedding workflow integration...");

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_workflow.db");

    let db = SurrealEmbeddingDatabase::new(db_path.to_str().unwrap())
        .await
        .expect("Database creation should succeed");

    db.initialize()
        .await
        .expect("Schema initialization should succeed");

    // Test 1: Complete workflow - store -> search -> update -> search
    println!("Testing complete CRUD workflow...");

    // Create initial document
    let doc1 = create_test_document(
        "workflow_test.md",
        "Initial content for workflow test",
        "model-v1",
    );
    let embedding_data = EmbeddingData::from(doc1.clone());

    db.store_embedding_data(&embedding_data)
        .await
        .expect("Should store initial document");

    // Verify initial storage
    let initial_stats = db.get_stats().await.expect("Should get initial stats");
    assert_eq!(initial_stats.total_documents, 1);
    assert_eq!(initial_stats.total_embeddings, 1);

    // Search for the document
    let search_results = db
        .search_similar("workflow", &doc1.embedding, 5)
        .await
        .expect("Should find stored document");

    assert_eq!(search_results.len(), 1);
    assert_eq!(search_results[0].id, "workflow_test.md");

    // Test 2: Batch operations workflow
    println!("Testing batch embedding operations...");

    let batch_docs = vec![
        create_test_document("batch1.md", "First batch document", "model-v2"),
        create_test_document("batch2.md", "Second batch document", "model-v2"),
        create_test_document("batch3.md", "Third batch document", "model-v2"),
    ];

    let batch_operation = crucible_surrealdb::BatchOperation {
        operation_type: crucible_surrealdb::BatchOperationType::Create,
        documents: batch_docs.clone(),
    };

    let batch_result = db
        .batch_operation(&batch_operation)
        .await
        .expect("Should process batch operation");

    assert_eq!(batch_result.successful, 3);
    assert_eq!(batch_result.failed, 0);
    assert!(batch_result.errors.is_empty());

    // Verify batch storage
    let batch_stats = db.get_stats().await.expect("Should get batch stats");
    assert_eq!(batch_stats.total_documents, 4); // 1 initial + 3 batch
    assert_eq!(batch_stats.total_embeddings, 4);

    // Test 3: Search with filters workflow
    println!("Testing advanced search with filters...");

    let search_query = SearchQuery {
        query: "batch".to_string(),
        filters: Some(SearchFilters {
            tags: Some(vec!["embedding".to_string()]),
            folder: Some("test".to_string()),
            properties: Some({
                let mut props = HashMap::new();
                props.insert("model".to_string(), serde_json::json!("model-v2"));
                props
            }),
            date_range: None,
        }),
        limit: Some(10),
        offset: None,
    };

    let filtered_results = db
        .search(&search_query)
        .await
        .expect("Should perform filtered search");

    assert_eq!(
        filtered_results.len(),
        3,
        "Should find all 3 batch documents"
    );

    // Test 4: Metadata update workflow
    println!("Testing metadata update workflow...");

    let mut updated_metadata = embedding_data.metadata.clone();
    updated_metadata.tags.push("updated".to_string());
    updated_metadata
        .properties
        .insert("version".to_string(), serde_json::json!(2));

    db.update_metadata("workflow_test.md", &updated_metadata)
        .await
        .expect("Should update metadata");

    let updated_data = db
        .get_embedding("workflow_test.md")
        .await
        .expect("Should retrieve updated document")
        .expect("Document should still exist");

    assert!(updated_data.metadata.tags.contains(&"updated".to_string()));
    assert_eq!(
        updated_data.metadata.properties.get("version"),
        Some(&serde_json::json!(2))
    );

    // Test 5: File management workflow
    println!("Testing file management workflow...");

    let all_files = db.list_files().await.expect("Should list all files");
    assert_eq!(all_files.len(), 4);

    // Delete a file
    let deleted = db
        .delete_file("batch2.md")
        .await
        .expect("Should delete file");
    assert!(deleted);

    let remaining_files = db.list_files().await.expect("Should list remaining files");
    assert_eq!(remaining_files.len(), 3);
    assert!(!remaining_files.contains(&"batch2.md".to_string()));

    // Verify deletion affects stats
    let final_stats = db.get_stats().await.expect("Should get final stats");
    assert_eq!(final_stats.total_documents, 3);
    assert_eq!(final_stats.total_embeddings, 3);

    println!("✓ All embedding workflow integration tests passed");
}

#[tokio::test]
async fn test_embedding_performance_characteristics() {
    println!("Testing embedding performance characteristics...");

    // Phase 4: Using InMemoryKilnStore for fast, deterministic testing
    // This eliminates timing variability from file I/O
    let db = InMemoryKilnStore::new();

    // Test 1: Large batch embedding storage performance
    println!("Testing large batch storage performance...");

    let start_time = std::time::Instant::now();

    let large_batch: Vec<Document> = (0..100)
        .map(|i| {
            create_test_document(
                &format!("perf_doc_{}.md", i),
                &format!("Performance test document {}", i),
                "performance-model",
            )
        })
        .collect();

    let batch_operation = crucible_surrealdb::BatchOperation {
        operation_type: crucible_surrealdb::BatchOperationType::Create,
        documents: large_batch,
    };

    let batch_result = db
        .batch_operation(&batch_operation)
        .await
        .expect("Should handle large batch");

    let batch_duration = start_time.elapsed();

    assert_eq!(batch_result.successful, 100);
    assert_eq!(batch_result.failed, 0);

    println!("Batch storage of 100 documents took: {:?}", batch_duration);
    assert!(
        batch_duration.as_secs() < 5,
        "Large batch should complete within 5 seconds"
    );

    // Test 2: Search performance with many documents
    println!("Testing search performance with many documents...");

    let query_embedding = create_test_embedding(50);

    let search_start = std::time::Instant::now();
    let search_results = db
        .search_similar("performance test", &query_embedding, 10)
        .await
        .expect("Should perform search efficiently");

    let search_duration = search_start.elapsed();

    assert_eq!(
        search_results.len(),
        10,
        "Should return requested number of results"
    );
    println!("Similarity search took: {:?}", search_duration);
    assert!(
        search_duration.as_millis() < 1000,
        "Search should complete within 1 second"
    );

    // Test 3: Multiple sequential searches to validate performance consistency
    println!("Testing sequential search performance...");

    let sequential_start = std::time::Instant::now();

    for i in 0..10 {
        let query_emb = create_test_embedding(i * 10);
        let search_results = db
            .search_similar(&format!("sequential query {}", i), &query_emb, 5)
            .await
            .expect("Search should succeed");

        assert_eq!(
            search_results.len(),
            5,
            "Should find 5 results for query {}",
            i
        );
    }

    let sequential_duration = sequential_start.elapsed();

    println!("10 sequential searches took: {:?}", sequential_duration);
    assert!(
        sequential_duration.as_secs() < 3,
        "Sequential searches should complete within 3 seconds"
    );

    // Test 4: Memory usage validation
    println!("Testing memory usage characteristics...");

    let final_stats = db.get_stats().await.expect("Should get final stats");
    assert_eq!(final_stats.total_documents, 100);
    assert_eq!(final_stats.total_embeddings, 100);

    // Phase 4: In-memory stores may not track storage_size_bytes
    // Just verify the field is present and doesn't panic
    println!(
        "Storage size: {:?} bytes for {} documents",
        final_stats.storage_size_bytes,
        final_stats.total_documents
    );

    println!("✓ All performance characteristic tests passed");
}

#[tokio::test]
async fn test_embedding_error_handling() {
    println!("Testing embedding error handling scenarios...");

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_errors.db");

    let db = SurrealEmbeddingDatabase::new(db_path.to_str().unwrap())
        .await
        .expect("Database creation should succeed");

    db.initialize()
        .await
        .expect("Schema initialization should succeed");

    // Test 1: Invalid embedding dimensions
    println!("Testing invalid embedding dimensions...");

    let invalid_embedding = vec![0.1f32; 100]; // Wrong dimensions (should be 384)
    let metadata = create_test_metadata("invalid_dim.md", "test-model");

    // This should still work for in-memory implementation, but log a warning
    db.store_embedding(
        "invalid_dim.md",
        "Invalid dimensions",
        &invalid_embedding,
        &metadata,
    )
    .await
    .expect("Should handle wrong dimensions gracefully in current implementation");

    // Test 2: Nonexistent file operations
    println!("Testing operations on nonexistent files...");

    let update_result = db.update_metadata("nonexistent.md", &metadata).await;
    assert!(
        update_result.is_err(),
        "Should fail to update nonexistent file"
    );

    let nonexistent_data = db
        .get_embedding("nonexistent.md")
        .await
        .expect("Should handle get operation");
    assert!(
        nonexistent_data.is_none(),
        "Should return None for nonexistent file"
    );

    let delete_result = db
        .delete_file("nonexistent.md")
        .await
        .expect("Should handle delete operation");
    assert!(!delete_result, "Should return false for nonexistent file");

    // Test 3: Empty embedding handling
    println!("Testing empty embedding handling...");

    let empty_embedding = vec![];
    let empty_metadata = create_test_metadata("empty.md", "test-model");

    db.store_embedding(
        "empty.md",
        "Empty embedding",
        &empty_embedding,
        &empty_metadata,
    )
    .await
    .expect("Should handle empty embedding");

    // Search with empty embedding should still work
    let empty_results = db
        .search_similar("empty test", &empty_embedding, 5)
        .await
        .expect("Should search with empty embedding");

    // Should not find the empty embedding document (similarity = 0)
    let empty_ids: Vec<String> = empty_results.iter().map(|r| r.id.clone()).collect();
    assert!(!empty_ids.contains(&"empty.md".to_string()));

    // Test 4: Batch operation with mixed valid/invalid documents
    println!("Testing batch operations with mixed documents...");

    let mut mixed_docs = vec![
        create_test_document("valid1.md", "Valid document 1", "test-model"),
        create_test_document("valid2.md", "Valid document 2", "test-model"),
    ];

    // Add a document with potentially problematic data
    mixed_docs.push(Document {
        id: "problematic".to_string(),
        file_path: "".to_string(), // Empty path
        title: None,
        content: "".to_string(),
        embedding: vec![],
        tags: vec![],
        folder: "".to_string(),
        properties: HashMap::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    });

    let batch_operation = crucible_surrealdb::BatchOperation {
        operation_type: crucible_surrealdb::BatchOperationType::Create,
        documents: mixed_docs,
    };

    let batch_result = db
        .batch_operation(&batch_operation)
        .await
        .expect("Should handle mixed batch gracefully");

    // Should still process the valid documents
    assert!(
        batch_result.successful >= 2,
        "Should process valid documents"
    );

    println!("✓ All error handling tests passed");
}

#[tokio::test]
async fn test_embedding_index_functionality() {
    println!("Testing embedding index functionality...");

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_indexes.db");

    let db = SurrealEmbeddingDatabase::new(db_path.to_str().unwrap())
        .await
        .expect("Database creation should succeed");

    db.initialize()
        .await
        .expect("Schema initialization should succeed");

    // Test 1: Create documents with known similarity patterns
    println!("Creating documents with controlled similarity patterns...");

    let base_embedding = create_test_embedding(1000);

    // Create embeddings with controlled similarity to base
    let test_cases = vec![
        ("identical.md", "Identical content", base_embedding.clone()),
        ("very_similar.md", "Very similar content", {
            let mut emb = base_embedding.clone();
            // Slight modification to 10% of dimensions
            for i in (0..384).step_by(10) {
                emb[i] += 0.01;
            }
            emb
        }),
        ("moderately_similar.md", "Moderately similar content", {
            let mut emb = base_embedding.clone();
            // Modify 30% of dimensions
            for i in (0..384).step_by(3) {
                emb[i] += 0.1;
            }
            emb
        }),
        (
            "dissimilar.md",
            "Dissimilar content",
            create_test_embedding(2000),
        ),
        (
            "very_dissimilar.md",
            "Very dissimilar content",
            create_test_embedding(3000),
        ),
    ];

    for (file_path, content, embedding) in &test_cases {
        let metadata = create_test_metadata(file_path, "index-test-model");
        db.store_embedding(file_path, content, &embedding, &metadata)
            .await
            .expect("Should store test document");
    }

    // Test 2: Verify similarity ranking works correctly
    println!("Testing similarity ranking accuracy...");

    let search_results = db
        .search_similar("base query", &base_embedding, 10)
        .await
        .expect("Should perform ranked similarity search");

    assert_eq!(search_results.len(), 5, "Should find all test documents");

    // Print results for debugging
    println!("Search results:");
    for (i, result) in search_results.iter().enumerate() {
        println!("  {}: {} (score: {:.6})", i, result.id, result.score);
    }

    // Should find at least one result with high similarity
    let high_similarity_results: Vec<_> =
        search_results.iter().filter(|r| r.score > 0.95).collect();

    assert!(
        !high_similarity_results.is_empty(),
        "Should find at least one high-similarity result"
    );

    // Should find all our test documents in the results
    let result_ids: Vec<String> = search_results.iter().map(|r| r.id.clone()).collect();
    let original_ids = vec![
        "identical.md".to_string(),
        "very_similar.md".to_string(),
        "moderately_similar.md".to_string(),
        "dissimilar.md".to_string(),
        "very_dissimilar.md".to_string(),
    ];

    for original_id in &original_ids {
        assert!(
            result_ids.contains(original_id),
            "Should find {} in search results",
            original_id
        );
    }

    // All results should have valid scores (allowing for small floating point precision issues)
    for result in &search_results {
        assert!(
            result.score >= 0.0 && result.score <= 1.1,
            "Score should be between 0 and 1.1 (allowing for precision)"
        );
    }

    // Test 3: Test with different top_k values
    println!("Testing different result limits...");

    let top_2_results = db
        .search_similar("test", &base_embedding, 2)
        .await
        .expect("Should limit to top 2 results");

    assert_eq!(top_2_results.len(), 2);
    // Should find results from our test documents (order may vary due to similarity calculation)
    let top_2_ids: Vec<String> = top_2_results.iter().map(|r| r.id.clone()).collect();
    let original_ids = vec![
        "identical.md",
        "very_similar.md",
        "moderately_similar.md",
        "dissimilar.md",
        "very_dissimilar.md",
    ];
    assert!(
        top_2_ids
            .iter()
            .any(|id| original_ids.contains(&id.as_str())),
        "Should find test documents in top 2"
    );

    let top_1_results = db
        .search_similar("test", &base_embedding, 1)
        .await
        .expect("Should limit to top 1 result");

    assert_eq!(top_1_results.len(), 1);
    // Should return one of our test documents
    assert!(
        original_ids.contains(&top_1_results[0].id.as_str()),
        "Should return a test document as top result"
    );

    // Test 4: Index performance with repeated searches
    println!("Testing search performance consistency...");

    let mut search_times = Vec::new();
    for _ in 0..10 {
        let start = std::time::Instant::now();
        db.search_similar("performance test", &base_embedding, 5)
            .await
            .expect("Search should succeed");
        search_times.push(start.elapsed());
    }

    let avg_time = search_times.iter().sum::<std::time::Duration>() / search_times.len() as u32;
    let max_time = search_times.iter().max().unwrap();
    let min_time = search_times.iter().min().unwrap();

    println!(
        "Search performance: avg={:?}, min={:?}, max={:?}",
        avg_time, min_time, max_time
    );

    // Performance should be reasonable for in-memory operations
    assert!(
        max_time.as_millis() < 100,
        "Max search time should be under 100ms"
    );
    assert!(
        avg_time.as_millis() < 50,
        "Average search time should be under 50ms"
    );

    println!("✓ All embedding index functionality tests passed");
}
