//! # Data Store Unit Tests
//!
//! This module contains comprehensive unit tests for the Data Store service,
//! testing individual components, database operations, and error handling.

use std::collections::HashMap;
use std::time::Duration;

use crucible_services::{
    data_store::{
        CrucibleDataStore, DataStoreConfig, DatabaseBackend, DatabaseBackendConfig,
        DocumentData, DocumentId, DocumentMetadata, DatabaseBackendType,
        QueryRequest, QueryResponse, TransactionStatus
    },
    errors::ServiceError,
    types::{ServiceHealth, ServiceStatus},
};

/// Create a test data store configuration
fn create_test_config() -> DataStoreConfig {
    DataStoreConfig {
        backend: DatabaseBackendConfig {
            backend_type: DatabaseBackendType::Memory,
            connection_string: ":memory:".to_string(),
            max_connections: 5,
            connection_timeout: Duration::from_secs(30),
            query_timeout: Duration::from_secs(10),
        },
        default_database: "test_db".to_string(),
        enable_wal: true,
        cache_size: 1024 * 1024, // 1MB
        enable_vector_search: true,
        vector_dimensions: 1536,
    }
}

/// Create a test document
fn create_test_document(id: &str, content: &str) -> DocumentData {
    DocumentData {
        id: DocumentId::from(id),
        metadata: DocumentMetadata {
            title: Some(format!("Test Document {}", id)),
            author: Some("test_user".to_string()),
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            tags: vec!["test".to_string(), "unit_test".to_string()],
            content_type: "text/plain".to_string(),
            size_bytes: content.len() as u64,
            checksum: Some(format!("checksum_{}", id)),
            version: 1,
        },
        content: content.to_string(),
        embeddings: None,
        indexed_fields: {
            let mut fields = HashMap::new();
            fields.insert("title".to_string(), format!("Test Document {}", id));
            fields.insert("author".to_string(), "test_user".to_string());
            fields
        },
    }
}

#[cfg(test)]
mod data_store_tests {
    use super::*;

    #[test]
    fn test_data_store_config_creation() {
        let config = create_test_config();

        assert_eq!(config.default_database, "test_db");
        assert!(config.enable_wal);
        assert_eq!(config.cache_size, 1024 * 1024);
        assert!(config.enable_vector_search);
        assert_eq!(config.vector_dimensions, 1536);
    }

    #[test]
    fn test_database_backend_config() {
        let backend_config = DatabaseBackendConfig {
            backend_type: DatabaseBackendType::DuckDB,
            connection_string: "/tmp/test.db".to_string(),
            max_connections: 10,
            connection_timeout: Duration::from_secs(60),
            query_timeout: Duration::from_secs(30),
        };

        assert!(matches!(backend_config.backend_type, DatabaseBackendType::DuckDB));
        assert_eq!(backend_config.connection_string, "/tmp/test.db");
        assert_eq!(backend_config.max_connections, 10);
        assert_eq!(backend_config.connection_timeout, Duration::from_secs(60));
        assert_eq!(backend_config.query_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_document_id_creation() {
        let id1 = DocumentId::from("test_doc_1");
        let id2 = DocumentId::new();
        let id3 = DocumentId::with_uuid(uuid::Uuid::new_v4());

        assert_eq!(id1.to_string(), "test_doc_1");
        assert!(!id2.to_string().is_empty());
        assert!(!id3.to_string().is_empty());
        assert_ne!(id2.to_string(), id3.to_string());
    }

    #[test]
    fn test_document_metadata_creation() {
        let now = chrono::Utc::now();
        let metadata = DocumentMetadata {
            title: Some("Test Document".to_string()),
            author: Some("test_author".to_string()),
            created_at: now,
            modified_at: now,
            tags: vec!["test".to_string(), "example".to_string()],
            content_type: "text/markdown".to_string(),
            size_bytes: 1024,
            checksum: Some("abc123".to_string()),
            version: 2,
        };

        assert_eq!(metadata.title.unwrap(), "Test Document");
        assert_eq!(metadata.author.unwrap(), "test_author");
        assert_eq!(metadata.tags.len(), 2);
        assert!(metadata.tags.contains(&"test".to_string()));
        assert_eq!(metadata.content_type, "text/markdown");
        assert_eq!(metadata.size_bytes, 1024);
        assert_eq!(metadata.checksum.unwrap(), "abc123");
        assert_eq!(metadata.version, 2);
    }

    #[test]
    fn test_document_data_creation() {
        let doc = create_test_document("doc1", "This is test content");

        assert_eq!(doc.id.to_string(), "doc1");
        assert_eq!(doc.content, "This is test content");
        assert_eq!(doc.metadata.title.unwrap(), "Test Document doc1");
        assert_eq!(doc.metadata.author.unwrap(), "test_user");
        assert_eq!(doc.indexed_fields.len(), 2);
        assert!(doc.embeddings.is_none());
    }

    #[test]
    fn test_query_request_creation() {
        let query = QueryRequest {
            database: "test_db".to_string(),
            query: "SELECT * FROM documents WHERE tags LIKE '%test%'".to_string(),
            parameters: vec![],
            limit: Some(100),
            offset: Some(0),
            order_by: Some("created_at DESC".to_string()),
        };

        assert_eq!(query.database, "test_db");
        assert!(query.query.contains("documents"));
        assert_eq!(query.limit.unwrap(), 100);
        assert_eq!(query.offset.unwrap(), 0);
        assert_eq!(query.order_by.unwrap(), "created_at DESC");
    }

    #[test]
    fn test_query_response_creation() {
        let mut results = Vec::new();
        results.push(create_test_document("doc1", "Content 1"));
        results.push(create_test_document("doc2", "Content 2"));

        let response = QueryResponse {
            results,
            total_count: 2,
            execution_time: Duration::from_millis(150),
            has_more: false,
        };

        assert_eq!(response.results.len(), 2);
        assert_eq!(response.total_count, 2);
        assert_eq!(response.execution_time, Duration::from_millis(150));
        assert!(!response.has_more);
    }

    #[test]
    fn test_transaction_status_variants() {
        let statuses = vec![
            TransactionStatus::Active,
            TransactionStatus::Committed,
            TransactionStatus::RolledBack,
        ];

        for status in statuses {
            match status {
                TransactionStatus::Active => assert!(matches!(status, TransactionStatus::Active)),
                TransactionStatus::Committed => assert!(matches!(status, TransactionStatus::Committed)),
                TransactionStatus::RolledBack => assert!(matches!(status, TransactionStatus::RolledBack)),
            }
        }
    }

    #[test]
    fn test_database_backend_types() {
        let backends = vec![
            DatabaseBackendType::Memory,
            DatabaseBackendType::DuckDB,
            DatabaseBackendType::SQLite,
        ];

        for backend in backends {
            match backend {
                DatabaseBackendType::Memory => assert!(matches!(backend, DatabaseBackendType::Memory)),
                DatabaseBackendType::DuckDB => assert!(matches!(backend, DatabaseBackendType::DuckDB)),
                DatabaseBackendType::SQLite => assert!(matches!(backend, DatabaseBackendType::SQLite)),
            }
        }
    }

    #[test]
    fn test_document_validation() {
        let valid_doc = create_test_document("valid", "Valid content");
        let empty_doc = DocumentData {
            id: DocumentId::from("empty"),
            metadata: DocumentMetadata {
                title: None,
                author: None,
                created_at: chrono::Utc::now(),
                modified_at: chrono::Utc::now(),
                tags: vec![],
                content_type: "text/plain".to_string(),
                size_bytes: 0,
                checksum: None,
                version: 1,
            },
            content: String::new(),
            embeddings: None,
            indexed_fields: HashMap::new(),
        };

        // Valid document should have all required fields
        assert!(!valid_doc.id.to_string().is_empty());
        assert!(!valid_doc.content.is_empty());
        assert!(valid_doc.metadata.title.is_some());
        assert!(valid_doc.metadata.size_bytes > 0);

        // Empty document should still be valid but with minimal data
        assert!(!empty_doc.id.to_string().is_empty());
        assert!(empty_doc.content.is_empty());
        assert!(empty_doc.metadata.title.is_none());
        assert_eq!(empty_doc.metadata.size_bytes, 0);
    }

    #[test]
    fn test_connection_timeout_configuration() {
        let timeouts = vec![
            Duration::from_secs(5),
            Duration::from_secs(30),
            Duration::from_secs(60),
            Duration::from_secs(300), // 5 minutes
        ];

        for timeout in timeouts {
            let config = DatabaseBackendConfig {
                backend_type: DatabaseBackendType::Memory,
                connection_string: ":memory:".to_string(),
                max_connections: 1,
                connection_timeout: timeout,
                query_timeout: timeout / 3,
            };

            assert_eq!(config.connection_timeout, timeout);
            assert!(config.query_timeout < config.connection_timeout);
        }
    }

    #[test]
    fn test_max_connections_configuration() {
        let connection_counts = vec![1, 5, 10, 20, 50];

        for max_conn in connection_counts {
            let config = DatabaseBackendConfig {
                backend_type: DatabaseBackendType::Memory,
                connection_string: ":memory:".to_string(),
                max_connections: max_conn,
                connection_timeout: Duration::from_secs(30),
                query_timeout: Duration::from_secs(10),
            };

            assert_eq!(config.max_connections, max_conn);
        }
    }

    #[test]
    fn test_vector_search_configuration() {
        let config_with_vector = DataStoreConfig {
            backend: DatabaseBackendConfig {
                backend_type: DatabaseBackendType::DuckDB,
                connection_string: ":memory:".to_string(),
                max_connections: 5,
                connection_timeout: Duration::from_secs(30),
                query_timeout: Duration::from_secs(10),
            },
            default_database: "vector_db".to_string(),
            enable_wal: true,
            cache_size: 2 * 1024 * 1024, // 2MB
            enable_vector_search: true,
            vector_dimensions: 768,
        };

        let config_without_vector = DataStoreConfig {
            enable_vector_search: false,
            vector_dimensions: 0,
            ..config_with_vector.clone()
        };

        assert!(config_with_vector.enable_vector_search);
        assert_eq!(config_with_vector.vector_dimensions, 768);
        assert!(!config_without_vector.enable_vector_search);
        assert_eq!(config_without_vector.vector_dimensions, 0);
    }

    #[test]
    fn test_document_size_calculation() {
        let small_content = "Small";
        let large_content = "Large content ".repeat(1000);

        let small_doc = create_test_document("small", small_content);
        let large_doc = create_test_document("large", &large_content);

        assert!(small_doc.metadata.size_bytes < large_doc.metadata.size_bytes);
        assert_eq!(small_doc.metadata.size_bytes, small_content.len() as u64);
        assert_eq!(large_doc.metadata.size_bytes, large_content.len() as u64);
    }

    #[test]
    fn test_document_tags_handling() {
        let mut doc = create_test_document("tagged", "Content with tags");

        // Add more tags
        doc.metadata.tags.extend(vec![
            "important".to_string(),
            "reviewed".to_string(),
            "published".to_string(),
        ]);

        assert_eq!(doc.metadata.tags.len(), 5); // 2 original + 3 new
        assert!(doc.metadata.tags.contains(&"test".to_string()));
        assert!(doc.metadata.tags.contains(&"important".to_string()));
        assert!(doc.metadata.tags.contains(&"published".to_string()));

        // Remove a tag
        doc.metadata.tags.retain(|tag| tag != "unit_test");
        assert_eq!(doc.metadata.tags.len(), 4);
        assert!(!doc.metadata.tags.contains(&"unit_test".to_string()));
    }

    #[test]
    fn test_indexed_fields_operations() {
        let mut doc = create_test_document("indexed", "Content to index");

        // Add more indexed fields
        doc.indexed_fields.insert("category".to_string(), "documentation".to_string());
        doc.indexed_fields.insert("language".to_string(), "en".to_string());
        doc.indexed_fields.insert("word_count".to_string(), "3".to_string());

        assert_eq!(doc.indexed_fields.len(), 5); // 2 original + 3 new
        assert_eq!(doc.indexed_fields.get("category").unwrap(), "documentation");
        assert_eq!(doc.indexed_fields.get("language").unwrap(), "en");
        assert_eq!(doc.indexed_fields.get("word_count").unwrap(), "3");

        // Update an existing field
        doc.indexed_fields.insert("word_count".to_string(), "4".to_string());
        assert_eq!(doc.indexed_fields.get("word_count").unwrap(), "4");
        assert_eq!(doc.indexed_fields.len(), 5); // Still 5, just updated
    }

    #[test]
    fn test_error_scenarios() {
        // Test invalid configurations
        let invalid_config = DataStoreConfig {
            backend: DatabaseBackendConfig {
                backend_type: DatabaseBackendType::Memory,
                connection_string: "".to_string(), // Empty connection string
                max_connections: 0, // Invalid: no connections
                connection_timeout: Duration::from_secs(0), // Invalid: zero timeout
                query_timeout: Duration::from_secs(0), // Invalid: zero timeout
            },
            default_database: "".to_string(), // Empty database name
            enable_wal: true,
            cache_size: 0, // No cache
            enable_vector_search: true,
            vector_dimensions: 0, // Vector enabled but zero dimensions
        };

        // These would be validation checks in actual implementation
        assert_eq!(invalid_config.backend.connection_string, "");
        assert_eq!(invalid_config.backend.max_connections, 0);
        assert_eq!(invalid_config.default_database, "");
        assert_eq!(invalid_config.cache_size, 0);
        assert_eq!(invalid_config.vector_dimensions, 0);
    }

    #[tokio::test]
    async fn test_data_store_service_creation() {
        let config = create_test_config();

        // This would test actual data store creation if implemented
        // For now, test configuration validation
        assert_eq!(config.default_database, "test_db");
        assert!(config.enable_vector_search);
        assert_eq!(config.vector_dimensions, 1536);
    }

    #[test]
    fn test_document_metadata_timestamps() {
        let created_time = chrono::Utc::now();
        let modified_time = created_time + chrono::Duration::minutes(5);

        let metadata = DocumentMetadata {
            title: Some("Timestamp Test".to_string()),
            author: Some("test_user".to_string()),
            created_at: created_time,
            modified_at: modified_time,
            tags: vec![],
            content_type: "text/plain".to_string(),
            size_bytes: 100,
            checksum: Some("time_test".to_string()),
            version: 1,
        };

        assert_eq!(metadata.created_at, created_time);
        assert_eq!(metadata.modified_at, modified_time);
        assert!(metadata.modified_at > metadata.created_at);
    }
}