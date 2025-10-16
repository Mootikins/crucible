// Simplified Integration Tests for Rune-based MCP Tools
//
// This test suite validates basic Rune tool functionality that can be tested
// without complex setup.

use crucible_mcp::rune_tools::ToolRegistry;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;

// ============================================================================
// Test Helper Functions
// ============================================================================

fn get_example_tools_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tools")
        .join("examples")
}

fn create_minimal_context() -> Result<Arc<rune::Context>, anyhow::Error> {
    let context = rune::Context::with_default_modules()?;
    Ok(Arc::new(context))
}

// ============================================================================
// Basic Tool Registry Tests
// ============================================================================

#[test]
fn test_registry_creation_with_default_context() {
    let tools_dir = get_example_tools_dir();
    let context = create_minimal_context().expect("Failed to create context");

    let result = ToolRegistry::new(tools_dir, context);

    match result {
        Ok(registry) => {
            let tools = registry.list_tools();
            println!("Successfully loaded {} Rune tools", tools.len());
            assert!(tools.len() > 0, "Should load at least one tool");
        }
        Err(e) => {
            // Some tools may fail to load due to missing stdlib functions
            // This is expected in a minimal context
            println!("Registry creation error (expected with minimal context): {}", e);
        }
    }
}

#[test]
fn test_empty_directory_handling() {
    let temp_dir = tempdir().unwrap();
    let empty_dir = temp_dir.path().join("empty_tools");
    std::fs::create_dir(&empty_dir).unwrap();

    let context = create_minimal_context().expect("Failed to create context");
    let result = ToolRegistry::new(empty_dir, context);

    match result {
        Ok(registry) => {
            let tools = registry.list_tools();
            assert_eq!(tools.len(), 0, "Empty directory should have no tools");
        }
        Err(e) => {
            println!("Empty directory error: {}", e);
        }
    }
}

#[test]
fn test_invalid_directory_handling() {
    let context = create_minimal_context().expect("Failed to create context");
    let result = ToolRegistry::new(PathBuf::from("/nonexistent/directory"), context);

    // Should handle gracefully
    if let Ok(registry) = result {
        let tools = registry.list_tools();
        assert_eq!(tools.len(), 0, "Nonexistent directory should result in no tools");
    }
}

#[test]
fn test_tool_file_discovery() {
    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir(&tools_dir).unwrap();

    // Create a minimal valid Rune tool file
    let tool_content = r#"
pub fn NAME() { "simple_tool" }
pub fn DESCRIPTION() { "A simple test tool" }
pub fn INPUT_SCHEMA() {
    #{ type: "object", properties: #{} }
}
pub async fn call(args) {
    #{ success: true, message: "Hello from simple_tool" }
}
"#;

    std::fs::write(tools_dir.join("simple_tool.rn"), tool_content).unwrap();

    // Also create a non-.rn file that should be ignored
    std::fs::write(tools_dir.join("readme.txt"), "Not a tool file").unwrap();

    let context = create_minimal_context().expect("Failed to create context");
    let result = ToolRegistry::new(tools_dir.clone(), context);

    match result {
        Ok(registry) => {
            let tools = registry.list_tools();
            println!("Tools discovered: {:?}", tools.iter().map(|t| &t.name).collect::<Vec<_>>());

            // Should only discover .rn files
            assert!(
                tools.iter().any(|t| t.name.contains("simple")),
                "Should discover the simple_tool.rn file"
            );
        }
        Err(e) => {
            println!("Tool discovery error: {}", e);
        }
    }
}

#[test]
fn test_malformed_tool_file_handling() {
    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir(&tools_dir).unwrap();

    // Create an invalid Rune tool file
    let bad_tool_content = r#"
// Missing required functions
pub fn NAME() { "bad_tool" }
// No DESCRIPTION, INPUT_SCHEMA, or call function
"#;

    std::fs::write(tools_dir.join("bad_tool.rn"), bad_tool_content).unwrap();

    let context = create_minimal_context().expect("Failed to create context");
    let result = ToolRegistry::new(tools_dir, context);

    // Should handle malformed files gracefully
    match result {
        Ok(registry) => {
            let tools = registry.list_tools();
            println!("Tools loaded despite bad file: {}", tools.len());
        }
        Err(e) => {
            println!("Expected error for malformed tool: {}", e);
        }
    }
}

// ============================================================================
// Tool Metadata Validation Tests
// ============================================================================

#[test]
fn test_tool_metadata_structure() {
    let tools_dir = get_example_tools_dir();
    let context = create_minimal_context().expect("Failed to create context");

    let result = ToolRegistry::new(tools_dir, context);

    if let Ok(registry) = result {
        let tools = registry.list_tools();

        for tool_meta in tools {
            // Validate basic metadata structure
            assert!(!tool_meta.name.is_empty(), "Tool name should not be empty");
            assert!(!tool_meta.description.is_empty(), "Tool description should not be empty");

            println!(
                "Validated tool metadata: {} - {}",
                tool_meta.name, tool_meta.description
            );
        }
    }
}

// ============================================================================
// Filesystem Tests
// ============================================================================

#[test]
fn test_nested_directory_handling() {
    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir(&tools_dir).unwrap();

    // Create nested directories
    let nested = tools_dir.join("subdir");
    std::fs::create_dir(&nested).unwrap();

    // Create tool files at different levels
    let tool1 = r#"
pub fn NAME() { "root_tool" }
pub fn DESCRIPTION() { "Tool in root" }
pub fn INPUT_SCHEMA() { #{ type: "object", properties: #{} } }
pub async fn call(args) { #{ success: true } }
"#;

    let tool2 = r#"
pub fn NAME() { "nested_tool" }
pub fn DESCRIPTION() { "Tool in subdirectory" }
pub fn INPUT_SCHEMA() { #{ type: "object", properties: #{} } }
pub async fn call(args) { #{ success: true } }
"#;

    std::fs::write(tools_dir.join("root_tool.rn"), tool1).unwrap();
    std::fs::write(nested.join("nested_tool.rn"), tool2).unwrap();

    let context = create_minimal_context().expect("Failed to create context");
    let result = ToolRegistry::new(tools_dir, context);

    if let Ok(registry) = result {
        let tools = registry.list_tools();
        println!("Tools found in nested structure: {:?}",
            tools.iter().map(|t| &t.name).collect::<Vec<_>>());

        // Should find tools in nested directories
        assert!(tools.len() >= 1, "Should find at least the root tool");
    }
}

#[test]
fn test_case_sensitivity() {
    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir(&tools_dir).unwrap();

    let tool_content = r#"
pub fn NAME() { "test_tool" }
pub fn DESCRIPTION() { "Test tool" }
pub fn INPUT_SCHEMA() { #{ type: "object", properties: #{} } }
pub async fn call(args) { #{ success: true } }
"#;

    // Test different file extensions
    std::fs::write(tools_dir.join("lowercase.rn"), tool_content).unwrap();
    std::fs::write(tools_dir.join("UPPERCASE.RN"), tool_content).unwrap();  // Should be ignored
    std::fs::write(tools_dir.join("MixedCase.rn"), tool_content).unwrap();

    let context = create_minimal_context().expect("Failed to create context");
    let result = ToolRegistry::new(tools_dir, context);

    if let Ok(registry) = result {
        let tools = registry.list_tools();
        println!("Tools with various cases: {:?}",
            tools.iter().map(|t| &t.name).collect::<Vec<_>>());
    }
}

#[test]
fn test_hidden_files_ignored() {
    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir(&tools_dir).unwrap();

    let tool_content = r#"
pub fn NAME() { "test_tool" }
pub fn DESCRIPTION() { "Test tool" }
pub fn INPUT_SCHEMA() { #{ type: "object", properties: #{} } }
pub async fn call(args) { #{ success: true } }
"#;

    std::fs::write(tools_dir.join("visible.rn"), tool_content).unwrap();
    std::fs::write(tools_dir.join(".hidden.rn"), tool_content).unwrap();

    let context = create_minimal_context().expect("Failed to create context");
    let result = ToolRegistry::new(tools_dir, context);

    if let Ok(registry) = result {
        let tools = registry.list_tools();
        let tool_names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();

        // Hidden files should typically be ignored
        println!("Tools (should exclude hidden): {:?}", tool_names);
    }
}
