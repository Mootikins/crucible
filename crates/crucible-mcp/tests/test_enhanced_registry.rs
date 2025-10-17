/// Enhanced Registry Integration Tests
///
/// Tests the enhanced ToolRegistry with AST-based discovery and schema generation.

use anyhow::Result;
use crucible_mcp::rune_tools::ToolRegistry;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

/// Create a test file with multiple modules for enhanced discovery
fn create_multi_module_test_file() -> String {
    r#"
/// File operations module
pub mod file {
    /// Create a file with content
    pub async fn create_file(args) {
        #{
            success: true,
            file: args.path,
            size: args.content.len()
        }
    }

    /// Read file contents
    pub async fn read_file(args) {
        "Sample content for testing"
    }
}

/// UI helpers module
pub mod ui {
    /// Format results in different styles
    pub async fn format_results(args) {
        if args.format == "json" {
            "{\"formatted\": \"json\"}"
        } else {
            "formatted results"
        }
    }

    /// Get suggestions
    pub async fn get_suggestions(args) {
        ["suggestion1", "suggestion2"]
    }
}

/// Agent tools module
pub mod agent {
    /// Analyze data patterns
    pub async fn analyze_data(args) {
        #{
            patterns: ["pattern1", "pattern2"],
            confidence: 0.95
        }
    }

    /// Recommend actions
    pub async fn recommend(args) {
        #{
            actions: ["action1", "action2"],
            reasoning: "Based on analysis"
        }
    }
}
"#.to_string()
}

#[tokio::test]
async fn test_enhanced_registry_backwards_compatibility() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create a traditional-style tool file for backwards compatibility
    let test_content = r#"
pub fn NAME() { "test_tool" }
pub fn DESCRIPTION() { "A test tool for registry validation" }
pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{ message: #{ type: "string" } },
        required: ["message"]
    }
}

pub async fn call(args) {
    #{ success: true, message: args.message }
}
"#;

    let test_file_path = tool_dir.join("test_tool.rn");
    std::fs::write(&test_file_path, test_content)?;

    // Create enhanced registry (currently uses fallback loading)
    let context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new_with_enhanced_discovery(tool_dir.clone(), context, true)?;

    // Verify tools were discovered using traditional loading
    let tool_count = registry.tool_count();
    println!("Registry discovered {} tools", tool_count);

    assert_eq!(tool_count, 1, "Should discover one traditional tool");

    // Verify tool metadata
    let tools = registry.list_tools();
    assert_eq!(tools.len(), 1);

    let tool = &tools[0];
    assert_eq!(tool.name, "test_tool");
    assert_eq!(tool.description, "A test tool for registry validation");

    // Verify schema structure
    assert_eq!(tool.input_schema["type"], "object");
    assert!(tool.input_schema["properties"].is_object());

    Ok(())
}

#[tokio::test]
async fn test_enhanced_registry_structure() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create enhanced registry without tools to test structure
    let context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new_with_enhanced_discovery(tool_dir.clone(), context, true)?;

    // Verify registry structure
    assert_eq!(registry.tool_count(), 0);
    assert!(registry.tool_dir.exists());

    // Test registry methods
    assert_eq!(registry.tool_names().len(), 0);
    assert_eq!(registry.list_tools().len(), 0);

    Ok(())
}

// TODO: Re-enable these tests when async initialization is properly supported
// #[tokio::test]
// async fn test_traditional_vs_enhanced_discovery() { ... }

// #[tokio::test]
// async fn test_registry_error_handling() { ... }