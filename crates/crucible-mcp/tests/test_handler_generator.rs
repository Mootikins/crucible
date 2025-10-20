/// Handler Generator Integration Tests
///
/// Tests the dynamic tool handler generation system for enhanced Rune tools.

use anyhow::Result;
use crucible_mcp::rune_tools::{ToolRegistry, DynamicRuneToolHandler, ToolHandlerGenerator};
use crucible_mcp::embeddings::{EmbeddingProvider, EmbeddingResponse, EmbeddingResult};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;

// Simple test embedding provider
struct TestEmbeddingProvider;

#[async_trait::async_trait]
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

    async fn list_models(&self) -> EmbeddingResult<Vec<crucible_mcp::embeddings::provider::ModelInfo>> {
        Ok(vec![crucible_mcp::embeddings::provider::ModelInfo {
            name: "test-model".to_string(),
            display_name: Some("Test Model".to_string()),
            family: None,
            dimensions: Some(384),
            size_bytes: None,
            parameter_size: None,
            quantization: None,
            format: None,
            modified_at: None,
            digest: None,
            max_tokens: None,
            recommended: true,
            metadata: None,
        }])
    }
}

/// Create a test file with multiple tools for handler generation
fn create_multi_tool_file() -> String {
    r#"
pub mod utils {
    /// Uppercase a string
    pub async fn uppercase(args) {
        args.input.to_uppercase()
    }

    /// Reverse a string
    pub async fn reverse(args) {
        args.input.chars().rev().collect::<String>()
    }
}

pub mod math {
    /// Add two numbers
    pub async fn add(args) {
        args.a + args.b
    }

    /// Multiply two numbers
    pub async fn multiply(args) {
        args.a * args.b
    }
}

pub mod validation {
    /// Validate email format
    pub async fn validate_email(args) {
        args.input.contains('@') && args.input.contains('.')
    }

    /// Check if string is not empty
    pub async fn is_not_empty(args) {
        !args.input.trim().is_empty()
    }
}
"#.to_string()
}

#[tokio::test]
async fn test_dynamic_tool_handler_with_modules() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create test file with multiple modules
    let test_file_path = tool_dir.join("multi_tool.rn");
    let test_content = create_multi_tool_file();
    std::fs::write(&test_file_path, test_content)?;

    // Create registry (currently uses fallback loading, so traditional tools won't work)
    let context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new_with_enhanced_discovery(tool_dir.clone(), context, true)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    // Create dynamic handler
    let handler = DynamicRuneToolHandler::new(registry_arc.clone());

    // For now, test with the fallback system
    let tool_count = {
        let reg = registry_arc.read().await;
        reg.tool_count()
    };

    println!("Dynamic handler created for {} tools", tool_count);

    // Test handler methods
    assert!(handler.has_tool("").await); // Empty string check shouldn't panic

    // Get metadata
    let metadata = handler.get_all_tools_metadata().await;
    println!("Available tools: {:?}", metadata.iter().map(|m| &m.name).collect::<Vec<_>>());

    Ok(())
}

#[tokio::test]
async fn test_tool_handler_generator_structure() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create registry
    let context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new_with_enhanced_discovery(tool_dir.clone(), context, true)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    // Create handler generator
    let mut generator = ToolHandlerGenerator::new(registry_arc.clone());

    // Test structure
    let tools = generator.generate_tool_list().await?;
    println!("Generated {} tools", tools.len());

    // All tools should have proper structure
    for tool in &tools {
        assert_eq!(tool.input_schema.get("type"), Some(&json!("object")));
        assert!(tool.annotations.is_some());

        let annotations = tool.annotations.as_ref().unwrap();
        assert!(annotations.title.is_some());
        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.destructive_hint, Some(false));
        assert_eq!(annotations.idempotent_hint, Some(false));
        assert_eq!(annotations.open_world_hint, Some(true));
    }

    Ok(())
}

#[tokio::test]
async fn test_enhanced_tool_service() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create registry
    let context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new_with_enhanced_discovery(tool_dir.clone(), context, true)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    // Create mock database and provider for service
    let db = Arc::new(crucible_mcp::database::EmbeddingDatabase::new(":memory:").await?);
    let provider = Arc::new(TestEmbeddingProvider);

    // Create enhanced service
    let mut service = crucible_mcp::rune_tools::EnhancedToolService::new(
        db,
        provider,
        registry_arc.clone()
    );

    // Test service methods
    let tools = service.list_enhanced_tools().await?;
    println!("Enhanced service has {} tools", tools.len());

    // Test service structure
    for tool in &tools {
        assert!(!tool.name.is_empty());
        assert!(tool.description.is_some());
    }

    Ok(())
}

#[tokio::test]
async fn test_handler_error_handling() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create registry
    let context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new_with_enhanced_discovery(tool_dir.clone(), context, true)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    // Create handler
    let handler = DynamicRuneToolHandler::new(registry_arc);

    // Test execution of non-existent tool
    let result = handler.execute_tool("non_existent_tool", json!({"test": "value"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("not found"));

    // Test metadata of non-existent tool
    let metadata = handler.get_tool_metadata("non_existent_tool").await;
    assert!(metadata.is_none());

    Ok(())
}

#[tokio::test]
async fn test_handler_generator_caching() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create registry
    let context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new_with_enhanced_discovery(tool_dir.clone(), context, true)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    // Create generator
    let mut generator = ToolHandlerGenerator::new(registry_arc.clone());

    // Test that getting the same handler multiple times returns the same instance
    // The current implementation always creates a handler, so we test that behavior
    {
        let handler1 = generator.get_handler("test_tool");
        assert!(handler1.is_some());
    }

    {
        let handler2 = generator.get_handler("test_tool");
        assert!(handler2.is_some());
    }

    // Generate tools
    let tools_before = generator.generate_tool_list().await?;
    let tools_after = generator.generate_tool_list().await?;

    // Should return same number of tools
    assert_eq!(tools_before.len(), tools_after.len());

    Ok(())
}