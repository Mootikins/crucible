// tests/test_lib.rs
//! Comprehensive tests for lib.rs main API

use crucible_mcp::{
    create_provider, EmbeddingConfig, EmbeddingDatabase, EmbeddingProvider,
    McpServer, McpTool, StdioMcpServer,
};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test database
async fn setup_test_db() -> (TempDir, String) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();
    (temp_dir, db_path)
}

/// Helper to create a mock embedding provider
fn create_test_provider() -> Arc<dyn EmbeddingProvider> {
    let config = EmbeddingConfig::ollama(None, None);
    futures::executor::block_on(create_provider(config)).unwrap()
}

#[tokio::test]
async fn test_mcp_server_new() {
    let (_temp_dir, db_path) = setup_test_db().await;
    let provider = create_test_provider();

    let server = McpServer::new(&db_path, provider).await;
    assert!(server.is_ok());
}

#[tokio::test]
async fn test_mcp_server_get_tools() {
    let tools = McpServer::get_tools();

    // Verify we have all expected tools
    assert!(!tools.is_empty());

    // Check for key tools
    let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();

    assert!(tool_names.contains(&"search_by_properties".to_string()));
    assert!(tool_names.contains(&"search_by_tags".to_string()));
    assert!(tool_names.contains(&"search_by_folder".to_string()));
    assert!(tool_names.contains(&"search_by_filename".to_string()));
    assert!(tool_names.contains(&"search_by_content".to_string()));
    assert!(tool_names.contains(&"semantic_search".to_string()));
    assert!(tool_names.contains(&"index_vault".to_string()));
    assert!(tool_names.contains(&"get_note_metadata".to_string()));
    assert!(tool_names.contains(&"update_note_properties".to_string()));
    assert!(tool_names.contains(&"index_document".to_string()));
    assert!(tool_names.contains(&"search_documents".to_string()));
    assert!(tool_names.contains(&"get_document_stats".to_string()));
    assert!(tool_names.contains(&"update_document_properties".to_string()));
}

#[tokio::test]
async fn test_mcp_tool_structure() {
    let tools = McpServer::get_tools();

    for tool in tools.iter() {
        // Each tool should have a name
        assert!(!tool.name.is_empty());

        // Each tool should have a description
        assert!(!tool.description.is_empty());

        // Input schema should be an object
        assert!(tool.input_schema.is_object());

        // Input schema should have type and properties
        let schema = &tool.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
    }
}

#[tokio::test]
async fn test_handle_unknown_tool() {
    let (_temp_dir, db_path) = setup_test_db().await;
    let provider = create_test_provider();
    let server = McpServer::new(&db_path, provider).await.unwrap();

    let result = server
        .handle_tool_call("nonexistent_tool", json!({}))
        .await
        .unwrap();

    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("Unknown tool"));
}

#[tokio::test]
async fn test_handle_get_document_stats() {
    let (_temp_dir, db_path) = setup_test_db().await;
    let provider = create_test_provider();
    let server = McpServer::new(&db_path, provider).await.unwrap();

    let result = server
        .handle_tool_call("get_document_stats", json!({}))
        .await;

    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_start_server() {
    let (_temp_dir, db_path) = setup_test_db().await;
    let provider = create_test_provider();
    let server = McpServer::new(&db_path, provider).await.unwrap();

    let result = server.start().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_stdio_server_creation() {
    let server = StdioMcpServer::new("test-server".to_string(), "1.0.0".to_string());

    // Server should be created successfully
    // (Can't test name/version as they're private, but creation tests the API)
    drop(server);
}

#[tokio::test]
async fn test_tool_descriptions_have_prefixes() {
    let tools = McpServer::get_tools();

    for tool in tools.iter() {
        // Check that description starts with [READ], [WRITE], [INDEX], or [INTERNAL]
        let desc = &tool.description;
        assert!(
            desc.starts_with("[READ]")
                || desc.starts_with("[WRITE]")
                || desc.starts_with("[INDEX]")
                || desc.starts_with("[INTERNAL]"),
            "Tool {} description should start with category prefix: {}",
            tool.name,
            desc
        );
    }
}

#[tokio::test]
async fn test_search_by_properties_schema() {
    let tools = McpServer::get_tools();
    let tool = tools.iter().find(|t| t.name == "search_by_properties").unwrap();

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["properties"].is_object());
    assert_eq!(schema["required"][0], "properties");
}

#[tokio::test]
async fn test_search_by_tags_schema() {
    let tools = McpServer::get_tools();
    let tool = tools.iter().find(|t| t.name == "search_by_tags").unwrap();

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["tags"]["type"], "array");
    assert_eq!(schema["required"][0], "tags");
}

#[tokio::test]
async fn test_semantic_search_schema() {
    let tools = McpServer::get_tools();
    let tool = tools.iter().find(|t| t.name == "semantic_search").unwrap();

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["query"]["type"], "string");
    assert_eq!(schema["properties"]["top_k"]["type"], "integer");
    assert_eq!(schema["properties"]["top_k"]["default"], 10);
    assert_eq!(schema["required"][0], "query");
}

#[tokio::test]
async fn test_index_vault_schema() {
    let tools = McpServer::get_tools();
    let tool = tools.iter().find(|t| t.name == "index_vault").unwrap();

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["force"]["type"], "boolean");
    assert_eq!(schema["properties"]["force"]["default"], false);
}

#[tokio::test]
async fn test_update_note_properties_schema() {
    let tools = McpServer::get_tools();
    let tool = tools.iter().find(|t| t.name == "update_note_properties").unwrap();

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["path"]["type"], "string");
    assert!(schema["properties"]["properties"].is_object());
    assert_eq!(schema["required"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_tool_count() {
    let tools = McpServer::get_tools();
    // We should have exactly 13 tools
    assert_eq!(tools.len(), 13);
}

#[tokio::test]
async fn test_re_exports() {
    // Test that we can access re-exported types
    let config = EmbeddingConfig::ollama(None, None);

    let provider = create_provider(config).await;
    assert!(provider.is_ok());
}

#[tokio::test]
async fn test_database_creation() {
    let (_temp_dir, db_path) = setup_test_db().await;
    let db = EmbeddingDatabase::new(&db_path).await;
    assert!(db.is_ok());
}

#[tokio::test]
async fn test_mcptool_serialization() {
    let tools = McpServer::get_tools();
    let tool = &tools[0];

    // Test that the tool can be serialized to JSON
    let json = serde_json::to_string(tool);
    assert!(json.is_ok());

    // Test that it can be deserialized back
    let deserialized: Result<McpTool, _> = serde_json::from_str(&json.unwrap());
    assert!(deserialized.is_ok());
}

#[tokio::test]
async fn test_handle_tool_call_search_by_tags_missing_required_field() {
    let (_temp_dir, db_path) = setup_test_db().await;
    let provider = create_test_provider();
    let server = McpServer::new(&db_path, provider).await.unwrap();

    // Pass arguments without the required tags field
    let result = server
        .handle_tool_call("search_by_tags", json!({}))
        .await
        .unwrap();

    // Should return an error result (not panic), since tags are required
    assert!(!result.success || result.data.is_none());
}

#[tokio::test]
async fn test_all_tools_have_unique_names() {
    let tools = McpServer::get_tools();
    let mut names = std::collections::HashSet::new();

    for tool in tools.iter() {
        assert!(
            names.insert(tool.name.clone()),
            "Duplicate tool name found: {}",
            tool.name
        );
    }
}

#[tokio::test]
async fn test_tool_schemas_are_valid_json_schema() {
    let tools = McpServer::get_tools();

    for tool in tools.iter() {
        let schema = &tool.input_schema;

        // Every schema should have type: object
        assert_eq!(schema["type"], "object");

        // Should have properties field
        assert!(schema.get("properties").is_some());

        // If required field exists, it should be an array
        if let Some(required) = schema.get("required") {
            assert!(required.is_array());
        }
    }
}
