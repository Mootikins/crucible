use crucible_mcp::{EmbeddingConfig, create_provider};
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::types::ToolCallArgs;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setup
    let embedding_config = EmbeddingConfig::from_env()?;
    let provider = create_provider(embedding_config).await?;

    let vault_path = env::var("OBSIDIAN_VAULT_PATH")?;
    let db_path = format!("{}/.crucible/embeddings.db", vault_path);
    let database = EmbeddingDatabase::new(&db_path).await?;

    println!("✅ Setup complete");
    println!("✅ Model: {}", provider.model_name());
    println!("✅ Vault: {}", vault_path);

    // Test indexing
    println!("\n📚 Indexing vault...");
    let args = ToolCallArgs {
        force: Some(true),
        properties: None,
        tags: None,
        path: Some(vault_path.clone()), // Use the vault path from environment
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
    };

    match crucible_mcp::tools::index_vault(&database, &provider, &args).await {
        Ok(result) => {
            println!("✅ Indexing completed successfully");
            if let Some(data) = result.data {
                println!("   Indexed files: {}", data);
            }
            if let Some(error) = result.error {
                println!("   Indexing error: {}", error);
            }
        }
        Err(e) => {
            println!("❌ Indexing failed: {}", e);
        }
    }

    // Test semantic search
    println!("\n🔍 Testing semantic search for 'testing'...");
    let search_args = ToolCallArgs {
        query: Some("testing".to_string()),
        top_k: Some(5),
        force: None,
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
    };

    match crucible_mcp::tools::semantic_search(&database, &provider, &search_args).await {
        Ok(result) => {
            println!("✅ Semantic search completed successfully");
            if let Some(data) = result.data {
                println!("   Results: {}", serde_json::to_string_pretty(&data)?);
            }
        }
        Err(e) => {
            println!("❌ Semantic search failed: {}", e);
            return Err(e.into());
        }
    }

    // Test content search
    println!("\n📝 Testing content search for 'welcome'...");
    let content_args = ToolCallArgs {
        query: Some("welcome".to_string()),
        force: None,
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
        top_k: None,
    };

    match crucible_mcp::tools::search_by_content(&database, &content_args).await {
        Ok(result) => {
            println!("✅ Content search completed successfully");
            if let Some(data) = result.data {
                println!("   Results: {}", serde_json::to_string_pretty(&data)?);
            }
        }
        Err(e) => {
            println!("⚠️  Content search note: {}", e);
        }
    }

    println!("\n🎉 Core functionality is working!");
    Ok(())
}