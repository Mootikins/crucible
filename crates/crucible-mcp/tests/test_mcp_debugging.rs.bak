// Test MCP Tool Discovery with Debugging
//
// This test specifically triggers the debugging in list_tools() to see why
// Rune tools aren't appearing in the MCP tool list.

use crucible_mcp::CrucibleMcpService;
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::rune_tools::ToolRegistry;
use rmcp::service::RequestContext;
use std::sync::Arc;
use tempfile::tempdir;
use async_trait::async_trait;
use crucible_mcp::embeddings::{EmbeddingResponse, EmbeddingResult, EmbeddingProvider};
use crucible_mcp::obsidian_client::ObsidianClient;

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
async fn test_debug_mcp_tool_list() {
    // Test that triggers debugging in list_tools() to see what's happening

    // Initialize tracing to capture debug logs
    let _subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .try_init();

    let temp_dir = tempdir().unwrap();

    // Set up database
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    // Create mock obsidian client
    let obsidian = match ObsidianClient::new() {
        Ok(client) => Arc::new(client),
        Err(_) => {
            println!("⚠️  Skipping test - Obsidian client not available");
            return;
        }
    };

    // Set up Rune tool registry with test tools
    let tool_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tool_dir).unwrap();

    // Create a simple test tool
    let test_tool_source = r#"
        pub fn NAME() { "debug_rune_tool" }
        pub fn DESCRIPTION() { "A test Rune tool for debugging MCP discovery" }
        pub fn INPUT_SCHEMA() {
            #{
                type: "object",
                properties: #{
                    message: #{ type: "string", description: "Test message" }
                },
                required: ["message"]
            }
        }

        pub async fn call(args) {
            let message = args.get("message").unwrap_or("no message");
            #{
                success: true,
                echo: message
            }
        }
    "#;

    std::fs::write(tool_dir.join("debug_rune_tool.rn"), test_tool_source).unwrap();

    // Create tool registry
    let registry = ToolRegistry::new_with_stdlib(tool_dir, database.clone(), obsidian)
        .expect("Failed to create tool registry");

    // Verify registry has the tool
    assert!(registry.has_tool("debug_rune_tool"), "Registry should have debug_rune_tool");
    println!("✅ Tool registry contains debug_rune_tool");

    // Create MCP service with Rune tools
    let provider: Arc<dyn EmbeddingProvider> = Arc::new(TestEmbeddingProvider);
    let service = CrucibleMcpService::with_rune_tools(database, provider, registry);

    // Test listing tools - this should trigger our debugging logs
    println!("🔍 Calling list_tools() to trigger debugging...");
    let tool_list_result = service.list_tools(None, RequestContext::empty()).await;

    match tool_list_result {
        Ok(tool_list) => {
            println!("✅ Service returned tool list successfully");
            println!("Total tools: {}", tool_list.tools.len());

            // Print all tool names
            let tool_names: Vec<_> = tool_list.tools.iter()
                .map(|tool| tool.name.as_ref())
                .collect();
            println!("Available tools: {:?}", tool_names);

            // Check if our Rune tool is in the list
            if tool_names.contains(&"debug_rune_tool") {
                println!("✅ debug_rune_tool found in MCP tool list");

                // Verify tool metadata
                if let Some(tool) = tool_list.tools.iter().find(|t| t.name.as_ref() == "debug_rune_tool") {
                    assert_eq!(tool.description.as_ref().unwrap(), "A test Rune tool for debugging MCP discovery");
                    println!("✅ Tool metadata is correct");
                }
            } else {
                panic!(
                    "❌ debug_rune_tool NOT found in MCP tool list.\n\
                     Available tools: {:?}\n\
                     Expected: debug_rune_tool should be automatically discovered and listed.\n\
                     Check the debug logs above to see what went wrong.",
                    tool_names
                );
            }

            // Should also have native tools
            let native_tools = ["search_by_properties", "search_by_tags", "list_notes_in_folder",
                              "search_by_filename", "search_by_content", "semantic_search",
                              "build_search_index", "get_note_metadata", "update_note_properties",
                              "get_vault_stats", "__run_rune_tool"];

            for native_tool in &native_tools {
                if tool_names.contains(&*native_tool) {
                    println!("✅ Native tool {} found", native_tool);
                } else {
                    println!("⚠️  Native tool {} missing", native_tool);
                }
            }
        }
        Err(e) => {
            panic!("❌ Failed to list tools: {}", e);
        }
    }
}