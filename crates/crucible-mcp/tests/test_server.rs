mod test_helpers;

use crucible_mcp::{types::ToolCallArgs, McpServer};
use serde_json::json;
use tempfile::tempdir;
use test_helpers::create_test_provider;

#[tokio::test]
async fn test_server_initialization() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");

    let _server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // Test that server was created successfully
    assert!(true); // If we get here, initialization succeeded
}

#[tokio::test]
async fn test_get_tools() {
    let tools = McpServer::get_tools();

    assert_eq!(tools.len(), 13);

    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(tool_names.contains(&"search_by_properties"));
    assert!(tool_names.contains(&"search_by_tags"));
    assert!(tool_names.contains(&"search_by_folder"));
    assert!(tool_names.contains(&"search_by_filename"));
    assert!(tool_names.contains(&"search_by_content"));
    assert!(tool_names.contains(&"semantic_search"));
    assert!(tool_names.contains(&"index_vault"));
    assert!(tool_names.contains(&"get_note_metadata"));
    assert!(tool_names.contains(&"update_note_properties"));
    assert!(tool_names.contains(&"index_document"));
    assert!(tool_names.contains(&"search_documents"));
    assert!(tool_names.contains(&"get_document_stats"));
    assert!(tool_names.contains(&"update_document_properties"));
}

#[tokio::test]
async fn test_tool_schemas() {
    let tools = McpServer::get_tools();

    for tool in tools {
        assert!(!tool.name.is_empty());
        assert!(!tool.description.is_empty());
        assert!(tool.input_schema.is_object());

        // Check that schema has required fields
        let schema = tool.input_schema.as_object().unwrap();
        assert_eq!(schema.get("type"), Some(&json!("object")));
        assert!(schema.contains_key("properties"));
    }
}

#[tokio::test]
async fn test_unknown_tool_error() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");

    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let result = server
        .handle_tool_call("unknown_tool", json!({}))
        .await
        .unwrap();

    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("Unknown tool"));
}

#[tokio::test]
async fn test_tool_call_args_parsing() {
    let args_json = json!({
        "properties": {"status": "active"},
        "tags": ["project", "urgent"],
        "path": "/test/folder",
        "recursive": true,
        "pattern": "*.md",
        "query": "test search",
        "top_k": 5,
        "force": false
    });

    let args: ToolCallArgs = serde_json::from_value(args_json).unwrap();

    assert!(args.properties.is_some());
    assert!(args.tags.is_some());
    assert!(args.path.is_some());
    assert!(args.recursive.is_some());
    assert!(args.pattern.is_some());
    assert!(args.query.is_some());
    assert!(args.top_k.is_some());
    assert!(args.force.is_some());

    assert_eq!(args.tags.unwrap(), vec!["project", "urgent"]);
    assert_eq!(args.path.unwrap(), "/test/folder");
    assert_eq!(args.query.unwrap(), "test search");
    assert_eq!(args.top_k.unwrap(), 5);
}

#[tokio::test]
async fn test_tool_call_args_minimal() {
    let args_json = json!({
        "query": "minimal test"
    });

    let args: ToolCallArgs = serde_json::from_value(args_json).unwrap();

    assert!(args.properties.is_none());
    assert!(args.tags.is_none());
    assert!(args.path.is_none());
    assert!(args.recursive.is_none());
    assert!(args.pattern.is_none());
    assert!(args.query.is_some());
    assert!(args.top_k.is_none());
    assert!(args.force.is_none());

    assert_eq!(args.query.unwrap(), "minimal test");
}

#[tokio::test]
async fn test_server_start() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");

    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // Test that start doesn't panic (even though it's a no-op for now)
    let result = server.start().await;
    assert!(result.is_ok());
}

// New comprehensive tests for 90%+ coverage

#[tokio::test]
async fn test_search_by_properties_success() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // First add some data
    let index_args = json!({ "force": true });
    server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();

    let args = json!({
        "properties": {"status": "active"}
    });

    let result = server
        .handle_tool_call("search_by_properties", args)
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.data.is_some());
    assert!(result.error.is_none());
}

#[tokio::test]
async fn test_search_by_properties_missing_args() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let args = json!({});

    let result = server
        .handle_tool_call("search_by_properties", args)
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("Missing properties"));
}

#[tokio::test]
async fn test_search_by_tags_success() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // First add some data
    let index_args = json!({ "force": true });
    server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();

    let args = json!({
        "tags": ["test", "important"]
    });

    let result = server
        .handle_tool_call("search_by_tags", args)
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.data.is_some());
    assert!(result.error.is_none());
}

#[tokio::test]
async fn test_search_by_tags_missing_args() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let args = json!({});

    let result = server
        .handle_tool_call("search_by_tags", args)
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("Missing tags"));
}

#[tokio::test]
async fn test_search_by_folder_success() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // First add some data
    let index_args = json!({ "force": true });
    server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();

    let args = json!({
        "path": "/test",
        "recursive": true
    });

    let result = server
        .handle_tool_call("search_by_folder", args)
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.data.is_some());
    assert!(result.error.is_none());
}

#[tokio::test]
async fn test_search_by_folder_missing_args() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let args = json!({});

    let result = server
        .handle_tool_call("search_by_folder", args)
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("Missing path"));
}

#[tokio::test]
async fn test_search_by_filename_success() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // First add some data
    let index_args = json!({ "force": true });
    server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();

    let args = json!({
        "pattern": "*.md"
    });

    let result = server
        .handle_tool_call("search_by_filename", args)
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.data.is_some());
    assert!(result.error.is_none());
}

#[tokio::test]
async fn test_search_by_filename_missing_args() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let args = json!({});

    let result = server
        .handle_tool_call("search_by_filename", args)
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("Missing pattern"));
}

#[tokio::test]
async fn test_search_by_content_success() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // First add some data
    let index_args = json!({ "force": true });
    server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();

    let args = json!({
        "query": "test content"
    });

    let result = server
        .handle_tool_call("search_by_content", args)
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.data.is_some());
    assert!(result.error.is_none());
}

#[tokio::test]
async fn test_search_by_content_missing_args() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let args = json!({});

    let result = server
        .handle_tool_call("search_by_content", args)
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("Missing query"));
}

#[tokio::test]
async fn test_semantic_search_success() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // First add some data
    let index_args = json!({ "force": true });
    server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();

    let args = json!({
        "query": "semantic search test",
        "top_k": 5
    });

    let result = server
        .handle_tool_call("semantic_search", args)
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.data.is_some());
    assert!(result.error.is_none());
}

#[tokio::test]
async fn test_semantic_search_missing_args() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let args = json!({});

    let result = server
        .handle_tool_call("semantic_search", args)
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("Missing query"));
}

#[tokio::test]
async fn test_index_vault_success() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let args = json!({
        "force": true
    });

    let result = server.handle_tool_call("index_vault", args).await.unwrap();
    assert!(result.success);
    assert!(result.data.is_some());
    assert!(result.error.is_none());

    // Check that data was actually indexed
    let data = result.data.unwrap();
    let indexed_count = data.get("indexed").unwrap().as_u64().unwrap();
    assert!(indexed_count > 0);
}

#[tokio::test]
async fn test_index_vault_incremental() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // First index
    let args1 = json!({ "force": true });
    let result1 = server.handle_tool_call("index_vault", args1).await.unwrap();
    assert!(result1.success);

    // Second index without force (should skip existing)
    let args2 = json!({ "force": false });
    let result2 = server.handle_tool_call("index_vault", args2).await.unwrap();
    assert!(result2.success);

    // Should have indexed fewer files the second time
    let count1 = result1
        .data
        .unwrap()
        .get("indexed")
        .unwrap()
        .as_u64()
        .unwrap();
    let count2 = result2
        .data
        .unwrap()
        .get("indexed")
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(count2 < count1);
}

#[tokio::test]
async fn test_get_note_metadata_success() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // First add some data
    let index_args = json!({ "force": true });
    server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();

    let args = json!({
        "path": "file0.md"
    });

    let result = server
        .handle_tool_call("get_note_metadata", args)
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.data.is_some());
    assert!(result.error.is_none());
}

#[tokio::test]
async fn test_get_note_metadata_missing() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let args = json!({
        "path": "nonexistent.md"
    });

    let result = server
        .handle_tool_call("get_note_metadata", args)
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("File not found"));
}

#[tokio::test]
async fn test_get_note_metadata_missing_args() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let args = json!({});

    let result = server
        .handle_tool_call("get_note_metadata", args)
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("Missing path"));
}

#[tokio::test]
async fn test_update_note_properties_success() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // First add some data
    let index_args = json!({ "force": true });
    server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();

    let args = json!({
        "path": "file0.md",
        "properties": {
            "status": "updated",
            "priority": 2
        }
    });

    let result = server
        .handle_tool_call("update_note_properties", args)
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.data.is_some());
    assert!(result.error.is_none());
}

#[tokio::test]
async fn test_update_note_properties_missing_file() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let args = json!({
        "path": "nonexistent.md",
        "properties": {
            "status": "updated"
        }
    });

    let result = server
        .handle_tool_call("update_note_properties", args)
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("File not found"));
}

#[tokio::test]
async fn test_update_note_properties_missing_args() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    let args = json!({
        "path": "test.md"
    });

    let result = server
        .handle_tool_call("update_note_properties", args)
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("Missing properties"));
}

#[tokio::test]
async fn test_tool_call_with_invalid_json() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // This should fail during JSON parsing
    let result = server
        .handle_tool_call("search_by_tags", json!("invalid"))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_all_tools_with_empty_database() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("server_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // Test all tools with empty database
    let tools = [
        "search_by_properties",
        "search_by_tags",
        "search_by_folder",
        "search_by_filename",
        "search_by_content",
        "semantic_search",
    ];

    for tool in tools {
        let args = match tool {
            "search_by_properties" => json!({"properties": {"test": "value"}}),
            "search_by_tags" => json!({"tags": ["test"]}),
            "search_by_folder" => json!({"path": "/test"}),
            "search_by_filename" => json!({"pattern": "*.md"}),
            "search_by_content" => json!({"query": "test"}),
            "semantic_search" => json!({"query": "test"}),
            _ => json!({}),
        };

        let result = server.handle_tool_call(tool, args).await.unwrap();
        // All should succeed but return empty results
        assert!(result.success);
        assert!(result.data.is_some());
    }
}
