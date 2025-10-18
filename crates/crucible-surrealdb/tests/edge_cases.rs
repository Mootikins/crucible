//! Edge case tests for SurrealEmbeddingDatabase
//! Tests boundary conditions, error scenarios, and performance edge cases

use crucible_surrealdb::{SurrealEmbeddingDatabase, EmbeddingData, EmbeddingMetadata, Document, SearchQuery, SearchFilters, BatchOperation, BatchOperationType};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_empty_database_operations() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Test search on empty database
    let results = db.search_similar("nonexistent", &[0.1; 384], 5).await.unwrap();
    assert_eq!(results.len(), 0);

    // Test get_embedding on nonexistent file
    let result = db.get_embedding("nonexistent.txt").await.unwrap();
    assert!(result.is_none());

    // Test file_exists on nonexistent file
    assert!(!db.file_exists("nonexistent.txt").await.unwrap());

    // Test list_files on empty database
    let files = db.list_files().await.unwrap();
    assert_eq!(files.len(), 0);

    // Test update_metadata on nonexistent file
    let new_metadata = HashMap::new();
    let result = db.update_metadata_hashmap("nonexistent.txt", new_metadata).await.unwrap();
    assert!(!result);

    // Test delete_file on nonexistent file
    let result = db.delete_file("nonexistent.txt").await.unwrap();
    assert!(!result);

    // Test get_stats on empty database
    let stats = db.get_stats().await.unwrap();
    assert_eq!(stats.total_documents, 0);
    assert_eq!(stats.total_embeddings, 0);
}

#[tokio::test]
async fn test_single_document_operations() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    let mut metadata = HashMap::new();
    metadata.insert("key".to_string(), serde_json::Value::String("value".to_string()));

    let embedding_data = EmbeddingData {
        file_path: "single.txt".to_string(),
        content: "Single document content".to_string(),
        embedding: vec![0.1; 384],
        metadata: EmbeddingMetadata {
            file_path: "single.txt".to_string(),
            title: Some("Single Document".to_string()),
            tags: vec!["tag1".to_string()],
            folder: "test".to_string(),
            properties: metadata.clone(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    db.store_embedding_data(&embedding_data).await.unwrap();

    // Test all operations work with single document
    assert!(db.file_exists("single.txt").await.unwrap());

    let retrieved = db.get_embedding("single.txt").await.unwrap().unwrap();
    assert_eq!(retrieved.file_path, "single.txt");

    let files = db.list_files().await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0], "single.txt");

    let stats = db.get_stats().await.unwrap();
    assert_eq!(stats.total_documents, 1);
    assert_eq!(stats.total_embeddings, 1);
}

#[tokio::test]
async fn test_maximum_filename_length() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Test with very long filename (approaching filesystem limits)
    let long_filename = "a".repeat(255) + ".txt";

    let embedding_data = EmbeddingData {
        file_path: long_filename.clone(),
        content: "Long filename test".to_string(),
        embedding: vec![0.1; 384],
        metadata: EmbeddingMetadata {
            file_path: long_filename.clone(),
            title: Some("Long Filename Test".to_string()),
            tags: vec![],
            folder: "test".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    // Should handle long filenames gracefully
    let result = db.store_embedding_data(&embedding_data).await;
    assert!(result.is_ok());

    if result.is_ok() {
        assert!(db.file_exists(&long_filename).await.unwrap());

        let retrieved = db.get_embedding(&long_filename).await.unwrap().unwrap();
        assert_eq!(retrieved.file_path, long_filename);
    }
}

#[tokio::test]
async fn test_empty_and_whitespace_strings() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Test with empty filename
    let embedding_data = EmbeddingData {
        file_path: "".to_string(),
        content: "Empty filename test".to_string(),
        embedding: vec![0.1; 384],
        metadata: EmbeddingMetadata {
            file_path: "".to_string(),
            title: Some("Empty Filename".to_string()),
            tags: vec![],
            folder: "".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    // Should handle empty filename gracefully
    let result = db.store_embedding_data(&embedding_data).await;
    assert!(result.is_ok());

    // Test with whitespace-only content
    let whitespace_content = "   \n\t   ".to_string();
    let embedding_data2 = EmbeddingData {
        file_path: "whitespace.txt".to_string(),
        content: whitespace_content.clone(),
        embedding: vec![0.2; 384],
        metadata: EmbeddingMetadata {
            file_path: "whitespace.txt".to_string(),
            title: Some("   ".to_string()),
            tags: vec!["".to_string()],
            folder: "   ".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    let result = db.store_embedding_data(&embedding_data2).await;
    assert!(result.is_ok());

    // Test search with empty query
    let results = db.search_similar("", &[0.1; 384], 5).await.unwrap();
    // Should not crash, though results depend on implementation
}

#[tokio::test]
async fn test_unicode_and_special_characters() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Test with Unicode characters, emojis, and special symbols
    let unicode_filename = "测试文件🚀.txt".to_string();
    let unicode_content = "Test content with Unicode: café, naïve, 中文, 🎉".to_string();
    let unicode_title = "测试标题 📚".to_string();
    let unicode_tags = vec!["标签1".to_string(), "täg2".to_string(), "🏷️".to_string()];
    let unicode_folder = "测试文件夹 📁".to_string();

    let mut properties = HashMap::new();
    properties.insert(" clé ".to_string(), serde_json::Value::String("valeur & test".to_string()));
    properties.insert("emoji".to_string(), serde_json::Value::String("🎯".to_string()));

    let embedding_data = EmbeddingData {
        file_path: unicode_filename.clone(),
        content: unicode_content.clone(),
        embedding: vec![0.1; 384],
        metadata: EmbeddingMetadata {
            file_path: unicode_filename.clone(),
            title: Some(unicode_title.clone()),
            tags: unicode_tags.clone(),
            folder: unicode_folder.clone(),
            properties: properties.clone(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    db.store_embedding_data(&embedding_data).await.unwrap();

    // Verify all Unicode data is preserved
    assert!(db.file_exists(&unicode_filename).await.unwrap());

    let retrieved = db.get_embedding(&unicode_filename).await.unwrap().unwrap();
    assert_eq!(retrieved.file_path, unicode_filename);
    assert_eq!(retrieved.content, unicode_content);
    assert_eq!(retrieved.metadata.title, Some(unicode_title));
    assert_eq!(retrieved.metadata.tags, unicode_tags);
    assert_eq!(retrieved.metadata.folder, unicode_folder);
    assert_eq!(retrieved.metadata.properties, properties);

    // Test search with Unicode query
    let results = db.search_similar("测试", &[0.1; 384], 5).await.unwrap();
    assert!(!results.is_empty());
}

#[tokio::test]
async fn test_embedding_vector_edge_cases() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Test with zero vector
    let zero_embedding_data = EmbeddingData {
        file_path: "zero.txt".to_string(),
        content: "Zero vector test".to_string(),
        embedding: vec![0.0; 384],
        metadata: EmbeddingMetadata {
            file_path: "zero.txt".to_string(),
            title: Some("Zero Vector".to_string()),
            tags: vec![],
            folder: "test".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    db.store_embedding_data(&zero_embedding_data).await.unwrap();

    // Test with maximum values
    let max_embedding_data = EmbeddingData {
        file_path: "max.txt".to_string(),
        content: "Maximum values test".to_string(),
        embedding: vec![f32::MAX; 384],
        metadata: EmbeddingMetadata {
            file_path: "max.txt".to_string(),
            title: Some("Max Values".to_string()),
            tags: vec![],
            folder: "test".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    db.store_embedding_data(&max_embedding_data).await.unwrap();

    // Test with minimum values
    let min_embedding_data = EmbeddingData {
        file_path: "min.txt".to_string(),
        content: "Minimum values test".to_string(),
        embedding: vec![f32::MIN; 384],
        metadata: EmbeddingMetadata {
            file_path: "min.txt".to_string(),
            title: Some("Min Values".to_string()),
            tags: vec![],
            folder: "test".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    db.store_embedding_data(&min_embedding_data).await.unwrap();

    // Test search with NaN vector
    let mut nan_embedding = vec![0.1; 384];
    nan_embedding[100] = f32::NAN;

    let results = db.search_similar("test", &nan_embedding, 5).await.unwrap();
    // Should handle NaN gracefully without crashing

    // Test search with infinity
    let mut inf_embedding = vec![0.1; 384];
    inf_embedding[200] = f32::INFINITY;

    let results = db.search_similar("test", &inf_embedding, 5).await.unwrap();
    // Should handle infinity gracefully without crashing
}

#[tokio::test]
async fn test_different_embedding_dimensions() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Test with smaller dimension
    let small_embedding_data = EmbeddingData {
        file_path: "small.txt".to_string(),
        content: "Small dimension test".to_string(),
        embedding: vec![0.1; 128], // Smaller than expected
        metadata: EmbeddingMetadata {
            file_path: "small.txt".to_string(),
            title: Some("Small Dimension".to_string()),
            tags: vec![],
            folder: "test".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    db.store_embedding_data(&small_embedding_data).await.unwrap();

    // Test with larger dimension
    let large_embedding_data = EmbeddingData {
        file_path: "large.txt".to_string(),
        content: "Large dimension test".to_string(),
        embedding: vec![0.1; 1024], // Larger than expected
        metadata: EmbeddingMetadata {
            file_path: "large.txt".to_string(),
            title: Some("Large Dimension".to_string()),
            tags: vec![],
            folder: "test".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    db.store_embedding_data(&large_embedding_data).await.unwrap();

    // Test search with different dimensions
    let results_128 = db.search_similar("test", &[0.1; 128], 5).await.unwrap();
    let results_1024 = db.search_similar("test", &[0.1; 1024], 5).await.unwrap();

    // Should handle different dimensions gracefully
}

#[tokio::test]
async fn test_concurrent_operations() {
    let db = std::sync::Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await.unwrap();

    let mut handles = vec![];

    // Concurrent stores
    for i in 0..10 {
        let db_clone = db.clone();
        let handle = tokio::spawn(async move {
            let embedding_data = EmbeddingData {
                file_path: format!("concurrent_{}.txt", i),
                content: format!("Concurrent content {}", i),
                embedding: vec![i as f32 / 10.0; 384],
                metadata: EmbeddingMetadata {
                    file_path: format!("concurrent_{}.txt", i),
                    title: Some(format!("Concurrent {}", i)),
                    tags: vec![format!("tag{}", i)],
                    folder: "concurrent".to_string(),
                    properties: HashMap::new(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                },
            };

            db_clone.store_embedding_data(&embedding_data).await.unwrap();
        });
        handles.push(handle);
    }

    // Concurrent reads
    for i in 0..5 {
        let db_clone = db.clone();
        let handle = tokio::spawn(async move {
            // Try to read files that may or may not exist yet
            let _ = db_clone.get_embedding(&format!("concurrent_{}.txt", i * 2)).await;
            let _ = db_clone.file_exists(&format!("concurrent_{}.txt", i * 2)).await;
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        let _ = handle.await;
    }

    // Verify all data was stored correctly
    let files = db.list_files().await.unwrap();
    assert_eq!(files.len(), 10);

    let stats = db.get_stats().await.unwrap();
    assert_eq!(stats.total_documents, 10);
}

#[tokio::test]
async fn test_large_dataset_performance() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    let start_time = std::time::Instant::now();

    // Store 1000 documents
    for i in 0..1000 {
        let embedding_data = EmbeddingData {
            file_path: format!("large_{}.txt", i),
            content: format!("Large dataset content {}", i),
            embedding: vec![i as f32 / 1000.0; 384],
            metadata: EmbeddingMetadata {
                file_path: format!("large_{}.txt", i),
                title: Some(format!("Large Document {}", i)),
                tags: vec![format!("tag{}", i % 10)],
                folder: format!("folder{}", i % 5),
                properties: HashMap::new(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        };

        db.store_embedding_data(&embedding_data).await.unwrap();
    }

    let store_time = start_time.elapsed();
    println!("Stored 1000 documents in {:?}", store_time);

    // Test search performance
    let search_start = std::time::Instant::now();
    let results = db.search_similar("large dataset", &[0.5; 384], 10).await.unwrap();
    let search_time = search_start.elapsed();

    println!("Searched 1000 documents in {:?}, found {} results", search_time, results.len());
    assert!(results.len() <= 10);

    // Test list_files performance
    let list_start = std::time::Instant::now();
    let files = db.list_files().await.unwrap();
    let list_time = list_start.elapsed();

    println!("Listed {} files in {:?}", files.len(), list_time);
    assert_eq!(files.len(), 1000);

    // Performance assertions (these should be adjusted based on requirements)
    assert!(store_time.as_secs() < 10, "Storage took too long: {:?}", store_time);
    assert!(search_time.as_secs() < 5, "Search took too long: {:?}", search_time);
    assert!(list_time.as_secs() < 1, "List operation took too long: {:?}", list_time);
}

#[tokio::test]
async fn test_timeout_operations() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Test operations with timeout
    let timeout_duration = Duration::from_millis(100);

    // Store with timeout
    let embedding_data = EmbeddingData {
        file_path: "timeout.txt".to_string(),
        content: "Timeout test".to_string(),
        embedding: vec![0.1; 384],
        metadata: EmbeddingMetadata {
            file_path: "timeout.txt".to_string(),
            title: Some("Timeout Test".to_string()),
            tags: vec![],
            folder: "test".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    let result = timeout(timeout_duration, db.store_embedding_data(&embedding_data)).await;
    assert!(result.is_ok(), "Store operation should complete within timeout");

    // Search with timeout
    let result = timeout(timeout_duration, db.search_similar("test", &[0.1; 384], 5)).await;
    assert!(result.is_ok(), "Search operation should complete within timeout");
}

#[tokio::test]
async fn test_batch_operation_edge_cases() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Test empty batch operation
    let empty_batch = BatchOperation {
        operation_type: BatchOperationType::Create,
        documents: vec![],
    };

    let result = db.batch_operation(&empty_batch).await.unwrap();
    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 0);
    assert!(result.errors.is_empty());

    // Test batch with mixed valid and invalid documents
    let mut documents = vec![];

    // Valid document
    documents.push(Document {
        id: "valid_doc".to_string(),
        file_path: "valid.txt".to_string(),
        title: Some("Valid Document".to_string()),
        content: "Valid content".to_string(),
        embedding: vec![0.1; 384],
        tags: vec!["valid".to_string()],
        folder: "test".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    });

    // Document with empty path (might be invalid)
    documents.push(Document {
        id: "empty_path".to_string(),
        file_path: "".to_string(),
        title: Some("Empty Path".to_string()),
        content: "Empty path content".to_string(),
        embedding: vec![0.2; 384],
        tags: vec![],
        folder: "test".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    });

    let batch = BatchOperation {
        operation_type: BatchOperationType::Create,
        documents,
    };

    let result = db.batch_operation(&batch).await.unwrap();
    // Should handle mixed documents gracefully
    assert!(result.successful >= 1); // At least the valid document
}

#[tokio::test]
async fn test_search_filter_edge_cases() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Add test data
    let embedding_data = EmbeddingData {
        file_path: "filter_test.txt".to_string(),
        content: "Filter test content".to_string(),
        embedding: vec![0.1; 384],
        metadata: EmbeddingMetadata {
            file_path: "filter_test.txt".to_string(),
            title: Some("Filter Test".to_string()),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            folder: "test/folder".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("key1".to_string(), serde_json::Value::String("value1".to_string()));
                props.insert("key2".to_string(), serde_json::Value::Number(serde_json::Number::from(42)));
                props
            },
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    db.store_embedding_data(&embedding_data).await.unwrap();

    // Test search with empty filters
    let search_query = SearchQuery {
        query: "test".to_string(),
        filters: None,
        limit: Some(10),
        offset: None,
    };

    let results = db.search_with_filters(&search_query).await.unwrap();
    assert!(!results.is_empty());

    // Test search with empty filter values
    let search_query = SearchQuery {
        query: "test".to_string(),
        filters: Some(SearchFilters {
            tags: Some(vec![]),
            folder: None,
            properties: None,
            date_range: None,
        }),
        limit: Some(10),
        offset: None,
    };

    let _results = db.search_with_filters(&search_query).await.unwrap();
    // Should handle empty filter lists gracefully

    // Test search with non-existent filter values
    let search_query = SearchQuery {
        query: "test".to_string(),
        filters: Some(SearchFilters {
            tags: Some(vec!["nonexistent_tag".to_string()]),
            folder: Some("nonexistent_folder".to_string()),
            properties: Some({
                let mut props = HashMap::new();
                props.insert("nonexistent_key".to_string(), serde_json::Value::String("nonexistent_value".to_string()));
                props
            }),
            date_range: None,
        }),
        limit: Some(10),
        offset: None,
    };

    let results = db.search_with_filters(&search_query).await.unwrap();
    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn test_graph_relation_edge_cases() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Add test documents
    let doc1 = EmbeddingData {
        file_path: "doc1.txt".to_string(),
        content: "Document 1".to_string(),
        embedding: vec![0.1; 384],
        metadata: EmbeddingMetadata {
            file_path: "doc1.txt".to_string(),
            title: Some("Document 1".to_string()),
            tags: vec![],
            folder: "test".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    let doc2 = EmbeddingData {
        file_path: "doc2.txt".to_string(),
        content: "Document 2".to_string(),
        embedding: vec![0.2; 384],
        metadata: EmbeddingMetadata {
            file_path: "doc2.txt".to_string(),
            title: Some("Document 2".to_string()),
            tags: vec![],
            folder: "test".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    db.store_embedding_data(&doc1).await.unwrap();
    db.store_embedding_data(&doc2).await.unwrap();

    // Test relation with non-existent documents
    let result = db.add_relation("nonexistent1.txt", "doc2.txt", "references", HashMap::new()).await.unwrap();
    assert!(!result);

    let result = db.add_relation("doc1.txt", "nonexistent2.txt", "references", HashMap::new()).await.unwrap();
    assert!(!result);

    // Test relation with empty strings
    let result = db.add_relation("", "doc2.txt", "references", HashMap::new()).await.unwrap();
    assert!(!result);

    let result = db.add_relation("doc1.txt", "", "references", HashMap::new()).await.unwrap();
    assert!(!result);

    // Test valid relation
    let result = db.add_relation("doc1.txt", "doc2.txt", "references", HashMap::new()).await.unwrap();
    assert!(result);

    // Test duplicate relation
    let result = db.add_relation("doc1.txt", "doc2.txt", "references", HashMap::new()).await.unwrap();
    // Should handle gracefully (might succeed or fail depending on implementation)

    // Test relation removal with non-existent relation
    let result = db.remove_relation("doc1.txt", "doc2.txt", "nonexistent_relation").await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn test_memory_pressure_cleanup() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Add many large documents to test memory handling
    for i in 0..100 {
        let large_content = "Large content ".repeat(10000); // ~150KB per document
        let embedding_data = EmbeddingData {
            file_path: format!("large_{}.txt", i),
            content: large_content,
            embedding: vec![i as f32 / 100.0; 384],
            metadata: EmbeddingMetadata {
                file_path: format!("large_{}.txt", i),
                title: Some(format!("Large Document {}", i)),
                tags: vec![],
                folder: "test".to_string(),
                properties: HashMap::new(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        };

        db.store_embedding_data(&embedding_data).await.unwrap();
    }

    // Delete all documents to test cleanup
    for i in 0..100 {
        let result = db.delete_file(&format!("large_{}.txt", i)).await.unwrap();
        assert!(result);
    }

    // Verify cleanup
    let files = db.list_files().await.unwrap();
    assert_eq!(files.len(), 0);

    let stats = db.get_stats().await.unwrap();
    assert_eq!(stats.total_documents, 0);
}

#[tokio::test]
async fn test_error_recovery() {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await.unwrap();

    // Test that the database remains functional after various error conditions

    // Try to get non-existent file
    let result = db.get_embedding("nonexistent.txt").await.unwrap();
    assert!(result.is_none());

    // Database should still work for valid operations
    let embedding_data = EmbeddingData {
        file_path: "recovery_test.txt".to_string(),
        content: "Recovery test".to_string(),
        embedding: vec![0.1; 384],
        metadata: EmbeddingMetadata {
            file_path: "recovery_test.txt".to_string(),
            title: Some("Recovery Test".to_string()),
            tags: vec![],
            folder: "test".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    };

    db.store_embedding_data(&embedding_data).await.unwrap();
    assert!(db.file_exists("recovery_test.txt").await.unwrap());

    // Try to delete non-existent file
    let result = db.delete_file("nonexistent.txt").await.unwrap();
    assert!(!result);

    // Database should still work
    assert!(db.file_exists("recovery_test.txt").await.unwrap());

    // Clean up
    let result = db.delete_file("recovery_test.txt").await.unwrap();
    assert!(result);
}