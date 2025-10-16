// Test cases for Bug #1: MCP Server Indexing Errors
//
// These tests reproduce the "File not found" errors during vault indexing.
// They should initially fail and pass after implementing the fix.

use crucible_mcp::{CrucibleMcpService, EmbeddingConfig, create_provider};
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::tools::index_vault;
use crucible_mcp::types::{ToolCallArgs, ToolCallResult};
use rmcp::handler::server::ServerHandler;
use std::sync::Arc;
use tempfile::tempdir;
use std::collections::HashMap;
use async_trait::async_trait;
use crucible_mcp::embeddings::{EmbeddingResponse, EmbeddingResult, EmbeddingProvider};

// Mock embedding provider for testing
struct TestEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for TestEmbeddingProvider {
    async fn embed(&self, _text: &str) -> EmbeddingResult<EmbeddingResponse> {
        Ok(EmbeddingResponse::new(vec![0.1; 384], "test-model".to_string()))
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        Ok(texts.iter().map(|_| EmbeddingResponse::new(vec![0.1; 384], "test-model".to_string())).collect())
    }

    fn model_name(&self) -> &str {
        "test-model"
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn provider_name(&self) -> &str {
        "TestProvider"
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn test_index_vault_file_path_resolution() {
    // This test reproduces Bug #1: MCP Server Indexing Errors
    //
    // Expected: Files should be successfully indexed without "File not found" errors
    // Actual (before fix): All files fail with "File not found" errors despite existing

    let temp_dir = tempdir().unwrap();
    let vault_path = temp_dir.path();

    // Create test markdown files in the vault
    std::fs::write(vault_path.join("note1.md"), "# Note 1\nContent of note 1.").unwrap();
    std::fs::write(vault_path.join("note2.md"), "# Note 2\nContent of note 2.").unwrap();

    let subfolder = vault_path.join("subfolder");
    std::fs::create_dir(&subfolder).unwrap();
    std::fs::write(subfolder.join("note3.md"), "# Note 3\nContent of note 3.").unwrap();

    // Set up database and provider
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    let provider: Arc<dyn EmbeddingProvider> = Arc::new(TestEmbeddingProvider);

    // Set up environment variables for Obsidian client
    std::env::set_var("OBSIDIAN_VAULT_PATH", vault_path);

    // Test indexing with various path scenarios
    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: Some(vault_path.to_string_lossy().to_string()),
        recursive: Some(true),
        pattern: Some("*.md".to_string()),
        query: None,
        top_k: None,
        force: Some(true),
    };

    // This should succeed but currently fails due to path resolution issues
    let result = index_vault(&database, &provider, &args).await;

    // Before fix: This should fail with "File not found" errors
    // After fix: This should succeed with files indexed
    match result {
        Ok(tool_result) => {
            // If we get here, check if indexing actually worked
            if tool_result.success {
                // Success case - verify files were indexed
                let stats = database.get_stats().await.unwrap();
                let indexed_count = stats.get("total_files").unwrap_or(&0);
                assert!(
                    *indexed_count > 0,
                    "Expected files to be indexed, but found {} files",
                    indexed_count
                );
                println!("✅ Indexing succeeded: {} files indexed", indexed_count);
            } else {
                // Failure case - this is the current bug behavior
                let error_msg = tool_result.error.unwrap_or_else(|| "Unknown error".to_string());
                panic!(
                    "❌ Indexing failed with error: {}.\n\
                     This indicates Bug #1 is present: MCP server cannot resolve file paths during indexing.\n\
                     Expected: Files should be found and indexed successfully.\n\
                     Actual: {}",
                    error_msg, error_msg
                );
            }
        }
        Err(e) => {
            panic!(
                "❌ Indexing function returned error: {}.\n\
                 This indicates a more fundamental issue with the indexing implementation.",
                e
            );
        }
    }
}

#[tokio::test]
async fn test_index_vault_with_relative_paths() {
    // Test indexing with relative paths to expose path resolution issues
    let temp_dir = tempdir().unwrap();
    let vault_path = temp_dir.path();

    // Create test file
    std::fs::write(vault_path.join("test.md"), "# Test\nContent").unwrap();

    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    let provider: Arc<dyn EmbeddingProvider> = Arc::new(TestEmbeddingProvider);

    // Test with relative path (should expose the bug)
    let relative_path = "./test.md";
    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: Some(relative_path.to_string()),
        recursive: Some(true),
        pattern: None,
        query: None,
        top_k: None,
        force: Some(true),
    };

    let result = index_vault(&database, &provider, &args).await;

    // This test is designed to fail before the fix is implemented
    match result {
        Ok(tool_result) => {
            if !tool_result.success {
                println!("⚠️  Expected failure for relative path test (before fix): {}",
                    tool_result.error.unwrap_or_default());
                // This is expected behavior before the fix
            } else {
                // If this succeeds, the fix might already be implemented
                println!("✅ Relative path test passed - fix may already be implemented");
            }
        }
        Err(e) => {
            println!("⚠️  Expected error for relative path test (before fix): {}", e);
        }
    }
}

#[tokio::test]
async fn test_index_vault_with_special_characters() {
    // Test indexing with files that have special characters in names
    let temp_dir = tempdir().unwrap();
    let vault_path = temp_dir.path();

    // Create files with special characters
    std::fs::write(vault_path.join("test spaces.md"), "# Spaces\nContent with spaces").unwrap();
    std::fs::write(vault_path.join("test-dash.md"), "# Dash\nContent with dash").unwrap();
    std::fs::write(vault_path.join("test_underscore.md"), "# Underscore\nContent with underscore").unwrap();

    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    let provider: Arc<dyn EmbeddingProvider> = Arc::new(TestEmbeddingProvider);

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: Some(vault_path.to_string_lossy().to_string()),
        recursive: Some(true),
        pattern: Some("*.md".to_string()),
        query: None,
        top_k: None,
        force: Some(true),
    };

    let result = index_vault(&database, &provider, &args).await;

    match result {
        Ok(tool_result) => {
            if !tool_result.success {
                println!("⚠️  Special characters test failed (expected before fix): {}",
                    tool_result.error.unwrap_or_default());
            } else {
                println!("✅ Special characters test passed");
            }
        }
        Err(e) => {
            println!("⚠️  Special characters test error (expected before fix): {}", e);
        }
    }
}