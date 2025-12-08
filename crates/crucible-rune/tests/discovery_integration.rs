//! Integration tests for Rune tool discovery
//!
//! Tests the full discovery pipeline including:
//! - Multi-tool files with #[tool] attributes
//! - Legacy single-tool files
//! - Recursive directory scanning
//! - Multiple directories (global + kiln overlay)
//! - Edge cases and error handling

use crucible_rune::{RuneDiscoveryConfig, RuneToolRegistry, ToolDiscovery};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test directory with rune files
fn setup_test_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp dir")
}

// =============================================================================
// Multi-tool file tests
// =============================================================================

#[test]
fn test_multi_tool_file_with_all_param_types() {
    let temp = setup_test_dir();
    let tool_file = temp.path().join("all_types.rn");

    fs::write(
        &tool_file,
        r#"
/// String parameter tool
#[tool(desc = "Process text")]
#[param(name = "text", type = "string", desc = "Input text")]
pub fn process_text(text) {
    Ok(text.to_uppercase())
}

/// Integer parameter tool
#[tool(desc = "Process number")]
#[param(name = "count", type = "integer", desc = "Count value")]
#[param(name = "multiplier", type = "int", desc = "Multiplier")]
pub fn process_number(count, multiplier) {
    Ok(count * multiplier)
}

/// Boolean parameter tool
#[tool(desc = "Toggle feature")]
#[param(name = "enabled", type = "boolean", desc = "Feature flag")]
#[param(name = "force", type = "bool", desc = "Force mode")]
pub fn toggle_feature(enabled, force) {
    Ok(enabled || force)
}

/// Float parameter tool
#[tool(desc = "Calculate value")]
#[param(name = "value", type = "float", desc = "Input value")]
#[param(name = "precision", type = "number", desc = "Precision")]
pub fn calculate(value, precision) {
    Ok(value.round(precision))
}

/// Array parameter tool
#[tool(desc = "Process list")]
#[param(name = "items", type = "array", desc = "List of items")]
pub fn process_list(items) {
    Ok(items.len())
}

/// Object parameter tool
#[tool(desc = "Process config")]
#[param(name = "config", type = "object", desc = "Configuration object")]
pub fn process_config(config) {
    Ok(config)
}
"#,
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    assert_eq!(tools.len(), 6, "Should discover 6 tools");

    // Verify each tool has correct schema types
    let text_tool = tools.iter().find(|t| t.name == "process_text").unwrap();
    let schema = &text_tool.input_schema;
    assert_eq!(
        schema["properties"]["text"]["type"].as_str(),
        Some("string")
    );

    let number_tool = tools.iter().find(|t| t.name == "process_number").unwrap();
    let schema = &number_tool.input_schema;
    assert_eq!(
        schema["properties"]["count"]["type"].as_str(),
        Some("integer")
    );
    assert_eq!(
        schema["properties"]["multiplier"]["type"].as_str(),
        Some("integer")
    );

    let bool_tool = tools.iter().find(|t| t.name == "toggle_feature").unwrap();
    let schema = &bool_tool.input_schema;
    assert_eq!(
        schema["properties"]["enabled"]["type"].as_str(),
        Some("boolean")
    );

    let float_tool = tools.iter().find(|t| t.name == "calculate").unwrap();
    let schema = &float_tool.input_schema;
    assert_eq!(
        schema["properties"]["value"]["type"].as_str(),
        Some("number")
    );

    let array_tool = tools.iter().find(|t| t.name == "process_list").unwrap();
    let schema = &array_tool.input_schema;
    assert_eq!(
        schema["properties"]["items"]["type"].as_str(),
        Some("array")
    );

    let object_tool = tools.iter().find(|t| t.name == "process_config").unwrap();
    let schema = &object_tool.input_schema;
    assert_eq!(
        schema["properties"]["config"]["type"].as_str(),
        Some("object")
    );
}

#[test]
fn test_multi_tool_with_optional_params() {
    let temp = setup_test_dir();
    let tool_file = temp.path().join("optional_params.rn");

    fs::write(
        &tool_file,
        r#"
#[tool(desc = "Search with options")]
#[param(name = "query", type = "string", desc = "Search query")]
#[param(name = "limit", type = "integer", desc = "Max results", required = false)]
#[param(name = "offset", type = "integer", desc = "Start offset", required = false)]
pub fn search(query, limit, offset) {
    Ok(query)
}
"#,
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    assert_eq!(tools.len(), 1);

    let tool = &tools[0];
    let schema = &tool.input_schema;

    // Check required array - should only contain "query"
    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert_eq!(required[0].as_str(), Some("query"));

    // All three params should be in properties
    assert!(schema["properties"]["query"].is_object());
    assert!(schema["properties"]["limit"].is_object());
    assert!(schema["properties"]["offset"].is_object());
}

#[test]
fn test_multi_tool_with_tags() {
    let temp = setup_test_dir();
    let tool_file = temp.path().join("tagged_tools.rn");

    fs::write(
        &tool_file,
        r#"
#[tool(desc = "Note creation", tags = ["notes", "create"])]
pub fn create_note() {}

#[tool(desc = "Note search", tags = ["notes", "search", "query"])]
pub fn search_notes() {}

#[tool(desc = "No tags")]
pub fn plain_tool() {}
"#,
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    assert_eq!(tools.len(), 3);

    let create_tool = tools.iter().find(|t| t.name == "create_note").unwrap();
    assert!(create_tool.tags.contains(&"notes".to_string()));
    assert!(create_tool.tags.contains(&"create".to_string()));

    let search_tool = tools.iter().find(|t| t.name == "search_notes").unwrap();
    assert_eq!(search_tool.tags.len(), 3);
    assert!(search_tool.tags.contains(&"query".to_string()));

    let plain_tool = tools.iter().find(|t| t.name == "plain_tool").unwrap();
    assert!(plain_tool.tags.is_empty());
}

#[test]
fn test_async_function_tools() {
    let temp = setup_test_dir();
    let tool_file = temp.path().join("async_tools.rn");

    fs::write(
        &tool_file,
        r#"
#[tool(desc = "Sync tool")]
pub fn sync_tool() {}

#[tool(desc = "Async tool")]
pub async fn async_tool() {}

#[tool(desc = "Another async")]
pub async fn fetch_data() {}
"#,
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    assert_eq!(tools.len(), 3);

    // All should be discovered regardless of async/sync
    assert!(tools.iter().any(|t| t.name == "sync_tool"));
    assert!(tools.iter().any(|t| t.name == "async_tool"));
    assert!(tools.iter().any(|t| t.name == "fetch_data"));
}

// =============================================================================
// Legacy single-tool file tests
// =============================================================================

#[test]
fn test_legacy_format_full_metadata() {
    let temp = setup_test_dir();
    let tool_file = temp.path().join("legacy_full.rn");

    fs::write(
        &tool_file,
        r#"//! My legacy tool description
//! @entry run
//! @version 2.1.0
//! @tags processing, transform
//! @param input string The input text to process
//! @param count integer How many iterations

pub fn run(input, count) {
    for i in 0..count {
        println!("{}", input);
    }
}
"#,
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    assert_eq!(tools.len(), 1);

    let tool = &tools[0];
    assert_eq!(tool.name, "legacy_full");
    assert_eq!(tool.description, "My legacy tool description");
    assert_eq!(tool.entry_point, "run");
    assert_eq!(tool.version.as_deref(), Some("2.1.0"));
    assert!(tool.tags.contains(&"processing".to_string()));
    assert!(tool.tags.contains(&"transform".to_string()));

    // Check schema
    let schema = &tool.input_schema;
    assert!(schema["properties"]["input"].is_object());
    assert!(schema["properties"]["count"].is_object());
    assert_eq!(
        schema["properties"]["input"]["type"].as_str(),
        Some("string")
    );
    assert_eq!(
        schema["properties"]["count"]["type"].as_str(),
        Some("integer")
    );
}

#[test]
fn test_legacy_format_minimal() {
    let temp = setup_test_dir();
    let tool_file = temp.path().join("simple.rn");

    fs::write(
        &tool_file,
        r#"//! A simple greeting tool

pub fn main(name) {
    Ok(format!("Hello, {}!", name))
}
"#,
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "simple");
    assert_eq!(tools[0].description, "A simple greeting tool");
    assert_eq!(tools[0].entry_point, "main"); // Default entry point
}

// =============================================================================
// Directory structure tests
// =============================================================================

#[test]
fn test_recursive_discovery() {
    let temp = setup_test_dir();

    // Create nested directory structure
    let subdir1 = temp.path().join("notes");
    let subdir2 = temp.path().join("notes").join("advanced");
    let subdir3 = temp.path().join("utils");

    fs::create_dir_all(&subdir2).unwrap();
    fs::create_dir_all(&subdir3).unwrap();

    // Create tools at different levels
    fs::write(
        temp.path().join("root_tool.rn"),
        "//! Root level tool\npub fn main() {}",
    )
    .unwrap();
    fs::write(
        subdir1.join("notes_tool.rn"),
        "//! Notes tool\npub fn main() {}",
    )
    .unwrap();
    fs::write(
        subdir2.join("advanced_tool.rn"),
        "//! Advanced notes tool\npub fn main() {}",
    )
    .unwrap();
    fs::write(
        subdir3.join("util_tool.rn"),
        "//! Utility tool\npub fn main() {}",
    )
    .unwrap();

    // Test recursive discovery
    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: true,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    assert_eq!(tools.len(), 4, "Should find all 4 tools recursively");

    // Test non-recursive discovery
    let config_flat = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery_flat = ToolDiscovery::new(config_flat);
    let tools_flat = discovery_flat.discover_all().unwrap();

    assert_eq!(tools_flat.len(), 1, "Should find only root level tool");
    assert_eq!(tools_flat[0].name, "root_tool");
}

#[test]
fn test_multiple_directories_overlay() {
    let global_dir = setup_test_dir();
    let kiln_dir = setup_test_dir();

    // Global tools
    fs::write(
        global_dir.path().join("global_util.rn"),
        "//! Global utility\npub fn main() {}",
    )
    .unwrap();
    fs::write(
        global_dir.path().join("shared_tool.rn"),
        "//! Global shared tool\npub fn main() {}",
    )
    .unwrap();

    // Kiln-specific tools
    fs::write(
        kiln_dir.path().join("kiln_tool.rn"),
        "//! Kiln specific tool\npub fn main() {}",
    )
    .unwrap();
    fs::write(
        kiln_dir.path().join("shared_tool.rn"),
        "//! Kiln override of shared tool\npub fn main() {}",
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![
            global_dir.path().to_path_buf(),
            kiln_dir.path().to_path_buf(),
        ],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    // Should discover all 4 files (duplicate names allowed at discovery level)
    assert_eq!(tools.len(), 4);

    // Both shared_tool instances should be discovered
    let shared_tools: Vec<_> = tools.iter().filter(|t| t.name == "shared_tool").collect();
    assert_eq!(shared_tools.len(), 2);
}

#[test]
fn test_multiple_extensions() {
    let temp = setup_test_dir();

    fs::write(
        temp.path().join("tool1.rn"),
        "//! Rune file\npub fn main() {}",
    )
    .unwrap();
    fs::write(
        temp.path().join("tool2.rune"),
        "//! Rune file with .rune extension\npub fn main() {}",
    )
    .unwrap();
    fs::write(
        temp.path().join("script.py"),
        "# Python file - should be ignored",
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string(), "rune".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    assert_eq!(tools.len(), 2);
    assert!(tools.iter().any(|t| t.name == "tool1"));
    assert!(tools.iter().any(|t| t.name == "tool2"));
}

// =============================================================================
// Edge cases and error handling
// =============================================================================

#[test]
fn test_empty_file() {
    let temp = setup_test_dir();
    fs::write(temp.path().join("empty.rn"), "").unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    // Empty file should be discovered with just name, empty description
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "empty");
}

#[test]
fn test_file_with_only_comments() {
    let temp = setup_test_dir();
    fs::write(
        temp.path().join("comments_only.rn"),
        r#"// Regular comment
// Another comment
/* Block comment */
"#,
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    // Should be discovered as legacy format with empty description
    assert_eq!(tools.len(), 1);
}

#[test]
fn test_malformed_tool_attribute() {
    let temp = setup_test_dir();
    fs::write(
        temp.path().join("malformed.rn"),
        r#"
#[tool(desc = "Valid tool")]
pub fn valid_tool() {}

#[tool(desc = "Missing closing quote)]
pub fn bad_tool() {}

#[tool(desc = "Another valid tool")]
pub fn another_valid() {}
"#,
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    // Should still discover valid tools, skip malformed
    assert!(tools.iter().any(|t| t.name == "valid_tool"));
    assert!(tools.iter().any(|t| t.name == "another_valid"));
}

#[test]
fn test_nonexistent_directory() {
    let config = RuneDiscoveryConfig {
        tool_directories: vec![PathBuf::from("/nonexistent/path/that/doesnt/exist")],
        extensions: vec!["rn".to_string()],
        recursive: true,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    // Should not error, just return empty
    assert!(tools.is_empty());
}

#[test]
fn test_mixed_format_files() {
    let temp = setup_test_dir();

    // Legacy format file
    fs::write(
        temp.path().join("legacy.rn"),
        r#"//! Legacy tool
//! @param name string Name to greet

pub fn main(name) {
    Ok(format!("Hello, {}!", name))
}
"#,
    )
    .unwrap();

    // Multi-tool format file
    fs::write(
        temp.path().join("multi.rn"),
        r#"
#[tool(desc = "Tool A")]
pub fn tool_a() {}

#[tool(desc = "Tool B")]
pub fn tool_b() {}
"#,
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };
    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    assert_eq!(tools.len(), 3);

    let legacy = tools.iter().find(|t| t.name == "legacy").unwrap();
    assert_eq!(legacy.description, "Legacy tool");

    assert!(tools.iter().any(|t| t.name == "tool_a"));
    assert!(tools.iter().any(|t| t.name == "tool_b"));
}

// =============================================================================
// Registry integration tests
// =============================================================================

#[tokio::test]
async fn test_registry_registers_discovered_tools() {
    let temp = setup_test_dir();

    fs::write(
        temp.path().join("tools.rn"),
        r#"
#[tool(desc = "Tool One")]
#[param(name = "input", type = "string", desc = "Input")]
pub fn tool_one(input) {}

#[tool(desc = "Tool Two")]
pub fn tool_two() {}
"#,
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let registry = RuneToolRegistry::discover_from(config).await.unwrap();
    let tools = registry.list_tools().await;

    // Registry adds rune_ prefix
    assert_eq!(tools.len(), 2);
    // Tool names in the RuneTool struct don't include prefix
    assert!(tools.iter().any(|t| t.name == "tool_one"));
    assert!(tools.iter().any(|t| t.name == "tool_two"));
}

#[tokio::test]
async fn test_registry_tool_lookup() {
    let temp = setup_test_dir();

    fs::write(
        temp.path().join("lookup_test.rn"),
        r#"
#[tool(desc = "Lookup test tool")]
#[param(name = "query", type = "string", desc = "Query")]
pub fn lookup_tool(query) {}
"#,
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let registry = RuneToolRegistry::discover_from(config).await.unwrap();

    // Should find tool with rune_ prefix
    let tool = registry.get_tool("rune_lookup_tool").await;
    assert!(tool.is_some());
    assert_eq!(tool.unwrap().description, "Lookup test tool");

    // Should not find without prefix
    assert!(registry.get_tool("lookup_tool").await.is_none());

    // Should not find non-existent tool
    assert!(registry.get_tool("rune_nonexistent").await.is_none());
}

#[tokio::test]
async fn test_registry_handles_duplicate_names() {
    let dir1 = setup_test_dir();
    let dir2 = setup_test_dir();

    // Same tool name in different directories
    fs::write(
        dir1.path().join("duplicate.rn"),
        "//! First version\npub fn main() {}",
    )
    .unwrap();
    fs::write(
        dir2.path().join("duplicate.rn"),
        "//! Second version\npub fn main() {}",
    )
    .unwrap();

    let config = RuneDiscoveryConfig {
        tool_directories: vec![dir1.path().to_path_buf(), dir2.path().to_path_buf()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let registry = RuneToolRegistry::discover_from(config).await.unwrap();
    let tools = registry.list_tools().await;

    // Registry should have deduplicated - last one wins (kiln overlay pattern)
    assert_eq!(tools.len(), 1);
    // The second directory's version should win
    assert_eq!(tools[0].description, "Second version");
}
