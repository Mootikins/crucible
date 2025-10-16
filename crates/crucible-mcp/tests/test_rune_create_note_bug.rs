// Test cases for Bug #2: Rune Tool 'create_note' Not Found
//
// These tests reproduce the missing 'create_note' Rune tool error.
// They should initially fail and pass after implementing the create_note tool.

use crucible_mcp::{CrucibleMcpService, EmbeddingConfig, create_provider};
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::rune_tools::ToolRegistry;
use crucible_mcp::types::RuneToolParams;
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
async fn test_rune_create_note_tool_exists() {
    // This test reproduces Bug #2: Rune Tool 'create_note' Not Found
    //
    // Expected: The 'create_note' tool should exist and be callable
    // Actual (before fix): "Rune tool 'create_note' not found" error

    let temp_dir = tempdir().unwrap();
    let vault_path = temp_dir.path();

    // Set up database
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    // Create mock obsidian client
    let obsidian = Arc::new(ObsidianClient::new().expect("Failed to create Obsidian client"));

    // Set up Rune tool registry
    let tool_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tool_dir).unwrap();

    let registry = ToolRegistry::new_with_stdlib(tool_dir, database.clone(), obsidian.clone())
        .expect("Failed to create tool registry");

    // Set up MCP service with Rune tools
    let provider: Arc<dyn EmbeddingProvider> = Arc::new(TestEmbeddingProvider);
    let service = CrucibleMcpService::with_rune_tools(database, provider, registry);

    // Test calling the create_note tool via the __run_rune_tool dispatcher
    let params = RuneToolParams {
        tool_name: "create_note".to_string(),
        args: serde_json::json!({
            "title": "Test Note",
            "content": "# Test Note\n\nThis is a test note created via Rune tool.",
            "folder": "test-folder",
            "tags": ["test", "rune-tool"]
        }),
    };

    // This should succeed but currently fails because create_note tool doesn't exist
    // Since __run_rune_tool is private, we need to test through the service interface
    let result = service.call_tool(
        rmcp::model::CallToolRequest {
            name: "__run_rune_tool".to_string(),
            arguments: Some(serde_json::to_value(params).unwrap()),
        }
    ).await;

    match result {
        Ok(call_result) => {
            // Check if the tool call was successful
            if call_result.is_error {
                let error_content = call_result.content.first()
                    .and_then(|c| c.as_text())
                    .unwrap_or("Unknown error");

                if error_content.contains("Rune tool 'create_note' not found") {
                    panic!(
                        "❌ Bug #2 confirmed: Rune tool 'create_note' not found.\n\
                         Error: {}\n\
                         Expected: create_note tool should exist and be callable.\n\
                         Actual: Tool is missing from registry.",
                        error_content
                    );
                } else {
                    panic!(
                        "❌ Rune tool call failed with unexpected error: {}\n\
                         This might indicate a different issue with the Rune tool system.",
                        error_content
                    );
                }
            } else {
                // Success case - the tool exists and worked
                let success_content = call_result.content.first()
                    .and_then(|c| c.as_text())
                    .unwrap_or("No content");
                println!("✅ create_note tool succeeded: {}", success_content);
            }
        }
        Err(e) => {
            panic!(
                "❌ MCP protocol error when calling create_note: {}.\n\
                 This indicates a more fundamental issue with the MCP service.",
                e
            );
        }
    }
}

#[tokio::test]
async fn test_rune_tool_registry_contains_create_note() {
    // Direct test of the tool registry to check if create_note tool is loaded
    let temp_dir = tempdir().unwrap();
    let vault_path = temp_dir.path();

    // Set up database
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    // Create mock obsidian client
    let obsidian = Arc::new(ObsidianClient::new().expect("Failed to create Obsidian client"));

    // Set up Rune tool registry
    let tool_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tool_dir).unwrap();

    // Create a simple create_note.rn file for testing
    let create_note_source = r#"
        pub fn NAME() { "create_note" }
        pub fn DESCRIPTION() { "Create a new note in the vault" }
        pub fn INPUT_SCHEMA() {
            #{
                type: "object",
                properties: #{
                    title: #{ type: "string", description: "Note title" },
                    content: #{ type: "string", description: "Note content" },
                    folder: #{ type: "string", description: "Folder to create note in" },
                    tags: #{ type: "array", items: #{ type: "string" }, description: "Tags for the note" }
                },
                required: ["title", "content"]
            }
        }

        pub async fn call(args) {
            let title = args.get("title").unwrap_or("Untitled");
            let content = args.get("content").unwrap_or("");
            let folder = args.get("folder").unwrap_or("");
            let tags = args.get("tags").unwrap_or(#[]);

            // Create the note file
            let file_path = if folder.is_empty() {
                format!("{}.md", title)
            } else {
                format!("{}/{}.md", folder, title)
            };

            // In a real implementation, this would create the file in the vault
            // For now, just return success
            #{
                success: true,
                file_path: file_path,
                message: format!("Created note '{}'", title)
            }
        }
    "#;

    std::fs::write(tool_dir.join("create_note.rn"), create_note_source).unwrap();

    let registry = ToolRegistry::new_with_stdlib(tool_dir, database, obsidian)
        .expect("Failed to create tool registry");

    // Check if create_note tool is loaded
    if registry.has_tool("create_note") {
        println!("✅ create_note tool found in registry");

        // Test getting the tool
        let tool = registry.get_tool("create_note").unwrap();
        assert_eq!(tool.name, "create_note");
        assert_eq!(tool.description, "Create a new note in the vault");
        println!("✅ create_note tool metadata is correct");
    } else {
        let available_tools = registry.tool_names();
        panic!(
            "❌ Bug #2 confirmed: create_note tool not found in registry.\n\
             Available tools: {:?}\n\
             Expected: create_note tool should be loaded from .rn file.\n\
             Actual: Tool is missing despite .rn file existing.",
            available_tools
        );
    }
}

#[tokio::test]
async fn test_rune_list_available_tools() {
    // Test listing available Rune tools to see what's actually loaded
    let temp_dir = tempdir().unwrap();

    // Set up database
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    // Create mock obsidian client
    let obsidian = Arc::new(ObsidianClient::new().expect("Failed to create Obsidian client"));

    // Set up Rune tool registry
    let tool_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tool_dir).unwrap();

    // Create a simple test tool
    let test_tool_source = r#"
        pub fn NAME() { "test_tool" }
        pub fn DESCRIPTION() { "A simple test tool" }
        pub fn INPUT_SCHEMA() {
            #{ type: "object", properties: #{ name: #{ type: "string" } } }
        }

        pub async fn call(args) {
            #{ success: true, message: "Test tool called" }
        }
    "#;

    std::fs::write(tool_dir.join("test_tool.rn"), test_tool_source).unwrap();

    let registry = ToolRegistry::new_with_stdlib(tool_dir, database, obsidian)
        .expect("Failed to create tool registry");

    let available_tools = registry.tool_names();

    println!("Available Rune tools: {:?}", available_tools);

    // Check if create_note is among available tools
    if available_tools.contains(&"create_note".to_string()) {
        println!("✅ create_note tool is available");
    } else {
        println!("⚠️  create_note tool is NOT available (this is expected before the fix)");
        println!("   Available tools: {:?}", available_tools);
    }

    // At minimum, we should have some tools loaded
    assert!(
        !available_tools.is_empty(),
        "Expected at least some Rune tools to be loaded"
    );
}

#[tokio::test]
async fn test_rune_create_note_missing_file() {
    // Test what happens when create_note.rn file doesn't exist
    let temp_dir = tempdir().unwrap();

    // Set up database
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    // Create mock obsidian client
    let obsidian = Arc::new(ObsidianClient::new().expect("Failed to create Obsidian client"));

    // Set up Rune tool registry WITHOUT create_note.rn file
    let tool_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tool_dir).unwrap();

    let registry = ToolRegistry::new_with_stdlib(tool_dir, database, obsidian)
        .expect("Failed to create tool registry");

    // Verify create_note tool is not present (this is the current buggy state)
    assert!(
        !registry.has_tool("create_note"),
        "create_note tool should not be present when .rn file doesn't exist"
    );

    println!("⚠️  Confirmed: create_note tool missing when .rn file doesn't exist");
    println!("    This represents the current buggy state that needs to be fixed");
}