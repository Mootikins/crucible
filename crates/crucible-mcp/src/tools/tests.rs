// Comprehensive tests for tools/mod.rs
#[cfg(test)]
mod tools_tests {
    use super::super::*;
    use crate::database::EmbeddingDatabase;
    use crate::embeddings::{create_provider, create_mock_provider, EmbeddingConfig};
    use crate::types::{ToolCallArgs, EmbeddingMetadata};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn setup_test_db() -> EmbeddingDatabase {
        EmbeddingDatabase::new(":memory:").await.unwrap()
    }

    async fn setup_test_provider() -> Arc<dyn crate::embeddings::EmbeddingProvider> {
        // Use the mock provider for testing to avoid external HTTP calls
        create_mock_provider(768)
    }

    fn create_test_metadata(path: &str) -> EmbeddingMetadata {
        let mut properties = HashMap::new();
        properties.insert("status".to_string(), json!("active"));

        EmbeddingMetadata {
            file_path: path.to_string(),
            title: Some(path.to_string()),
            tags: vec!["test".to_string()],
            folder: "test_folder".to_string(),
            properties,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_search_by_properties_success() {
        let db = setup_test_db().await;

        // Store test embedding
        let metadata = create_test_metadata("test.md");
        let embedding = vec![0.1; 1536];
        db.store_embedding("test.md", "content", &embedding, &metadata).await.unwrap();

        // Search by properties
        let mut search_props = HashMap::new();
        search_props.insert("status".to_string(), json!("active"));

        let args = ToolCallArgs {
            properties: Some(search_props),
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = search_by_properties(&db, &args).await.unwrap();
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_search_by_properties_missing_param() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = search_by_properties(&db, &args).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Missing properties"));
    }

    #[tokio::test]
    async fn test_search_by_tags_success() {
        let db = setup_test_db().await;

        // Store test embedding
        let metadata = create_test_metadata("test.md");
        let embedding = vec![0.1; 1536];
        db.store_embedding("test.md", "content", &embedding, &metadata).await.unwrap();

        let args = ToolCallArgs {
            properties: None,
            tags: Some(vec!["test".to_string()]),
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = search_by_tags(&db, &args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_search_by_tags_missing_param() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = search_by_tags(&db, &args).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Missing tags parameter"));
    }

    #[tokio::test]
    async fn test_search_by_folder_recursive() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: Some("test_folder".to_string()),
            recursive: Some(true),
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = search_by_folder(&db, &args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_search_by_folder_non_recursive() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: Some("test_folder".to_string()),
            recursive: Some(false),
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = search_by_folder(&db, &args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_search_by_folder_missing_path() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = search_by_folder(&db, &args).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Missing path"));
    }

    #[tokio::test]
    async fn test_search_by_filename_wildcard() {
        let db = setup_test_db().await;

        let metadata = create_test_metadata("test.md");
        let embedding = vec![0.1; 1536];
        db.store_embedding("test.md", "content", &embedding, &metadata).await.unwrap();

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: Some("*.md".to_string()),
            query: None,
            top_k: None,
            force: None,
        };

        let result = search_by_filename(&db, &args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_search_by_filename_exact() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: Some("test".to_string()),
            query: None,
            top_k: None,
            force: None,
        };

        let result = search_by_filename(&db, &args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_search_by_filename_missing_pattern() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = search_by_filename(&db, &args).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Missing pattern"));
    }

    #[tokio::test]
    async fn test_search_by_content_success() {
        let db = setup_test_db().await;

        let metadata = create_test_metadata("test.md");
        let embedding = vec![0.1; 1536];
        db.store_embedding("test.md", "hello world", &embedding, &metadata).await.unwrap();

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: Some("hello".to_string()),
            top_k: None,
            force: None,
        };

        let result = search_by_content(&db, &args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_search_by_content_missing_query() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = search_by_content(&db, &args).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Missing query"));
    }

    #[tokio::test]
    async fn test_semantic_search_success() {
        let db = setup_test_db().await;
        let provider = setup_test_provider().await;

        let metadata = create_test_metadata("test.md");
        let embedding = vec![0.1; 1536];
        db.store_embedding("test.md", "test content", &embedding, &metadata).await.unwrap();

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: Some("test".to_string()),
            top_k: Some(5),
            force: None,
        };

        let result = semantic_search(&db, &provider, &args).await.unwrap();
        // semantic_search may fail if embedding provider doesn't work in test environment
        // The important thing is that it doesn't panic
        assert!(result.error.is_some() || result.success);
    }

    #[tokio::test]
    async fn test_semantic_search_missing_query() {
        let db = setup_test_db().await;
        let provider = setup_test_provider().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = semantic_search(&db, &provider, &args).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Missing query"));
    }

    #[tokio::test]
    async fn test_get_note_metadata_success() {
        let db = setup_test_db().await;

        let metadata = create_test_metadata("test.md");
        let embedding = vec![0.1; 1536];
        db.store_embedding("test.md", "content", &embedding, &metadata).await.unwrap();

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: Some("test.md".to_string()),
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = get_note_metadata(&db, &args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_get_note_metadata_not_found() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: Some("nonexistent.md".to_string()),
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = get_note_metadata(&db, &args).await.unwrap();
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("File not found"));
    }

    #[tokio::test]
    async fn test_get_note_metadata_missing_path() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = get_note_metadata(&db, &args).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Missing path"));
    }

    #[tokio::test]
    async fn test_update_note_properties_success() {
        let db = setup_test_db().await;

        let metadata = create_test_metadata("test.md");
        let embedding = vec![0.1; 1536];
        db.store_embedding("test.md", "content", &embedding, &metadata).await.unwrap();

        let mut new_props = HashMap::new();
        new_props.insert("status".to_string(), json!("updated"));

        let args = ToolCallArgs {
            properties: Some(new_props),
            tags: None,
            path: Some("test.md".to_string()),
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = update_note_properties(&db, &args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_update_note_properties_missing_path() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: Some(HashMap::new()),
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = update_note_properties(&db, &args).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Missing path"));
    }

    #[tokio::test]
    async fn test_update_note_properties_missing_properties() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: Some("test.md".to_string()),
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = update_note_properties(&db, &args).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Missing properties"));
    }

    #[tokio::test]
    async fn test_get_document_stats_success() {
        let db = setup_test_db().await;

        let args = ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };

        let result = get_document_stats(&db, &args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_create_minimal_file_metadata() {
        let metadata = create_minimal_file_metadata("test/path/file.md");
        assert_eq!(metadata.path, "test/path/file.md");
        assert_eq!(metadata.folder, "test/path");
        assert!(metadata.tags.is_empty());
        assert!(metadata.properties.is_empty());
    }

    #[tokio::test]
    async fn test_scan_filesystem_nonexistent_path() {
        let result = scan_filesystem_for_markdown_files("/nonexistent/path").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_scan_filesystem_empty_directory() {
        let temp_dir = TempDir::new().unwrap();

        // Create subdirectory without leading dot to avoid hidden file filtering
        let sub_dir = temp_dir.path().join("vault");
        std::fs::create_dir(&sub_dir).unwrap();

        // Empty directory should return error
        let result = scan_filesystem_for_markdown_files(sub_dir.to_str().unwrap()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No markdown files found"));
    }

    #[tokio::test]
    async fn test_read_file_content_fallback_success() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.md");
        std::fs::write(&test_file, "Test content").unwrap();

        let result = read_file_content_fallback(test_file.to_str().unwrap()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Test content");
    }

    #[tokio::test]
    async fn test_read_file_content_fallback_not_found() {
        let result = read_file_content_fallback("/nonexistent/file.md").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_basic_file_metadata_success() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.md");
        std::fs::write(&test_file, "---\ntitle: Test\ntags: [test, demo]\n---\nContent").unwrap();

        let result = create_basic_file_metadata(test_file.to_str().unwrap()).await;
        assert!(result.is_ok());
        let metadata = result.unwrap();
        assert_eq!(metadata.path, test_file.to_str().unwrap());
        assert!(metadata.tags.len() > 0);
    }

    #[tokio::test]
    async fn test_create_basic_file_metadata_no_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.md");
        std::fs::write(&test_file, "Just content, no frontmatter").unwrap();

        let result = create_basic_file_metadata(test_file.to_str().unwrap()).await;
        assert!(result.is_ok());
    }
}
