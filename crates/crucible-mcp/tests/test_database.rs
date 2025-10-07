use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::types::EmbeddingMetadata;
use tempfile::tempdir;
use std::collections::HashMap;

#[tokio::test]
async fn test_database_initialization() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    // Test that we can check if a file exists (should be false for non-existent file)
    let exists = db.file_exists("nonexistent.md").await.unwrap();
    assert!(!exists);
}

#[tokio::test]
async fn test_store_and_retrieve_embedding() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let embedding = vec![0.1, 0.2, 0.3, 0.4];
    let metadata = EmbeddingMetadata {
        file_path: "test.md".to_string(),
        title: Some("Test File".to_string()),
        tags: vec!["test".to_string()],
        folder: "test_folder".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    // Store embedding
    db.store_embedding("test.md", "Test content", &embedding, &metadata).await.unwrap();
    
    // Check if file exists
    let exists = db.file_exists("test.md").await.unwrap();
    assert!(exists);
    
    // Retrieve embedding
    let retrieved = db.get_embedding("test.md").await.unwrap().unwrap();
    assert_eq!(retrieved.file_path, "test.md");
    assert_eq!(retrieved.content, "Test content");
    assert_eq!(retrieved.embedding, embedding);
    assert_eq!(retrieved.metadata.tags, vec!["test"]);
}

#[tokio::test]
async fn test_update_existing_embedding() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let embedding1 = vec![0.1, 0.2];
    let embedding2 = vec![0.3, 0.4];
    let metadata = EmbeddingMetadata {
        file_path: "test.md".to_string(),
        title: Some("Test File".to_string()),
        tags: vec!["test".to_string()],
        folder: "test_folder".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    // Store first embedding
    db.store_embedding("test.md", "Content 1", &embedding1, &metadata).await.unwrap();
    
    // Update with second embedding
    db.store_embedding("test.md", "Content 2", &embedding2, &metadata).await.unwrap();
    
    // Should still exist (not duplicate)
    let exists = db.file_exists("test.md").await.unwrap();
    assert!(exists);
    
    // Should have updated content
    let retrieved = db.get_embedding("test.md").await.unwrap().unwrap();
    assert_eq!(retrieved.content, "Content 2");
    assert_eq!(retrieved.embedding, embedding2);
}

#[tokio::test]
async fn test_list_files() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let embedding = vec![0.1, 0.2];
    let metadata = EmbeddingMetadata {
        file_path: "test.md".to_string(),
        title: Some("Test File".to_string()),
        tags: vec!["test".to_string()],
        folder: "test_folder".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    // Store multiple files
    db.store_embedding("file1.md", "Content 1", &embedding, &metadata).await.unwrap();
    db.store_embedding("file2.md", "Content 2", &embedding, &metadata).await.unwrap();
    
    let files = db.list_files().await.unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&"file1.md".to_string()));
    assert!(files.contains(&"file2.md".to_string()));
}

#[tokio::test]
async fn test_delete_file() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let embedding = vec![0.1, 0.2];
    let metadata = EmbeddingMetadata {
        file_path: "test.md".to_string(),
        title: Some("Test File".to_string()),
        tags: vec!["test".to_string()],
        folder: "test_folder".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    // Store file
    db.store_embedding("test.md", "Test content", &embedding, &metadata).await.unwrap();
    assert!(db.file_exists("test.md").await.unwrap());
    
    // Delete file
    db.delete_file("test.md").await.unwrap();
    assert!(!db.file_exists("test.md").await.unwrap());
}

#[tokio::test]
async fn test_get_stats() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let embedding = vec![0.1, 0.2];
    let metadata = EmbeddingMetadata {
        file_path: "test.md".to_string(),
        title: Some("Test File".to_string()),
        tags: vec!["test".to_string()],
        folder: "test_folder".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    // Store some files
    db.store_embedding("file1.md", "Content 1", &embedding, &metadata).await.unwrap();
    db.store_embedding("file2.md", "Content 2", &embedding, &metadata).await.unwrap();
    
    let stats = db.get_stats().await.unwrap();
    assert_eq!(stats.get("total_files"), Some(&2));
}

#[tokio::test]
async fn test_similarity_search() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let embedding1 = vec![1.0, 0.0, 0.0]; // Unit vector along x-axis
    let embedding2 = vec![0.0, 1.0, 0.0]; // Unit vector along y-axis
    let embedding3 = vec![0.0, 0.0, 1.0]; // Unit vector along z-axis
    
    let metadata = EmbeddingMetadata {
        file_path: "test.md".to_string(),
        title: Some("Test File".to_string()),
        tags: vec!["test".to_string()],
        folder: "test_folder".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    // Store embeddings
    db.store_embedding("file1.md", "Content 1", &embedding1, &metadata).await.unwrap();
    db.store_embedding("file2.md", "Content 2", &embedding2, &metadata).await.unwrap();
    db.store_embedding("file3.md", "Content 3", &embedding3, &metadata).await.unwrap();
    
    // Search with query similar to first file
    let query = vec![0.9, 0.1, 0.0]; // Similar to embedding1
    let results = db.search_similar(&query, 2).await.unwrap();
    
    assert_eq!(results.len(), 2);
    // First result should be most similar (file1.md)
    assert_eq!(results[0].id, "file1.md");
    assert!(results[0].score > results[1].score);
}

// New tests for 90%+ coverage

#[tokio::test]
async fn test_file_exists_false() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    // Test file that doesn't exist
    let exists = db.file_exists("nonexistent.md").await.unwrap();
    assert!(!exists);
}

#[tokio::test]
async fn test_get_embedding_none() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    // Test getting embedding for non-existent file
    let result = db.get_embedding("nonexistent.md").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_search_similar_empty_db() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    // Search in empty database
    let query = vec![0.1, 0.2, 0.3];
    let results = db.search_similar(&query, 5).await.unwrap();
    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn test_search_similar_with_limit() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let embedding = vec![1.0, 0.0];
    let metadata = EmbeddingMetadata {
        file_path: "test.md".to_string(),
        title: Some("Test File".to_string()),
        tags: vec!["test".to_string()],
        folder: "test_folder".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    // Store multiple files
    for i in 0..5 {
        db.store_embedding(&format!("file{}.md", i), &format!("Content {}", i), &embedding, &metadata).await.unwrap();
    }
    
    // Search with limit
    let query = vec![0.9, 0.1];
    let results = db.search_similar(&query, 3).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_store_embedding_with_empty_tags() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let embedding = vec![0.1, 0.2];
    let metadata = EmbeddingMetadata {
        file_path: "test.md".to_string(),
        title: Some("Test File".to_string()),
        tags: vec![], // Empty tags
        folder: "test_folder".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    db.store_embedding("test.md", "Test content", &embedding, &metadata).await.unwrap();
    
    let retrieved = db.get_embedding("test.md").await.unwrap().unwrap();
    assert_eq!(retrieved.metadata.tags, Vec::<String>::new());
}

#[tokio::test]
async fn test_store_embedding_with_empty_properties() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let embedding = vec![0.1, 0.2];
    let metadata = EmbeddingMetadata {
        file_path: "test.md".to_string(),
        title: Some("Test File".to_string()),
        tags: vec!["test".to_string()],
        folder: "test_folder".to_string(),
        properties: HashMap::new(), // Empty properties
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    db.store_embedding("test.md", "Test content", &embedding, &metadata).await.unwrap();
    
    let retrieved = db.get_embedding("test.md").await.unwrap().unwrap();
    assert!(retrieved.metadata.properties.is_empty());
}

#[tokio::test]
async fn test_list_files_empty() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    // Test listing files in empty database
    let files = db.list_files().await.unwrap();
    assert_eq!(files.len(), 0);
}

#[tokio::test]
async fn test_delete_nonexistent_file() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    // Delete non-existent file (should not error)
    db.delete_file("nonexistent.md").await.unwrap();
    
    // Verify it still doesn't exist
    assert!(!db.file_exists("nonexistent.md").await.unwrap());
}

#[tokio::test]
async fn test_get_stats_empty() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    // Test stats on empty database
    let stats = db.get_stats().await.unwrap();
    assert_eq!(stats.get("total_files"), Some(&0));
}

#[tokio::test]
async fn test_metadata_serialization() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let mut properties = HashMap::new();
    properties.insert("status".to_string(), serde_json::Value::String("active".to_string()));
    properties.insert("priority".to_string(), serde_json::Value::Number(serde_json::Number::from(1)));
    
    let embedding = vec![0.1, 0.2];
    let metadata = EmbeddingMetadata {
        file_path: "test.md".to_string(),
        title: Some("Test File".to_string()),
        tags: vec!["test".to_string(), "important".to_string()],
        folder: "test_folder".to_string(),
        properties,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    db.store_embedding("test.md", "Test content", &embedding, &metadata).await.unwrap();
    
    let retrieved = db.get_embedding("test.md").await.unwrap().unwrap();
    assert_eq!(retrieved.metadata.properties.len(), 2);
    assert_eq!(retrieved.metadata.tags.len(), 2);
}

#[tokio::test]
async fn test_embedding_array_conversion() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    // Test with a larger embedding vector
    let embedding = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
    let metadata = EmbeddingMetadata {
        file_path: "test.md".to_string(),
        title: Some("Test File".to_string()),
        tags: vec!["test".to_string()],
        folder: "test_folder".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    db.store_embedding("test.md", "Test content", &embedding, &metadata).await.unwrap();
    
    let retrieved = db.get_embedding("test.md").await.unwrap().unwrap();
    assert_eq!(retrieved.embedding, embedding);
}

#[tokio::test]
async fn test_search_by_tags() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let embedding = vec![0.1, 0.2];
    let metadata1 = EmbeddingMetadata {
        file_path: "file1.md".to_string(),
        title: Some("File 1".to_string()),
        tags: vec!["project".to_string(), "urgent".to_string()],
        folder: "test_folder".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    let metadata2 = EmbeddingMetadata {
        file_path: "file2.md".to_string(),
        title: Some("File 2".to_string()),
        tags: vec!["project".to_string(), "low".to_string()],
        folder: "test_folder".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    db.store_embedding("file1.md", "Content 1", &embedding, &metadata1).await.unwrap();
    db.store_embedding("file2.md", "Content 2", &embedding, &metadata2).await.unwrap();
    
    // Search by tags
    let results = db.search_by_tags(&["urgent".to_string()]).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(&"file1.md".to_string()));
}

#[tokio::test]
async fn test_search_by_properties() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("database_test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    
    let embedding = vec![0.1, 0.2];
    let mut properties1 = HashMap::new();
    properties1.insert("status".to_string(), serde_json::Value::String("active".to_string()));
    
    let mut properties2 = HashMap::new();
    properties2.insert("status".to_string(), serde_json::Value::String("inactive".to_string()));
    
    let metadata1 = EmbeddingMetadata {
        file_path: "file1.md".to_string(),
        title: Some("File 1".to_string()),
        tags: vec!["test".to_string()],
        folder: "test_folder".to_string(),
        properties: properties1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    let metadata2 = EmbeddingMetadata {
        file_path: "file2.md".to_string(),
        title: Some("File 2".to_string()),
        tags: vec!["test".to_string()],
        folder: "test_folder".to_string(),
        properties: properties2,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    db.store_embedding("file1.md", "Content 1", &embedding, &metadata1).await.unwrap();
    db.store_embedding("file2.md", "Content 2", &embedding, &metadata2).await.unwrap();
    
    // Search by properties
    let mut search_properties = HashMap::new();
    search_properties.insert("status".to_string(), serde_json::Value::String("active".to_string()));
    
    let results = db.search_by_properties(&search_properties).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(&"file1.md".to_string()));
}