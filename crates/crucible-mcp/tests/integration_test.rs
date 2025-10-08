mod test_helpers;

use crucible_mcp::McpServer;
use serde_json::json;
use tempfile::tempdir;
use test_helpers::create_test_provider;

#[tokio::test]
async fn test_full_indexing_and_search_flow() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("integration_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // Step 1: Index the vault
    let index_args = json!({ "force": true });
    let index_result = server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();
    assert!(index_result.success);

    let indexed_count = index_result
        .data
        .unwrap()
        .get("indexed")
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(indexed_count > 0);

    // Step 2: Search by content
    let content_search_args = json!({
        "query": "Content for file"
    });
    let content_result = server
        .handle_tool_call("search_by_content", content_search_args)
        .await
        .unwrap();
    assert!(content_result.success);

    let content_data = content_result.data.unwrap();
    let content_files = content_data.as_array().unwrap();
    assert!(content_files.len() > 0);

    // Step 3: Semantic search
    let semantic_args = json!({
        "query": "test content",
        "top_k": 3
    });
    let semantic_result = server
        .handle_tool_call("semantic_search", semantic_args)
        .await
        .unwrap();
    assert!(semantic_result.success);

    let semantic_data = semantic_result.data.unwrap();
    let semantic_files = semantic_data.as_array().unwrap();
    assert!(semantic_files.len() > 0);

    // Step 4: Search by tags
    let tag_args = json!({
        "tags": ["indexed"]
    });
    let tag_result = server
        .handle_tool_call("search_by_tags", tag_args)
        .await
        .unwrap();
    assert!(tag_result.success);

    let tag_data = tag_result.data.unwrap();
    let tag_files = tag_data.as_array().unwrap();
    assert!(tag_files.len() > 0);

    // Step 5: Search by filename pattern
    let filename_args = json!({
        "pattern": "file*.md"
    });
    let filename_result = server
        .handle_tool_call("search_by_filename", filename_args)
        .await
        .unwrap();
    assert!(filename_result.success);

    let filename_data = filename_result.data.unwrap();
    let filename_files = filename_data.as_array().unwrap();
    assert!(filename_files.len() > 0);
}

#[tokio::test]
async fn test_metadata_update_affects_search() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("integration_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // Step 1: Index the vault
    let index_args = json!({ "force": true });
    server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();

    // Step 2: Get a file to update
    let files_result = server
        .handle_tool_call("search_by_filename", json!({"pattern": "file*.md"}))
        .await
        .unwrap();
    let files_data = files_result.data.unwrap();
    let files = files_data.as_array().unwrap();
    assert!(files.len() > 0);

    let file_path = files[0].as_str().unwrap();

    // Step 3: Update properties
    let update_args = json!({
        "path": file_path,
        "properties": {
            "status": "updated",
            "priority": 1,
            "category": "test"
        }
    });
    let update_result = server
        .handle_tool_call("update_note_properties", update_args)
        .await
        .unwrap();
    assert!(update_result.success);

    // Step 4: Search by the new properties
    let search_args = json!({
        "properties": {
            "status": "updated"
        }
    });
    let search_result = server
        .handle_tool_call("search_by_properties", search_args)
        .await
        .unwrap();
    assert!(search_result.success);

    let found_data = search_result.data.unwrap();
    let found_files = found_data.as_array().unwrap();
    assert!(found_files.contains(&json!(file_path)));

    // Step 5: Verify metadata was actually updated
    let metadata_args = json!({
        "path": file_path
    });
    let metadata_result = server
        .handle_tool_call("get_note_metadata", metadata_args)
        .await
        .unwrap();
    assert!(metadata_result.success);

    let metadata = metadata_result.data.unwrap();
    assert_eq!(metadata["properties"]["status"], "updated");
    assert_eq!(metadata["properties"]["priority"], 1);
    assert_eq!(metadata["properties"]["category"], "test");
}

#[tokio::test]
async fn test_multi_step_workflow() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("integration_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // Step 1: Initial indexing
    let index_args = json!({ "force": true });
    let index_result = server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();
    assert!(index_result.success);

    // Step 2: Search for files in a specific folder (search for files starting with "file")
    let folder_args = json!({
        "path": "file",
        "recursive": true
    });
    let folder_result = server
        .handle_tool_call("search_by_folder", folder_args)
        .await
        .unwrap();
    assert!(folder_result.success);

    let folder_data = folder_result.data.unwrap();
    let folder_files = folder_data.as_array().unwrap();
    assert!(folder_files.len() > 0);

    // Step 3: Update multiple files with different properties
    for (i, file) in folder_files.iter().enumerate() {
        let file_path = file.as_str().unwrap();
        let update_args = json!({
            "path": file_path,
            "properties": {
                "workflow_step": i,
                "processed": true,
                "batch": "test_batch"
            }
        });
        let update_result = server
            .handle_tool_call("update_note_properties", update_args)
            .await
            .unwrap();
        assert!(update_result.success);
    }

    // Step 4: Search for all processed files
    let processed_args = json!({
        "properties": {
            "processed": true
        }
    });
    let processed_result = server
        .handle_tool_call("search_by_properties", processed_args)
        .await
        .unwrap();
    assert!(processed_result.success);

    let processed_data = processed_result.data.unwrap();
    let processed_files = processed_data.as_array().unwrap();
    assert_eq!(processed_files.len(), folder_files.len());

    // Step 5: Search for files in a specific batch
    let batch_args = json!({
        "properties": {
            "batch": "test_batch"
        }
    });
    let batch_result = server
        .handle_tool_call("search_by_properties", batch_args)
        .await
        .unwrap();
    assert!(batch_result.success);

    let batch_data = batch_result.data.unwrap();
    let batch_files = batch_data.as_array().unwrap();
    assert_eq!(batch_files.len(), folder_files.len());

    // Step 6: Semantic search to find similar content
    let semantic_args = json!({
        "query": "test content",
        "top_k": 5
    });
    let semantic_result = server
        .handle_tool_call("semantic_search", semantic_args)
        .await
        .unwrap();
    assert!(semantic_result.success);

    let semantic_data = semantic_result.data.unwrap();
    let semantic_files = semantic_data.as_array().unwrap();
    assert!(semantic_files.len() > 0);

    // Step 7: Verify we can get metadata for any file
    for file in &semantic_files[..2] {
        // Test first 2 files
        let file_obj = file.as_object().unwrap();
        let file_path = file_obj["id"].as_str().unwrap();

        let metadata_args = json!({
            "path": file_path
        });
        let metadata_result = server
            .handle_tool_call("get_note_metadata", metadata_args)
            .await
            .unwrap();
        assert!(metadata_result.success);

        let metadata = metadata_result.data.unwrap();
        assert!(metadata["properties"]["processed"].as_bool().unwrap());
    }
}

#[tokio::test]
async fn test_error_handling_workflow() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("integration_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // Test various error conditions in sequence

    // 1. Search with missing arguments
    let missing_args_result = server.handle_tool_call("search_by_tags", json!({})).await;
    match missing_args_result {
        Ok(result) => {
            assert!(!result.success);
            assert!(result.error.is_some());
            if let Some(error_msg) = result.error {
                assert!(error_msg.contains("Missing tags"));
            }
        }
        Err(e) => {
            // If it returns an error, that's also acceptable for missing arguments
            assert!(e.to_string().contains("Missing tags"));
        }
    }

    // 2. Search for non-existent file
    let nonexistent_result = server
        .handle_tool_call("get_note_metadata", json!({"path": "nonexistent.md"}))
        .await
        .unwrap();
    assert!(!nonexistent_result.success);
    assert!(nonexistent_result.error.is_some());

    // 3. Update non-existent file
    let update_nonexistent_result = server
        .handle_tool_call(
            "update_note_properties",
            json!({
                "path": "nonexistent.md",
                "properties": {"test": "value"}
            }),
        )
        .await
        .unwrap();
    assert!(!update_nonexistent_result.success);
    assert!(update_nonexistent_result.error.is_some());

    // 4. Unknown tool
    let unknown_tool_result = server
        .handle_tool_call("unknown_tool", json!({}))
        .await
        .unwrap();
    assert!(!unknown_tool_result.success);
    assert!(unknown_tool_result.error.is_some());
    assert!(unknown_tool_result.error.unwrap().contains("Unknown tool"));

    // 5. Valid operations should still work
    let index_args = json!({ "force": true });
    let index_result = server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();
    assert!(index_result.success);

    let search_args = json!({
        "query": "test"
    });
    let search_result = server
        .handle_tool_call("search_by_content", search_args)
        .await
        .unwrap();
    assert!(search_result.success);
}

#[tokio::test]
async fn test_performance_with_multiple_operations() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("integration_test.db");
    let server = McpServer::new(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // Index a reasonable amount of data
    let index_args = json!({ "force": true });
    let index_result = server
        .handle_tool_call("index_vault", index_args)
        .await
        .unwrap();
    assert!(index_result.success);

    let indexed_count = index_result
        .data
        .unwrap()
        .get("indexed")
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(indexed_count > 0);

    // Perform multiple search operations
    let search_operations = [
        ("search_by_content", json!({"query": "content"})),
        ("search_by_filename", json!({"pattern": "*.md"})),
        ("search_by_tags", json!({"tags": ["indexed"]})),
        ("semantic_search", json!({"query": "test", "top_k": 3})),
    ];

    for (tool_name, args) in search_operations {
        let result = server.handle_tool_call(tool_name, args).await.unwrap();
        assert!(result.success);
        assert!(result.data.is_some());
    }

    // Test folder search
    let folder_result = server
        .handle_tool_call(
            "search_by_folder",
            json!({
                "path": "vault",
                "recursive": true
            }),
        )
        .await
        .unwrap();
    assert!(folder_result.success);

    // Test properties search
    let properties_result = server
        .handle_tool_call(
            "search_by_properties",
            json!({
                "properties": {"test": "value"}
            }),
        )
        .await
        .unwrap();
    assert!(properties_result.success);
}
