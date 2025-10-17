// Test MCP Tool Discovery for Rune Tools
//
// This test checks if Rune tools are properly exposed in the MCP tool list.
// It should initially fail and pass after implementing proper tool discovery.

use crucible_mcp::CrucibleMcpService;
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::rune_tools::ToolRegistry;
use rmcp::handler::server::ServerHandler;
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
async fn test_mcp_service_lists_rune_tools() {
    // Test that the MCP service properly lists Rune tools in its tool list

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
        pub fn NAME() { "test_rune_tool" }
        pub fn DESCRIPTION() { "A test Rune tool for MCP discovery" }
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

    std::fs::write(tool_dir.join("test_rune_tool.rn"), test_tool_source).unwrap();

    // Create tool registry
    let registry = ToolRegistry::new_with_stdlib(tool_dir, database.clone(), obsidian)
        .expect("Failed to create tool registry");

    // Verify registry has the tool
    assert!(registry.has_tool("test_rune_tool"), "Registry should have test_rune_tool");
    println!("✅ Tool registry contains test_rune_tool");

    // Create MCP service with Rune tools
    let provider: Arc<dyn EmbeddingProvider> = Arc::new(TestEmbeddingProvider);
    let service = CrucibleMcpService::with_rune_tools(database, provider, registry);

    // Test listing tools
    // Simplified approach - create basic request context
    let tool_list_result = service.list_tools(None, rmcp::service::RequestContext::empty()).await;

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
            if tool_names.contains(&"test_rune_tool") {
                println!("✅ test_rune_tool found in MCP tool list");

                // Verify tool metadata
                if let Some(tool) = tool_list.tools.iter().find(|t| t.name.as_ref() == "test_rune_tool") {
                    assert_eq!(tool.description.as_ref().unwrap(), "A test Rune tool for MCP discovery");
                    println!("✅ Tool metadata is correct");
                }
            } else {
                panic!(
                    "❌ test_rune_tool NOT found in MCP tool list.\n\
                     Available tools: {:?}\n\
                     Expected: test_rune_tool should be automatically discovered and listed.",
                    tool_names
                );
            }

            // Should also have native tools
            let native_tools = ["search_by_properties", "search_by_tags", "list_notes_in_folder",
                              "search_by_filename", "search_by_content", "semantic_search",
                              "build_search_index", "get_note_metadata", "update_note_properties",
                              "get_vault_stats"];

            for native_tool in &native_tools {
                if tool_names.contains(&**native_tool) {
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

#[tokio::test]
async fn test_rune_tool_schema_conversion() {
    // Test that Rune tool INPUT_SCHEMA is properly converted to MCP schema

    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();

    // Create tool with complex schema
    let complex_tool_source = r#"
        pub fn NAME() { "complex_tool" }
        pub fn DESCRIPTION() { "Tool with complex schema" }
        pub fn INPUT_SCHEMA() {
            #{
                type: "object",
                properties: #{
                    title: #{ type: "string", description: "Document title" },
                    content: #{ type: "string", description: "Document content" },
                    tags: #{
                        type: "array",
                        items: #{ type: "string" },
                        description: "Document tags"
                    },
                    priority: #{
                        type: "number",
                        description: "Priority level",
                        minimum: 1,
                        maximum: 10
                    },
                    draft: #{
                        type: "boolean",
                        description: "Is this a draft?",
                        default: false
                    }
                },
                required: ["title", "content"]
            }
        }

        pub async fn call(args) {
            #{ success: true }
        }
    "#;

    std::fs::write(tools_dir.join("complex_tool.rn"), complex_tool_source).unwrap();

    // Set up database and registry
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    let obsidian = match ObsidianClient::new() {
        Ok(client) => Arc::new(client),
        Err(_) => {
            println!("⚠️  Skipping test - Obsidian client not available");
            return;
        }
    };

    let registry = ToolRegistry::new_with_stdlib(tools_dir, database, obsidian)
        .expect("Failed to create tool registry");

    // Get tool metadata
    if registry.has_tool("complex_tool") {
        let tool = registry.get_tool("complex_tool").unwrap();
        println!("✅ Complex tool loaded");
        println!("Input schema: {}", serde_json::to_string_pretty(&tool.input_schema).unwrap());

        // Verify schema structure
        let schema = &tool.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());

        let properties = schema["properties"].as_object().unwrap();
        assert!(properties.contains_key("title"));
        assert!(properties.contains_key("content"));
        assert!(properties.contains_key("tags"));
        assert!(properties.contains_key("priority"));
        assert!(properties.contains_key("draft"));

        println!("✅ Schema validation passed");
    } else {
        panic!("❌ Complex tool not loaded");
    }
}

#[test]
fn test_rune_tool_ast_parsing() {
    // Test that Rune AST can be parsed to extract metadata

    let test_tool_source = r#"
        pub fn NAME() { "ast_test_tool" }
        pub fn DESCRIPTION() { "Tool for AST parsing test" }
        pub fn INPUT_SCHEMA() {
            #{
                type: "object",
                properties: #{
                    message: #{ type: "string" }
                }
            }
        }

        pub async fn call(args) {
            #{ success: true }
        }
    "#;

    let context = Arc::new(rune::Context::with_default_modules().unwrap());

    // Test that we can extract metadata from source
    match crucible_mcp::rune_tools::RuneTool::from_source(test_tool_source, &context) {
        Ok(tool) => {
            assert_eq!(tool.name, "ast_test_tool");
            assert_eq!(tool.description, "Tool for AST parsing test");

            let schema = &tool.input_schema;
            assert_eq!(schema["type"], "object");
            assert!(schema["properties"]["message"]["type"] == "string");

            println!("✅ AST parsing successful");
            println!("Tool name: {}", tool.name);
            println!("Description: {}", tool.description);
            println!("Schema: {}", serde_json::to_string_pretty(&tool.input_schema).unwrap());
        }
        Err(e) => {
            panic!("❌ AST parsing failed: {}", e);
        }
    }
}