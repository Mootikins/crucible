// examples/mcp_client_demo.rs
//! Demonstration of using the Crucible MCP server programmatically

use crucible_mcp::McpServer;
use serde_json::json;
use tempfile::tempdir;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 Crucible MCP Server Demo");
    println!("============================\n");

    // Create a temporary database for this demo
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("demo.db");
    
    println!("📂 Creating database at: {:?}", db_path);
    let server = McpServer::new(db_path.to_str().unwrap()).await?;
    
    println!("✅ MCP Server initialized\n");

    // Demo 1: List available tools
    println!("🔧 Available MCP Tools:");
    let tools = McpServer::get_tools();
    for (i, tool) in tools.iter().enumerate() {
        println!("  {}. {} - {}", i + 1, tool.name, tool.description);
    }
    println!();

    // Demo 2: Index some sample content
    println!("📝 Indexing sample content...");
    let result = server.handle_tool_call(
        "index_vault",
        json!({"force": true})
    ).await?;
    
    if result.success {
        println!("✅ Indexing completed: {:?}", result.data);
    } else {
        println!("❌ Indexing failed: {:?}", result.error);
    }
    println!();

    // Demo 3: Semantic search
    println!("🔍 Performing semantic search...");
    let result = server.handle_tool_call(
        "semantic_search",
        json!({
            "query": "file content",
            "top_k": 3
        })
    ).await?;
    
    if result.success {
        println!("✅ Search completed:");
        if let Some(data) = result.data {
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    } else {
        println!("❌ Search failed: {:?}", result.error);
    }
    println!();

    // Demo 4: Index a Crucible document
    println!("📄 Indexing a Crucible document...");
    let document = json!({
        "id": "doc-123",
        "title": "My Research Notes",
        "content": "This document contains important research about machine learning and artificial intelligence.",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z"
    });
    
    let result = server.handle_tool_call(
        "index_document",
        json!({"document": document})
    ).await?;
    
    if result.success {
        println!("✅ Document indexed: {:?}", result.data);
    } else {
        println!("❌ Document indexing failed: {:?}", result.error);
    }
    println!();

    // Demo 5: Search documents
    println!("🔎 Searching Crucible documents...");
    let result = server.handle_tool_call(
        "search_documents",
        json!({
            "query": "machine learning research",
            "top_k": 5
        })
    ).await?;
    
    if result.success {
        println!("✅ Document search completed:");
        if let Some(data) = result.data {
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    } else {
        println!("❌ Document search failed: {:?}", result.error);
    }
    println!();

    // Demo 6: Update document properties
    println!("📝 Updating document properties...");
    let properties = json!({
        "processed": true,
        "tags": ["research", "ml", "ai"],
        "priority": "high"
    });
    
    let result = server.handle_tool_call(
        "update_document_properties",
        json!({
            "document_id": "My Research Notes",
            "properties": properties
        })
    ).await?;
    
    if result.success {
        println!("✅ Properties updated: {:?}", result.data);
    } else {
        println!("❌ Property update failed: {:?}", result.error);
    }
    println!();

    // Demo 7: Get statistics
    println!("📊 Getting database statistics...");
    let result = server.handle_tool_call(
        "get_document_stats",
        json!({})
    ).await?;
    
    if result.success {
        println!("✅ Statistics retrieved:");
        if let Some(data) = result.data {
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    } else {
        println!("❌ Statistics retrieval failed: {:?}", result.error);
    }
    println!();

    // Demo 8: Content search
    println!("📋 Searching by content...");
    let result = server.handle_tool_call(
        "search_by_content",
        json!({"query": "research"})
    ).await?;
    
    if result.success {
        println!("✅ Content search completed:");
        if let Some(data) = result.data {
            let empty_vec = vec![];
            let results = data.as_array().unwrap_or(&empty_vec);
            println!("  Found {} matching documents", results.len());
            for (i, result) in results.iter().enumerate() {
                if let Some(path) = result.get("file_path").and_then(|p| p.as_str()) {
                    println!("    {}. {}", i + 1, path);
                }
            }
        }
    } else {
        println!("❌ Content search failed: {:?}", result.error);
    }
    println!();

    // Demo 9: Error handling
    println!("⚠️  Testing error handling...");
    let result = server.handle_tool_call(
        "search_by_tags",
        json!({}) // Missing required 'tags' parameter
    ).await?;
    
    if !result.success {
        println!("✅ Error handled correctly: {}", result.error.unwrap_or_else(|| "Unknown error".to_string()));
    } else {
        println!("❌ Expected error but got success");
    }
    println!();

    println!("🎉 Demo completed successfully!");
    println!("\n💡 This demonstrates the full MCP server functionality:");
    println!("   • Document indexing and search");
    println!("   • Semantic vector search");
    println!("   • Metadata management");
    println!("   • Error handling");
    println!("   • Integration with Crucible documents");
    
    Ok(())
}
