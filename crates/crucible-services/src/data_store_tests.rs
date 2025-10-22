//! Unit tests for DataStore service
//!
//! This module provides comprehensive unit tests for the DataStore service,
//! covering all major functionality including CRUD operations, querying,
//! transactions, vector search, and multi-backend support.

use super::*;
use crate::events::routing::MockEventRouter;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

/// Mock database backend for testing
#[derive(Debug)]
struct MockDatabaseBackend {
    data: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    should_fail: bool,
}

#[async_trait]
impl crate::service_traits::DatabaseBackend for MockDatabaseBackend {
    async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn execute_query(&self, query: &str, params: &[serde_json::Value]) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        if self.should_fail {
            return Err("Mock database failure".into());
        }

        // Simple mock query execution
        let data = self.data.read().await;
        match query {
            q if q.contains("SELECT") => Ok(data.values().cloned().collect()),
            q if q.contains("INSERT") => Ok(vec![serde_json::json!({"affected_rows": 1})]),
            q if q.contains("UPDATE") => Ok(vec![serde_json::json!({"affected_rows": 1})]),
            q if q.contains("DELETE") => Ok(vec![serde_json::json!({"affected_rows": 1})]),
            _ => Ok(vec![]),
        }
    }

    async fn execute_transaction(&self, queries: &[(String, Vec<serde_json::Value>)]) -> Result<Vec<Vec<serde_json::Value>>, Box<dyn std::error::Error + Send + Sync>> {
        if self.should_fail {
            return Err("Mock transaction failure".into());
        }

        let mut results = Vec::new();
        for (query, params) in queries {
            let result = self.execute_query(query, params).await?;
            results.push(result);
        }
        Ok(results)
    }

    async fn begin_transaction(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(format!("tx_{}", Uuid::new_v4()))
    }

    async fn commit_transaction(&self, transaction_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Mock commit
        Ok(())
    }

    async fn rollback_transaction(&self, transaction_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Mock rollback
        Ok(())
    }

    async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(!self.should_fail)
    }

    async fn get_connection_info(&self) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        Ok(serde_json::json!({
            "backend": "mock",
            "connected": !self.should_fail,
            "version": "1.0.0"
        }))
    }
}

/// Create a test data store with memory backend
async fn create_test_data_store() -> DataStoreService {
    let config = create_test_data_store_config();
    DataStoreService::new(config).await.unwrap()
}

/// Create a test configuration
fn create_test_data_store_config() -> DataStoreConfig {
    DataStoreConfig {
        backend: DatabaseBackend::Memory,
        database_config: DatabaseBackendConfig::Memory(MemoryConfig {
            max_documents: Some(1000),
            persist_to_disk: Some(false),
            persistence_path: None,
        }),
        connection_pool: ConnectionPoolConfig {
            max_connections: 10,
            min_connections: 1,
            connection_timeout_seconds: 30,
            idle_timeout_seconds: 300,
            max_lifetime_seconds: Some(3600),
        },
        performance: PerformanceConfig {
            batch_size: 100,
            query_timeout_seconds: 30,
            enable_query_cache: true,
            cache_size_limit: Some(1000),
            enable_parallel_queries: false,
            max_parallel_workers: Some(4),
        },
        events: EventConfig {
            enabled: true,
            batch_size: 10,
            flush_interval_ms: 100,
            async_publishing: true,
        },
    }
}

/// Create a test document
fn create_test_document(id: &str, title: &str, content: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "title": title,
        "content": content,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "tags": ["test", "example"],
        "metadata": {
            "version": 1,
            "author": "test"
        }
    })
}

/// Create a test query
fn create_test_query() -> Query {
    Query {
        select: Some(vec!["id".to_string(), "title".to_string(), "content".to_string()]),
        from: Some("documents".to_string()),
        where_clause: Some("tags LIKE '%test%'".to_string()),
        order_by: Some(vec![OrderBy {
            column: "created_at".to_string(),
            direction: SortDirection::Desc,
        }]),
        limit: Some(10),
        offset: Some(0),
        group_by: None,
        having: None,
        params: HashMap::new(),
    }
}

/// Create a test vector for embeddings
fn create_test_vector() -> Vec<f32> {
    (0..1536).map(|i| (i as f32) / 1536.0).collect()
}

#[cfg(test)]
mod data_store_lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn test_data_store_creation() {
        let config = create_test_data_store_config();
        let store = DataStoreService::new(config).await;
        assert!(store.is_ok());
    }

    #[tokio::test]
    async fn test_service_lifecycle_start_stop() {
        let mut store = create_test_data_store().await;

        // Initially not running
        assert!(!store.is_running());

        // Start the service
        store.start().await.unwrap();
        assert!(store.is_running());

        // Starting again should not cause issues (idempotent)
        store.start().await.unwrap();
        assert!(store.is_running());

        // Stop the service
        store.stop().await.unwrap();
        assert!(!store.is_running());

        // Stopping again should not cause issues (idempotent)
        store.stop().await.unwrap();
        assert!(!store.is_running());
    }

    #[tokio::test]
    async fn test_service_restart() {
        let mut store = create_test_data_store().await;

        // Restart when not running
        store.restart().await.unwrap();
        assert!(store.is_running());

        // Restart when running
        store.restart().await.unwrap();
        assert!(store.is_running());
    }

    #[tokio::test]
    async fn test_service_metadata() {
        let store = create_test_data_store().await;

        assert_eq!(store.service_name(), "data-store");
        assert_eq!(store.service_version(), "1.0.0");
    }
}

#[cfg(test)]
mod data_store_health_tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_not_running() {
        let store = create_test_data_store().await;

        let health = store.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Unhealthy));
        assert!(health.message.is_some());
    }

    #[tokio::test]
    async fn test_health_check_running() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let health = store.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Healthy));
        assert!(health.message.is_some());

        // Check expected details
        assert!(health.details.contains_key("backend"));
        assert!(health.details.contains_key("active_connections"));
        assert!(health.details.contains_key("total_operations"));
        assert!(health.details.contains_key("success_rate"));
    }

    #[tokio::test]
    async fn test_backend_health_check() {
        let store = create_test_data_store().await;

        let health = store.check_backend_health().await.unwrap();
        assert!(health.is_healthy);
        assert!(!health.backend_type.is_empty());
        assert!(health.connection_info.is_some());
    }
}

#[cfg(test)]
mod data_store_configuration_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_configuration() {
        let store = create_test_data_store().await;
        let config = store.get_config().await.unwrap();

        // Should return the configuration we provided
        assert_eq!(config.backend, DatabaseBackend::Memory);
        assert_eq!(config.connection_pool.max_connections, 10);
        assert!(config.performance.enable_query_cache);
        assert!(config.events.enabled);
    }

    #[tokio::test]
    async fn test_update_configuration() {
        let mut store = create_test_data_store().await;

        let mut new_config = create_test_data_store_config();
        new_config.connection_pool.max_connections = 20;
        new_config.performance.enable_query_cache = false;
        new_config.events.enabled = false;

        store.update_config(new_config.clone()).await.unwrap();
        let retrieved_config = store.get_config().await.unwrap();

        assert_eq!(retrieved_config.connection_pool.max_connections, 20);
        assert!(!retrieved_config.performance.enable_query_cache);
        assert!(!retrieved_config.events.enabled);
    }

    #[tokio::test]
    async fn test_validate_configuration_valid() {
        let store = create_test_data_store().await;

        let valid_config = create_test_data_store_config();
        let result = store.validate_config(&valid_config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_configuration_invalid() {
        let store = create_test_data_store().await;

        let mut invalid_config = create_test_data_store_config();
        invalid_config.connection_pool.max_connections = 0; // Invalid: must be > 0

        let result = store.validate_config(&invalid_config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_reload_configuration() {
        let mut store = create_test_data_store().await;

        // Reload should succeed (even if it's a no-op)
        let result = store.reload_config().await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod data_store_crud_tests {
    use super::*;

    #[tokio::test]
    async fn test_create_document() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let document = create_test_document("doc1", "Test Document", "Test content");
        let result = store.create_document("documents", document.clone()).await;

        assert!(result.is_ok());
        let created_id = result.unwrap();
        assert_eq!(created_id, "doc1");
    }

    #[tokio::test]
    async fn test_create_document_with_auto_id() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let mut document = create_test_document("", "Auto ID Document", "Test content");
        document.as_object_mut().unwrap().remove("id");

        let result = store.create_document("documents", document).await;
        assert!(result.is_ok());

        let created_id = result.unwrap();
        assert!(!created_id.is_empty());
        assert_ne!(created_id, ""); // Should have a generated ID
    }

    #[tokio::test]
    async fn test_get_document() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let document = create_test_document("doc1", "Test Document", "Test content");
        store.create_document("documents", document.clone()).await.unwrap();

        let result = store.get_document("documents", "doc1").await;
        assert!(result.is_ok());

        let retrieved = result.unwrap();
        assert!(retrieved.is_some());

        let retrieved_doc = retrieved.unwrap();
        assert_eq!(retrieved_doc.get("id").unwrap(), "doc1");
        assert_eq!(retrieved_doc.get("title").unwrap(), "Test Document");
    }

    #[tokio::test]
    async fn test_get_non_existent_document() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let result = store.get_document("documents", "non_existent").await;
        assert!(result.is_ok());

        let retrieved = result.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_update_document() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let document = create_test_document("doc1", "Original Title", "Original content");
        store.create_document("documents", document).await.unwrap();

        let mut updated_doc = create_test_document("doc1", "Updated Title", "Updated content");
        updated_doc.as_object_mut().unwrap().insert("version".to_string(), serde_json::Value::Number(2.into()));

        let result = store.update_document("documents", "doc1", updated_doc).await;
        assert!(result.is_ok());

        let retrieved = store.get_document("documents", "doc1").await.unwrap().unwrap();
        assert_eq!(retrieved.get("title").unwrap(), "Updated Title");
        assert_eq!(retrieved.get("content").unwrap(), "Updated content");
    }

    #[tokio::test]
    async fn test_update_non_existent_document() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let document = create_test_document("doc1", "Updated Title", "Updated content");
        let result = store.update_document("documents", "non_existent", document).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_document() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let document = create_test_document("doc1", "Test Document", "Test content");
        store.create_document("documents", document).await.unwrap();

        let result = store.delete_document("documents", "doc1").await;
        assert!(result.is_ok());

        // Verify document is deleted
        let retrieved = store.get_document("documents", "doc1").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_delete_non_existent_document() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let result = store.delete_document("documents", "non_existent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_documents() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Create multiple documents
        for i in 1..=5 {
            let document = create_test_document(
                &format!("doc{}", i),
                &format!("Document {}", i),
                &format!("Content {}", i),
            );
            store.create_document("documents", document).await.unwrap();
        }

        let result = store.list_documents("documents", None, None).await;
        assert!(result.is_ok());

        let documents = result.unwrap();
        assert_eq!(documents.len(), 5);
    }

    #[tokio::test]
    async fn test_list_documents_with_pagination() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Create multiple documents
        for i in 1..=10 {
            let document = create_test_document(
                &format!("doc{}", i),
                &format!("Document {}", i),
                &format!("Content {}", i),
            );
            store.create_document("documents", document).await.unwrap();
        }

        // Get first page
        let result = store.list_documents("documents", Some(5), Some(0)).await;
        assert!(result.is_ok());
        let first_page = result.unwrap();
        assert_eq!(first_page.len(), 5);

        // Get second page
        let result = store.list_documents("documents", Some(5), Some(5)).await;
        assert!(result.is_ok());
        let second_page = result.unwrap();
        assert_eq!(second_page.len(), 5);
    }
}

#[cfg(test)]
mod data_store_query_tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_simple_query() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Create test documents
        let doc1 = create_test_document("doc1", "Test Document 1", "Content about testing");
        let doc2 = create_test_document("doc2", "Another Document", "Different content");
        let doc3 = create_test_document("doc3", "Test Document 2", "More testing content");

        store.create_document("documents", doc1).await.unwrap();
        store.create_document("documents", doc2).await.unwrap();
        store.create_document("documents", doc3).await.unwrap();

        let query = create_test_query();
        let result = store.execute_query(query).await;

        assert!(result.is_ok());
        let results = result.unwrap();
        assert!(results.len() >= 2); // At least the documents with "test" in tags
    }

    #[tokio::test]
    async fn test_execute_query_with_parameters() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let document = create_test_document("doc1", "Parameter Test", "Testing with parameters");
        store.create_document("documents", document).await.unwrap();

        let mut query = create_test_query();
        query.params.insert("title".to_string(), serde_json::Value::String("Parameter Test".to_string()));

        let result = store.execute_query(query).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_aggregate_query() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Create test documents
        for i in 1..=5 {
            let document = create_test_document(
                &format!("doc{}", i),
                &format!("Document {}", i),
                &format!("Content {}", i),
            );
            store.create_document("documents", document).await.unwrap();
        }

        let query = Query {
            select: Some(vec!["COUNT(*) as count".to_string()]),
            from: Some("documents".to_string()),
            where_clause: None,
            order_by: None,
            limit: None,
            offset: None,
            group_by: None,
            having: None,
            params: HashMap::new(),
        };

        let result = store.execute_query(query).await;
        assert!(result.is_ok());

        let results = result.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_execute_complex_query() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Create test documents with different metadata
        for i in 1..=10 {
            let mut document = create_test_document(
                &format!("doc{}", i),
                &format!("Document {}", i),
                &format!("Content {}", i),
            );

            // Add some documents with higher version numbers
            if i > 5 {
                document.as_object_mut().unwrap().insert("version".to_string(), serde_json::Value::Number(2.into()));
            }

            store.create_document("documents", document).await.unwrap();
        }

        let query = Query {
            select: Some(vec!["id".to_string(), "title".to_string(), "version".to_string()]),
            from: Some("documents".to_string()),
            where_clause: Some("version >= 2".to_string()),
            order_by: Some(vec![OrderBy {
                column: "created_at".to_string(),
                direction: SortDirection::Desc,
            }]),
            limit: Some(5),
            offset: Some(0),
            group_by: None,
            having: None,
            params: HashMap::new(),
        };

        let result = store.execute_query(query).await;
        assert!(result.is_ok());

        let results = result.unwrap();
        assert!(results.len() <= 5);
    }
}

#[cfg(test)]
mod data_store_transaction_tests {
    use super::*;

    #[tokio::test]
    async fn test_begin_commit_transaction() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let transaction_id = store.begin_transaction().await.unwrap();
        assert!(!transaction_id.is_empty());

        // Perform operations within transaction
        let document = create_test_document("tx_doc1", "Transaction Document", "Content");
        let _result = store.create_document_in_transaction("documents", document, &transaction_id).await.unwrap();

        // Commit transaction
        store.commit_transaction(&transaction_id).await.unwrap();

        // Verify document exists after commit
        let result = store.get_document("documents", "tx_doc1").await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_begin_rollback_transaction() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let transaction_id = store.begin_transaction().await.unwrap();
        assert!(!transaction_id.is_empty());

        // Perform operations within transaction
        let document = create_test_document("tx_doc2", "Rollback Document", "Content");
        let _result = store.create_document_in_transaction("documents", document, &transaction_id).await.unwrap();

        // Rollback transaction
        store.rollback_transaction(&transaction_id).await.unwrap();

        // Verify document doesn't exist after rollback
        let result = store.get_document("documents", "tx_doc2").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_multiple_operations_in_transaction() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let transaction_id = store.begin_transaction().await.unwrap();

        // Create multiple documents
        for i in 1..=3 {
            let document = create_test_document(
                &format!("tx_multi_{}", i),
                &format!("Multi Transaction {}", i),
                &format!("Content {}", i),
            );
            store.create_document_in_transaction("documents", document, &transaction_id).await.unwrap();
        }

        // Commit transaction
        store.commit_transaction(&transaction_id).await.unwrap();

        // Verify all documents exist
        for i in 1..=3 {
            let result = store.get_document("documents", &format!("tx_multi_{}", i)).await.unwrap();
            assert!(result.is_some());
        }
    }

    #[tokio::test]
    async fn test_transaction_isolation() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Create document outside transaction
        let outside_doc = create_test_document("outside_doc", "Outside Document", "Content");
        store.create_document("documents", outside_doc).await.unwrap();

        let transaction_id = store.begin_transaction().await.unwrap();

        // Create document in transaction
        let inside_doc = create_test_document("inside_doc", "Inside Document", "Content");
        store.create_document_in_transaction("documents", inside_doc, &transaction_id).await.unwrap();

        // Outside transaction should only see outside document
        let all_docs = store.list_documents("documents", None, None).await.unwrap();
        assert_eq!(all_docs.len(), 1);

        // Commit transaction
        store.commit_transaction(&transaction_id).await.unwrap();

        // Now should see both documents
        let all_docs = store.list_documents("documents", None, None).await.unwrap();
        assert_eq!(all_docs.len(), 2);
    }
}

#[cfg(test)]
mod data_store_vector_tests {
    use super::*;

    #[tokio::test]
    async fn test_create_vector_index() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let index_config = VectorIndexConfig {
            name: "test_index".to_string(),
            collection: "documents".to_string(),
            vector_field: "embedding".to_string(),
            distance_metric: DistanceMetric::Cosine,
            dimensions: 1536,
            index_type: VectorIndexType::HNSW,
            parameters: HashMap::new(),
        };

        let result = store.create_vector_index(index_config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_insert_vector() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let vector = create_test_vector();
        let metadata = serde_json::json!({
            "title": "Vector Document",
            "content": "Document with vector embedding"
        });

        let result = store.insert_vector("documents", "vec_doc1", vector, Some(metadata)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_vectors() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Insert test vectors
        for i in 1..=5 {
            let mut vector = create_test_vector();
            // Modify vector slightly for each document
            for j in 0..vector.len() {
                vector[j] += (i as f32) * 0.01;
            }

            let metadata = serde_json::json!({
                "title": format!("Vector Document {}", i),
                "content": format!("Content with vector embedding {}", i)
            });

            store.insert_vector("documents", &format!("vec_doc{}", i), vector, Some(metadata)).await.unwrap();
        }

        // Search for similar vectors
        let query_vector = create_test_vector();
        let search_request = VectorSearchRequest {
            collection: "documents".to_string(),
            query_vector,
            top_k: 3,
            threshold: Some(0.8),
            filter: None,
            include_metadata: true,
        };

        let result = store.search_vectors(search_request).await;
        assert!(result.is_ok());

        let search_results = result.unwrap();
        assert!(search_results.len() <= 3);

        for result in search_results {
            assert!(!result.id.is_empty());
            assert!(result.score >= 0.0);
            assert!(result.score <= 1.0);
            assert!(result.metadata.is_some());
        }
    }

    #[tokio::test]
    async fn test_search_vectors_with_filter() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Insert vectors with different categories
        let categories = vec!["technology", "science", "art"];
        for (i, category) in categories.iter().enumerate() {
            let mut vector = create_test_vector();
            for j in 0..vector.len() {
                vector[j] += (i as f32) * 0.01;
            }

            let metadata = serde_json::json!({
                "title": format!("Document {}", i),
                "category": category,
                "content": format!("Content in category {}", category)
            });

            store.insert_vector("documents", &format!("cat_doc_{}", i), vector, Some(metadata)).await.unwrap();
        }

        // Search with category filter
        let query_vector = create_test_vector();
        let filter = VectorSearchFilter::Eq("category".to_string(), serde_json::Value::String("technology".to_string()));

        let search_request = VectorSearchRequest {
            collection: "documents".to_string(),
            query_vector,
            top_k: 5,
            threshold: None,
            filter: Some(filter),
            include_metadata: true,
        };

        let result = store.search_vectors(search_request).await;
        assert!(result.is_ok());

        let search_results = result.unwrap();
        for result in search_results {
            let metadata = result.metadata.unwrap();
            let category = metadata.get("category").unwrap().as_str().unwrap();
            assert_eq!(category, "technology");
        }
    }

    #[tokio::test]
    async fn test_delete_vector() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let vector = create_test_vector();
        store.insert_vector("documents", "vec_to_delete", vector, None).await.unwrap();

        // Delete the vector
        let result = store.delete_vector("documents", "vec_to_delete").await;
        assert!(result.is_ok());

        // Verify vector is deleted by searching
        let query_vector = create_test_vector();
        let search_request = VectorSearchRequest {
            collection: "documents".to_string(),
            query_vector,
            top_k: 1,
            threshold: None,
            filter: None,
            include_metadata: false,
        };

        let result = store.search_vectors(search_request).await;
        assert!(result.is_ok());

        let search_results = result.unwrap();
        assert!(search_results.is_empty()); // Should be empty since we deleted the vector
    }
}

#[cfg(test)]
mod data_store_batch_tests {
    use super::*;

    #[tokio::test]
    async fn test_batch_insert() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let mut documents = Vec::new();
        for i in 1..=5 {
            let document = create_test_document(
                &format!("batch_doc_{}", i),
                &format!("Batch Document {}", i),
                &format!("Batch Content {}", i),
            );
            documents.push(document);
        }

        let result = store.batch_insert("documents", documents).await;
        assert!(result.is_ok());

        let inserted_ids = result.unwrap();
        assert_eq!(inserted_ids.len(), 5);

        // Verify all documents were inserted
        for i in 1..=5 {
            let doc = store.get_document("documents", &format!("batch_doc_{}", i)).await.unwrap();
            assert!(doc.is_some());
        }
    }

    #[tokio::test]
    async fn test_batch_update() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Create initial documents
        for i in 1..=3 {
            let document = create_test_document(
                &format!("update_doc_{}", i),
                &format!("Original Title {}", i),
                &format!("Original Content {}", i),
            );
            store.create_document("documents", document).await.unwrap();
        }

        // Batch update documents
        let mut updates = Vec::new();
        for i in 1..=3 {
            let updated_doc = create_test_document(
                &format!("update_doc_{}", i),
                &format!("Updated Title {}", i),
                &format!("Updated Content {}", i),
            );
            updates.push((format!("update_doc_{}", i), updated_doc));
        }

        let result = store.batch_update("documents", updates).await;
        assert!(result.is_ok());

        let updated_count = result.unwrap();
        assert_eq!(updated_count, 3);

        // Verify all documents were updated
        for i in 1..=3 {
            let doc = store.get_document("documents", &format!("update_doc_{}", i)).await.unwrap().unwrap();
            assert_eq!(doc.get("title").unwrap(), &format!("Updated Title {}", i));
        }
    }

    #[tokio::test]
    async fn test_batch_delete() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Create documents to delete
        for i in 1..=3 {
            let document = create_test_document(
                &format!("delete_doc_{}", i),
                &format!("Delete Document {}", i),
                &format!("Delete Content {}", i),
            );
            store.create_document("documents", document).await.unwrap();
        }

        let ids_to_delete = vec!["delete_doc_1".to_string(), "delete_doc_2".to_string(), "delete_doc_3".to_string()];
        let result = store.batch_delete("documents", ids_to_delete).await;
        assert!(result.is_ok());

        let deleted_count = result.unwrap();
        assert_eq!(deleted_count, 3);

        // Verify all documents were deleted
        for i in 1..=3 {
            let doc = store.get_document("documents", &format!("delete_doc_{}", i)).await.unwrap();
            assert!(doc.is_none());
        }
    }
}

#[cfg(test)]
mod data_store_index_tests {
    use super::*;

    #[tokio::test]
    async fn test_create_index() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let index_config = IndexConfig {
            name: "test_title_index".to_string(),
            collection: "documents".to_string(),
            fields: vec!["title".to_string()],
            index_type: IndexType::BTree,
            unique: false,
            sparse: false,
        };

        let result = store.create_index(index_config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_indexes() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Create multiple indexes
        let title_index = IndexConfig {
            name: "title_index".to_string(),
            collection: "documents".to_string(),
            fields: vec!["title".to_string()],
            index_type: IndexType::BTree,
            unique: false,
            sparse: false,
        };

        let content_index = IndexConfig {
            name: "content_index".to_string(),
            collection: "documents".to_string(),
            fields: vec!["content".to_string()],
            index_type: IndexType::FullText,
            unique: false,
            sparse: false,
        };

        store.create_index(title_index).await.unwrap();
        store.create_index(content_index).await.unwrap();

        let indexes = store.list_indexes("documents").await.unwrap();
        assert_eq!(indexes.len(), 2);

        let index_names: Vec<String> = indexes.iter().map(|i| i.name.clone()).collect();
        assert!(index_names.contains(&"title_index".to_string()));
        assert!(index_names.contains(&"content_index".to_string()));
    }

    #[tokio::test]
    async fn test_drop_index() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let index_config = IndexConfig {
            name: "index_to_drop".to_string(),
            collection: "documents".to_string(),
            fields: vec!["title".to_string()],
            index_type: IndexType::BTree,
            unique: false,
            sparse: false,
        };

        store.create_index(index_config).await.unwrap();

        let indexes = store.list_indexes("documents").await.unwrap();
        assert_eq!(indexes.len(), 1);

        store.drop_index("documents", "index_to_drop").await.unwrap();

        let indexes = store.list_indexes("documents").await.unwrap();
        assert_eq!(indexes.len(), 0);
    }
}

#[cfg(test)]
mod data_store_event_tests {
    use super::*;

    #[tokio::test]
    async fn test_event_subscription() {
        let mut store = create_test_data_store().await;

        let mut receiver = store.subscribe("document_created").await.unwrap();

        // Create a document to trigger an event
        let document = create_test_document("event_doc", "Event Document", "Content");
        store.start().await.unwrap();
        store.create_document("documents", document).await.unwrap();

        // Should receive a document created event
        let event = receiver.recv().await;
        assert!(event.is_some());

        if let Some(DataStoreEvent::DocumentCreated { collection, document_id, .. }) = event {
            assert_eq!(collection, "documents");
            assert_eq!(document_id, "event_doc");
        } else {
            panic!("Expected DocumentCreated event");
        }
    }

    #[tokio::test]
    async fn test_multiple_event_subscriptions() {
        let mut store = create_test_data_store().await;

        let mut created_rx = store.subscribe("document_created").await.unwrap();
        let mut updated_rx = store.subscribe("document_updated").await.unwrap();

        store.start().await.unwrap();

        // Create document
        let document = create_test_document("multi_event_doc", "Multi Event Document", "Content");
        store.create_document("documents", document.clone()).await.unwrap();

        // Update document
        let mut updated_doc = document.clone();
        updated_doc.as_object_mut().unwrap().insert("version".to_string(), serde_json::Value::Number(2.into()));
        store.update_document("documents", "multi_event_doc", updated_doc).await.unwrap();

        // Should receive created event
        let created_event = created_rx.recv().await;
        assert!(created_event.is_some());

        // Should receive updated event
        let updated_event = updated_rx.recv().await;
        assert!(updated_event.is_some());
    }

    #[tokio::test]
    async fn test_handle_data_store_event() {
        let mut store = create_test_data_store().await;

        let test_events = vec![
            DataStoreEvent::DocumentCreated {
                collection: "test".to_string(),
                document_id: "test_doc".to_string(),
                timestamp: chrono::Utc::now(),
            },
            DataStoreEvent::DocumentUpdated {
                collection: "test".to_string(),
                document_id: "test_doc".to_string(),
                timestamp: chrono::Utc::now(),
            },
            DataStoreEvent::DocumentDeleted {
                collection: "test".to_string(),
                document_id: "test_doc".to_string(),
                timestamp: chrono::Utc::now(),
            },
            DataStoreEvent::QueryExecuted {
                collection: "test".to_string(),
                query: "SELECT * FROM test".to_string(),
                execution_time_ms: 100,
                result_count: 5,
                timestamp: chrono::Utc::now(),
            },
            DataStoreEvent::IndexCreated {
                collection: "test".to_string(),
                index_name: "test_index".to_string(),
                fields: vec!["field1".to_string()],
                timestamp: chrono::Utc::now(),
            },
            DataStoreEvent::TransactionStarted {
                transaction_id: "tx_test".to_string(),
                timestamp: chrono::Utc::now(),
            },
            DataStoreEvent::TransactionCommitted {
                transaction_id: "tx_test".to_string(),
                duration_ms: 50,
                operations_count: 3,
                timestamp: chrono::Utc::now(),
            },
            DataStoreEvent::TransactionRolledBack {
                transaction_id: "tx_test".to_string(),
                duration_ms: 25,
                operations_count: 2,
                reason: "Test rollback".to_string(),
                timestamp: chrono::Utc::now(),
            },
            DataStoreEvent::Error {
                operation: "test_operation".to_string(),
                error: "Test error".to_string(),
                context: HashMap::new(),
                timestamp: chrono::Utc::now(),
            },
        ];

        for event in test_events {
            let result = store.handle_event(event.clone()).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_publish_event() {
        let mut store = create_test_data_store().await;

        let event = DataStoreEvent::DocumentCreated {
            collection: "test".to_string(),
            document_id: "test_doc".to_string(),
            timestamp: chrono::Utc::now(),
        };

        let result = store.publish(event).await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod data_store_metrics_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_initial_metrics() {
        let store = create_test_data_store().await;

        let metrics = store.get_metrics().await.unwrap();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.successful_requests, 0);
        assert_eq!(metrics.failed_requests, 0);
        assert_eq!(metrics.memory_usage, 0);
        assert_eq!(metrics.cpu_usage, 0.0);
    }

    #[tokio::test]
    async fn test_get_metrics_after_operations() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Perform some operations
        let document = create_test_document("metrics_doc", "Metrics Document", "Content");
        store.create_document("documents", document).await.unwrap();
        store.get_document("documents", "metrics_doc").await.unwrap();

        let metrics = store.get_metrics().await.unwrap();
        assert!(metrics.total_requests >= 2);
        assert!(metrics.successful_requests >= 2);
    }

    #[tokio::test]
    async fn test_reset_metrics() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Perform operations to generate metrics
        let document = create_test_document("reset_doc", "Reset Document", "Content");
        store.create_document("documents", document).await.unwrap();

        let metrics_before = store.get_metrics().await.unwrap();
        assert!(metrics_before.total_requests > 0);

        // Reset metrics
        store.reset_metrics().await.unwrap();

        let metrics_after = store.get_metrics().await.unwrap();
        assert_eq!(metrics_after.total_requests, 0);
        assert_eq!(metrics_after.successful_requests, 0);
        assert_eq!(metrics_after.failed_requests, 0);
    }

    #[tokio::test]
    async fn test_get_performance_metrics() {
        let store = create_test_data_store().await;

        let perf_metrics = store.get_performance_metrics().await.unwrap();
        assert_eq!(perf_metrics.active_connections, 0);
        assert_eq!(perf_metrics.memory_usage, 0);
        assert_eq!(perf_metrics.cpu_usage, 0.0);
        assert!(perf_metrics.custom_metrics.is_empty());
    }
}

#[cfg(test)]
mod data_store_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_end_to_end_workflow() {
        let mut store = create_test_data_store().await;

        // Start the service
        store.start().await.unwrap();
        assert!(store.is_running());

        // Check health
        let health = store.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Healthy));

        // Create documents
        let doc1 = create_test_document("e2e_doc1", "E2E Document 1", "Content 1");
        let doc2 = create_test_document("e2e_doc2", "E2E Document 2", "Content 2");

        store.create_document("documents", doc1).await.unwrap();
        store.create_document("documents", doc2).await.unwrap();

        // Query documents
        let query = create_test_query();
        let results = store.execute_query(query).await.unwrap();
        assert!(results.len() >= 2);

        // List documents
        let documents = store.list_documents("documents", None, None).await.unwrap();
        assert_eq!(documents.len(), 2);

        // Update document
        let mut updated_doc = create_test_document("e2e_doc1", "Updated E2E Document", "Updated Content");
        updated_doc.as_object_mut().unwrap().insert("version".to_string(), serde_json::Value::Number(2.into()));
        store.update_document("documents", "e2e_doc1", updated_doc).await.unwrap();

        // Verify update
        let retrieved = store.get_document("documents", "e2e_doc1").await.unwrap().unwrap();
        assert_eq!(retrieved.get("title").unwrap(), "Updated E2E Document");

        // Delete document
        store.delete_document("documents", "e2e_doc2").await.unwrap();

        // Verify deletion
        let documents = store.list_documents("documents", None, None).await.unwrap();
        assert_eq!(documents.len(), 1);

        // Check metrics
        let metrics = store.get_metrics().await.unwrap();
        assert!(metrics.total_requests > 0);
        assert!(metrics.successful_requests > 0);

        // Stop the service
        store.stop().await.unwrap();
        assert!(!store.is_running());
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let mut handles = vec![];

        // Launch multiple concurrent document creation operations
        for i in 0..10 {
            let store_clone = store.clone();
            let handle = tokio::spawn(async move {
                let document = create_test_document(
                    &format!("concurrent_doc_{}", i),
                    &format!("Concurrent Document {}", i),
                    &format!("Concurrent Content {}", i),
                );
                store_clone.create_document("documents", document).await
            });
            handles.push(handle);
        }

        // Wait for all operations to complete
        let mut successful = 0;
        for handle in handles {
            let result = handle.await.unwrap();
            if result.is_ok() {
                successful += 1;
            }
        }

        // Most or all should succeed
        assert!(successful >= 8);

        // Verify all documents were created
        let documents = store.list_documents("documents", None, None).await.unwrap();
        assert!(documents.len() >= 8);
    }

    #[tokio::test]
    async fn test_vector_search_workflow() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        // Create vector index
        let index_config = VectorIndexConfig {
            name: "test_search_index".to_string(),
            collection: "documents".to_string(),
            vector_field: "embedding".to_string(),
            distance_metric: DistanceMetric::Cosine,
            dimensions: 1536,
            index_type: VectorIndexType::HNSW,
            parameters: HashMap::new(),
        };
        store.create_vector_index(index_config).await.unwrap();

        // Insert documents with vectors
        for i in 1..=5 {
            let mut vector = create_test_vector();
            // Modify vector slightly for each document
            for j in 0..vector.len() {
                vector[j] += (i as f32) * 0.01;
            }

            let metadata = serde_json::json!({
                "title": format!("Search Document {}", i),
                "content": format!("Content for search {}", i)
            });

            store.insert_vector("documents", &format!("search_doc_{}", i), vector, Some(metadata)).await.unwrap();
        }

        // Perform vector search
        let query_vector = create_test_vector();
        let search_request = VectorSearchRequest {
            collection: "documents".to_string(),
            query_vector,
            top_k: 3,
            threshold: Some(0.8),
            filter: None,
            include_metadata: true,
        };

        let results = store.search_vectors(search_request).await.unwrap();
        assert!(results.len() <= 3);

        for result in results {
            assert!(!result.id.is_empty());
            assert!(result.score >= 0.0);
            assert!(result.metadata.is_some());

            let metadata = result.metadata.unwrap();
            let title = metadata.get("title").unwrap().as_str().unwrap();
            assert!(title.contains("Search Document"));
        }
    }

    #[tokio::test]
    async fn test_transaction_workflow() {
        let mut store = create_test_data_store().await;
        store.start().await.unwrap();

        let transaction_id = store.begin_transaction().await.unwrap();

        // Create multiple documents in transaction
        for i in 1..=3 {
            let document = create_test_document(
                &format!("tx_workflow_doc_{}", i),
                &format!("Transaction Workflow {}", i),
                &format!("Transaction Content {}", i),
            );
            store.create_document_in_transaction("documents", document, &transaction_id).await.unwrap();
        }

        // Query within transaction (should see the new documents)
        let query = Query {
            select: Some(vec!["*".to_string()]),
            from: Some("documents".to_string()),
            where_clause: Some("id LIKE 'tx_workflow_doc_%'".to_string()),
            order_by: None,
            limit: None,
            offset: None,
            group_by: None,
            having: None,
            params: HashMap::new(),
        };

        let results = store.execute_query_in_transaction(query, &transaction_id).await.unwrap();
        assert_eq!(results.len(), 3);

        // Commit transaction
        store.commit_transaction(&transaction_id).await.unwrap();

        // Verify documents exist after commit
        let documents = store.list_documents("documents", None, None).await.unwrap();
        assert_eq!(documents.len(), 3);
    }
}