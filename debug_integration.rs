use crucible_mcp::McpServer;
use serde_json::json;
use std::fs;
use tempfile::tempdir;
use std::sync::Arc;

use crucible_mcp::embeddings::{EmbeddingProvider, EmbeddingResponse, EmbeddingResult};
use async_trait::async_trait;

pub struct DebugEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for DebugEmbeddingProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        println!("Embedding text: {}", text);
        let embedding = vec![0.1; 384];
        Ok(EmbeddingResponse::new(embedding, "debug-model".to_string()))
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        println!("Embedding batch of {} texts", texts.len());
        let mut results = Vec::new();
        for text in texts {
            results.push(self.embed(&text).await?);
        }
        Ok(results)
    }

    fn model_name(&self) -> &str {
        "debug-model"
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn provider_name(&self) -> &str {
        "DebugProvider"
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        Ok(true)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let vault_path = temp_dir.path().join("vault");
    fs::create_dir(&vault_path)?;

    // Create test files
    let files = vec![
        ("file0.md", "Content for file: file0.md"),
        ("file1.md", "Content for file: file1.md"),
        ("file2.md", "Content for file: file2.md"),
    ];

    for (filename, content) in files {
        let file_path = vault_path.join(filename);
        fs::write(file_path, content)?;
    }

    println!("Created test vault at: {}", vault_path.display());

    let db_path = temp_dir.path().join("debug_test.db");
    let provider = Arc::new(DebugEmbeddingProvider);
    let server = McpServer::new(db_path.to_str().unwrap(), provider).await?;

    println!("Server created successfully");

    // Step 1: Index the vault
    let index_args = json!({
        "force": true,
        "path": vault_path.to_str().unwrap()
    });

    println!("Calling index_vault with args: {}", index_args);
    let index_result = server.handle_tool_call("index_vault", index_args).await?;

    println!("Index result: success={}, data={:?}, error={:?}",
        index_result.success,
        index_result.data,
        index_result.error);

    if !index_result.success {
        return Err(format!("Indexing failed: {:?}", index_result.error).into());
    }

    // Step 2: List files to see what's in the database
    let list_result = server.handle_tool_call("get_document_stats", json!({})).await?;
    println!("Stats result: success={}, data={:?}", list_result.success, list_result.data);

    // Step 3: Search by content
    let content_search_args = json!({
        "query": "Content for file"
    });

    println!("Searching for content with args: {}", content_search_args);
    let content_result = server.handle_tool_call("search_by_content", content_search_args).await?;

    println!("Content search result: success={}, data={:?}, error={:?}",
        content_result.success,
        content_result.data,
        content_result.error);

    if let Some(data) = &content_result.data {
        let content_files = data.as_array().unwrap_or(&vec![]);
        println!("Found {} files in content search", content_files.len());
        for (i, file) in content_files.iter().enumerate() {
            println!("File {}: {}", i, file);
        }
    }

    Ok(())
}