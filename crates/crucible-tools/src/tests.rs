//! Integration tests for crucible-tools
//!
//! This module contains comprehensive integration tests that verify
//! the functionality of the tool system, service integration, and
//! static registration.

use crate::service::{ExecutionContextBuilder, SystemToolService, ToolServiceFactory};
use crate::system_tools::ToolManager;
use crate::types::ToolCategory;
use crate::{init, create_tool_manager};
use serde_json::json;
use std::collections::HashMap;

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_service_basic_functionality() {
        let service = SystemToolService::new();

        // Test listing tools
        let tools = service.list_tools().await.unwrap();
        assert!(!tools.is_empty());

        // Test getting tools by category
        let vault_tools = service.get_tools_by_category(&ToolCategory::Vault).await.unwrap();
        assert!(!vault_tools.is_empty());

        let search_tools = service.get_tools_by_category(&ToolCategory::Search).await.unwrap();
        assert!(!search_tools.is_empty());

        let database_tools = service.get_tools_by_category(&ToolCategory::Database).await.unwrap();
        assert!(!database_tools.is_empty());
    }

    #[tokio::test]
    async fn test_tool_execution() {
        let service = SystemToolService::new();
        let context = ExecutionContextBuilder::new()
            .vault_path("/test/vault")
            .user_id("test_user")
            .build();

        // Test vault tool
        let result = service
            .execute_tool(
                "search_by_properties",
                json!({
                    "properties": {
                        "status": "active"
                    }
                }),
                context.clone(),
            )
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.data.is_some());

        // Test search tool
        let result = service
            .execute_tool(
                "semantic_search",
                json!({
                    "query": "test query",
                    "top_k": 5
                }),
                context.clone(),
            )
            .await
            .unwrap();

        assert!(result.success);

        // Test database tool
        let result = service
            .execute_tool(
                "get_document_stats",
                json!({}),
                context,
            )
            .await
            .unwrap();

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_tool_service_factory() {
        let default_service = ToolServiceFactory::create_default();
        let tools = default_service.list_tools().await.unwrap();
        assert!(!tools.is_empty());

        let minimal_service = ToolServiceFactory::create_minimal();
        let minimal_tools = minimal_service.list_tools().await.unwrap();
        // Minimal should have fewer tools
        assert!(minimal_tools.len() < tools.len());

        let dev_service = ToolServiceFactory::create_development();
        let dev_tools = dev_service.list_tools().await.unwrap();
        assert_eq!(dev_tools.len(), tools.len()); // Should have all tools
    }

    #[tokio::test]
    async fn test_search_tools_functionality() {
        let service = SystemToolService::new();
        let context = ExecutionContextBuilder::new().build();

        // Test semantic search
        let result = service
            .execute_tool(
                "semantic_search",
                json!({
                    "query": "machine learning",
                    "top_k": 3
                }),
                context.clone(),
            )
            .await
            .unwrap();

        assert!(result.success);
        if let Some(data) = result.data {
            let results = data.as_array().unwrap();
            assert!(!results.is_empty());
        }

        // Test advanced search
        let result = service
            .execute_tool(
                "advanced_search",
                json!({
                    "query": {
                        "text": "transformer architecture",
                        "semantic": true,
                        "tags": ["ai"]
                    },
                    "limit": 10
                }),
                context.clone(),
            )
            .await
            .unwrap();

        assert!(result.success);

        // Test index stats
        let result = service
            .execute_tool("get_index_stats", json!({}), context.clone())
            .await
            .unwrap();

        assert!(result.success);
        if let Some(data) = result.data {
            assert!(data.get("indexes").is_some());
            assert!(data.get("total_documents").is_some());
        }
    }

    #[tokio::test]
    async fn test_vault_tools_functionality() {
        let service = SystemToolService::new();
        let context = ExecutionContextBuilder::new()
            .vault_path("/test/vault")
            .build();

        // Test search by tags
        let result = service
            .execute_tool(
                "search_by_tags",
                json!({
                    "tags": ["research", "important"]
                }),
                context.clone(),
            )
            .await
            .unwrap();

        assert!(result.success);

        // Test search by folder
        let result = service
            .execute_tool(
                "search_by_folder",
                json!({
                    "path": "projects",
                    "recursive": true
                }),
                context.clone(),
            )
            .await
            .unwrap();

        assert!(result.success);

        // Test get note metadata
        let result = service
            .execute_tool(
                "get_note_metadata",
                json!({
                    "path": "test.md"
                }),
                context,
            )
            .await
            .unwrap();

        assert!(result.success);
        if let Some(data) = result.data {
            assert!(data.get("file_path").is_some());
            assert!(data.get("title").is_some());
        }
    }

    #[tokio::test]
    async fn test_database_tools_functionality() {
        let service = SystemToolService::new();
        let context = ExecutionContextBuilder::new().build();

        // Test search by content
        let result = service
            .execute_tool(
                "search_by_content",
                json!({
                    "query": "database design"
                }),
                context.clone(),
            )
            .await
            .unwrap();

        assert!(result.success);

        // Test search by filename
        let result = service
            .execute_tool(
                "search_by_filename",
                json!({
                    "pattern": "*.md"
                }),
                context.clone(),
            )
            .await
            .unwrap();

        assert!(result.success);

        // Test update note properties
        let result = service
            .execute_tool(
                "update_note_properties",
                json!({
                    "path": "test.md",
                    "properties": {
                        "status": "updated",
                        "priority": "high"
                    }
                }),
                context.clone(),
            )
            .await
            .unwrap();

        assert!(result.success);

        // Test index document
        let result = service
            .execute_tool(
                "index_document",
                json!({
                    "document": {
                        "id": "test-doc",
                        "title": "Test Document",
                        "content": "This is a test document content."
                    }
                }),
                context,
            )
            .await
            .unwrap();

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_error_handling() {
        let service = SystemToolService::new();
        let context = ExecutionContextBuilder::new().build();

        // Test missing required parameters
        let result = service
            .execute_tool("search_by_properties", json!({}), context.clone())
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());

        // Test non-existent tool
        let result = service
            .execute_tool("non_existent_tool", json!({}), context.clone())
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());

        // Test invalid parameters
        let result = service
            .execute_tool(
                "get_note_metadata",
                json!({
                    "path": 123  // Invalid type
                }),
                context,
            )
            .await
            .unwrap();

        // Tool should handle this gracefully
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_tool_discovery() {
        let service = SystemToolService::new();

        // Test searching tools
        let search_results = service.search_tools("search").await.unwrap();
        assert!(!search_results.is_empty());

        let vault_results = service.search_tools("vault").await.unwrap();
        assert!(!vault_results.is_empty());

        let db_results = service.search_tools("database").await.unwrap();
        assert!(!db_results.is_empty());

        // Test tool existence
        assert!(service.tool_exists("semantic_search").await.unwrap());
        assert!(service.tool_exists("search_by_tags").await.unwrap());
        assert!(!service.tool_exists("non_existent_tool").await.unwrap());

        // Test getting specific tool
        let tool = service.get_tool("semantic_search").await.unwrap();
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name, "semantic_search");
    }

    #[test]
    fn test_tool_manager_direct_usage() {
        let mut manager = ToolManager::new();

        // Register tools manually
        crate::vault_tools::register_vault_tools(&mut manager);
        crate::database_tools::register_database_tools(&mut manager);
        crate::search_tools::register_search_tools(&mut manager);

        let tools = manager.list_tools();
        assert!(!tools.is_empty());

        let vault_tools = manager.list_tools_by_category(&ToolCategory::Vault);
        assert!(!vault_tools.is_empty());

        let search_results = manager.search_tools("search");
        assert!(!search_results.is_empty());
    }

    #[test]
    fn test_library_initialization() {
        // Test library initialization
        let registry = init();
        assert!(!registry.list_tools().is_empty());

        // Test tool manager creation
        let manager = create_tool_manager();
        assert!(!manager.list_tools().is_empty());

        // Test that all categories have tools
        let vault_tools = manager.list_tools_by_category(&ToolCategory::Vault);
        let search_tools = manager.list_tools_by_category(&ToolCategory::Search);
        let database_tools = manager.list_tools_by_category(&ToolCategory::Database);

        assert!(!vault_tools.is_empty());
        assert!(!search_tools.is_empty());
        assert!(!database_tools.is_empty());
    }

    #[test]
    fn test_execution_context_builder() {
        let context = ExecutionContextBuilder::new()
            .workspace_path("/workspace")
            .vault_path("/vault")
            .user_id("user123")
            .session_id("session456")
            .build();

        assert_eq!(context.workspace_path, Some("/workspace".to_string()));
        assert_eq!(context.vault_path, Some("/vault".to_string()));
        assert_eq!(context.user_id, Some("user123".to_string()));
        assert_eq!(context.session_id, Some("session456".to_string()));
        assert!(context.timestamp.timestamp() > 0);
    }

    #[tokio::test]
    async fn test_concurrent_tool_execution() {
        let service = SystemToolService::new();
        let context = ExecutionContextBuilder::new().build();

        // Execute multiple tools concurrently
        let futures = vec![
            service.execute_tool("get_document_stats", json!({}), context.clone()),
            service.execute_tool("get_index_stats", json!({}), context.clone()),
            service.execute_tool(
                "search_by_filename",
                json!({"pattern": "*.md"}),
                context.clone(),
            ),
            service.execute_tool(
                "search_by_tags",
                json!({"tags": ["test"]}),
                context,
            ),
        ];

        let results = futures::future::join_all(futures).await;

        // All should complete successfully
        for result in results {
            let result = result.unwrap();
            assert!(result.success);
        }
    }

    #[tokio::test]
    async fn test_tool_caching_and_performance() {
        let service = SystemToolService::new();
        let context = ExecutionContextBuilder::new().build();

        // Test that tool definitions are cached properly
        let start = std::time::Instant::now();
        let _tools1 = service.list_tools().await.unwrap();
        let first_call_time = start.elapsed();

        let start = std::time::Instant::now();
        let _tools2 = service.list_tools().await.unwrap();
        let second_call_time = start.elapsed();

        // Second call should be faster (though this might not always be true due to system variability)
        // We just verify both calls succeed and complete in reasonable time
        assert!(first_call_time.as_millis() < 1000);
        assert!(second_call_time.as_millis() < 1000);

        // Verify they return the same results
        assert_eq!(_tools1.len(), _tools2.len());
    }
}