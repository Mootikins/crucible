//! Comprehensive tests for Rune command functionality
//!
//! This module tests the Rune command functionality including:
//! - Basic script execution via ToolRegistry
//! - Script discovery and loading
//! - Argument passing and validation
//! - Error handling (compilation errors, runtime errors)
//! - List commands functionality
//! - Script path resolution
//!
//! ## Architecture Notes
//!
//! This test suite has been adapted from the Phase 1 archived tests to work with
//! the Phase 2 simplified architecture:
//!
//! - **No test_utilities crate**: Uses inline test helpers
//! - **No service bridge**: Tests direct ToolRegistry execution
//! - **No migration config**: Service architecture has been removed
//! - **Direct execution**: Uses ToolRegistry directly instead of CLI commands
//!
//! ## What's Tested
//!
//! - ✅ Basic script execution with main() function
//! - ✅ Argument passing (0, 1, 2+ args)
//! - ✅ Script discovery from directories
//! - ✅ Compilation error handling
//! - ✅ Runtime error handling
//! - ✅ List tools functionality
//! - ✅ Script path resolution
//!
//! ## What's Skipped
//!
//! - ❌ Service bridge integration (removed in Phase 1.1)
//! - ❌ Migration config testing (obsolete)
//! - ❌ Fallback behavior (no longer applicable)
//! - ❌ Performance benchmarks (dedicated test suite exists)
//! - ❌ Memory usage tests (not reliable in unit tests)
//! - ❌ Timeout tests (should be handled by integration tests)
//! - ❌ Concurrent execution (REPL handles this)

use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;

use crucible_cli::commands::repl::tools::ToolRegistry;
use crucible_cli::config::CliConfig;

// ============================================================================
// Test Utilities
// ============================================================================

/// Test context for Rune script tests
///
/// Provides a temporary directory structure for testing script discovery
/// and execution without polluting the file system.
struct RuneTestContext {
    #[allow(dead_code)]
    temp_dir: TempDir,
    tool_dir: PathBuf,
}

impl RuneTestContext {
    /// Create new test context with tool directory
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let tool_dir = temp_dir.path().join("tools");
        std::fs::create_dir_all(&tool_dir)?;

        Ok(Self { temp_dir, tool_dir })
    }

    /// Create a test script with given name and content
    ///
    /// Returns the path to the created script file.
    fn create_script(&self, name: &str, content: &str) -> Result<PathBuf> {
        let script_path = self.tool_dir.join(format!("{}.rn", name));
        std::fs::write(&script_path, content)?;
        Ok(script_path)
    }

    /// Create a new ToolRegistry for the test tool directory
    async fn create_registry(&self) -> Result<ToolRegistry> {
        ToolRegistry::new(self.tool_dir.clone())
    }
}

// ============================================================================
// Basic Execution Tests
// ============================================================================

#[tokio::test]
async fn test_rune_execute_basic_script() -> Result<()> {
    let context = RuneTestContext::new()?;

    // Create a simple Rune script
    let script_content = r#"
pub fn main() {
    "Hello from Rune"
}
"#;

    context.create_script("hello", script_content)?;

    // Create registry and load tool
    let mut registry = context.create_registry().await?;
    registry.load_tool("hello").await?;

    // Execute the tool
    let result = registry.execute_tool("hello", &[]).await?;

    // Should succeed
    assert!(result.is_success(), "Script execution should succeed");
    assert!(
        result.output.contains("Hello"),
        "Output should contain greeting"
    );

    Ok(())
}

#[tokio::test]
async fn test_rune_execute_with_single_arg() -> Result<()> {
    let context = RuneTestContext::new()?;

    // Script that echoes its argument
    let script_content = r#"
pub fn main(name) {
    `Hello, ${name}!`
}
"#;

    context.create_script("greet", script_content)?;

    let mut registry = context.create_registry().await?;
    registry.load_tool("greet").await?;

    // Execute with argument
    let result = registry
        .execute_tool("greet", &["World".to_string()])
        .await?;

    assert!(result.is_success(), "Script with args should succeed");
    assert!(
        result.output.contains("World"),
        "Output should contain passed argument"
    );

    Ok(())
}

#[tokio::test]
async fn test_rune_execute_with_multiple_args() -> Result<()> {
    let context = RuneTestContext::new()?;

    // Script that uses multiple arguments
    let script_content = r#"
pub fn main(first, second) {
    `${first} and ${second}`
}
"#;

    context.create_script("combine", script_content)?;

    let mut registry = context.create_registry().await?;
    registry.load_tool("combine").await?;

    // Execute with two arguments
    let result = registry
        .execute_tool("combine", &["foo".to_string(), "bar".to_string()])
        .await?;

    assert!(result.is_success(), "Script with multiple args should succeed");
    assert!(result.output.contains("foo"), "Should contain first arg");
    assert!(result.output.contains("bar"), "Should contain second arg");

    Ok(())
}

#[tokio::test]
async fn test_rune_execute_returns_value() -> Result<()> {
    let context = RuneTestContext::new()?;

    // Script that returns a structured value
    let script_content = r#"
pub fn main() {
    42
}
"#;

    context.create_script("return_value", script_content)?;

    let mut registry = context.create_registry().await?;
    registry.load_tool("return_value").await?;

    let result = registry.execute_tool("return_value", &[]).await?;

    assert!(result.is_success(), "Script should succeed");
    assert!(
        result.output.contains("42"),
        "Output should contain return value"
    );

    Ok(())
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_rune_compile_error_syntax() -> Result<()> {
    let context = RuneTestContext::new()?;

    // Script with syntax error (missing closing brace)
    let invalid_script = r#"
pub fn main() {
    "unclosed string
}
"#;

    context.create_script("syntax_error", invalid_script)?;

    let mut registry = context.create_registry().await?;

    // Should fail to compile
    let result = registry.load_tool("syntax_error").await;
    assert!(result.is_err(), "Should fail to compile invalid syntax");

    Ok(())
}

#[tokio::test]
async fn test_rune_runtime_error() -> Result<()> {
    let context = RuneTestContext::new()?;

    // Script that will cause a runtime error (division by zero)
    let error_script = r#"
pub fn main() {
    1 / 0
}
"#;

    context.create_script("runtime_error", error_script)?;

    let mut registry = context.create_registry().await?;
    registry.load_tool("runtime_error").await?;

    // Execute - should handle error gracefully
    let result = registry.execute_tool("runtime_error", &[]).await?;

    // ToolResult should indicate error
    assert!(result.is_error(), "Should indicate execution failure");
    assert!(
        result.error_message().is_some(),
        "Should provide error message"
    );

    Ok(())
}

#[tokio::test]
async fn test_rune_execute_nonexistent_tool() -> Result<()> {
    let context = RuneTestContext::new()?;

    let registry = context.create_registry().await?;

    // Try to execute a tool that doesn't exist
    let result = registry.execute_tool("nonexistent", &[]).await;

    assert!(
        result.is_err(),
        "Should fail when executing nonexistent tool"
    );

    Ok(())
}

#[tokio::test]
async fn test_rune_compile_error_missing_main() -> Result<()> {
    let context = RuneTestContext::new()?;

    // Script without a main function
    let no_main_script = r#"
pub fn helper() {
    "I'm a helper"
}
"#;

    context.create_script("no_main", no_main_script)?;

    let mut registry = context.create_registry().await?;

    // Should compile successfully (syntax is valid)
    let load_result = registry.load_tool("no_main").await;
    assert!(load_result.is_ok(), "Should compile valid syntax");

    // But execution should fail (no main function)
    let exec_result = registry.execute_tool("no_main", &[]).await;

    // This will fail because there's no main() function to call
    assert!(
        exec_result.is_err() || exec_result.unwrap().is_error(),
        "Should fail when main() function is missing"
    );

    Ok(())
}

// ============================================================================
// Script Discovery Tests
// ============================================================================

#[tokio::test]
async fn test_rune_discover_empty_directory() -> Result<()> {
    let context = RuneTestContext::new()?;

    let mut registry = context.create_registry().await?;
    let tools = registry.discover_tools().await?;

    // Should return empty list for empty directory
    assert!(
        tools.is_empty(),
        "Empty directory should have no tools"
    );

    Ok(())
}

#[tokio::test]
async fn test_rune_discover_multiple_scripts() -> Result<()> {
    let context = RuneTestContext::new()?;

    // Create multiple test scripts
    let scripts = vec![
        ("alpha", "pub fn main() { \"alpha\" }"),
        ("beta", "pub fn main() { \"beta\" }"),
        ("gamma", "pub fn main() { \"gamma\" }"),
    ];

    for (name, content) in &scripts {
        context.create_script(name, content)?;
    }

    let mut registry = context.create_registry().await?;
    let tools = registry.discover_tools().await?;

    // Should discover all scripts
    assert_eq!(tools.len(), 3, "Should discover all 3 scripts");

    // Should be sorted alphabetically
    assert_eq!(tools[0], "alpha");
    assert_eq!(tools[1], "beta");
    assert_eq!(tools[2], "gamma");

    Ok(())
}

#[tokio::test]
async fn test_rune_discover_ignores_non_rune_files() -> Result<()> {
    let context = RuneTestContext::new()?;

    // Create both .rn and non-.rn files
    context.create_script("valid", "pub fn main() { \"valid\" }")?;
    std::fs::write(context.tool_dir.join("readme.txt"), "Not a Rune script")?;
    std::fs::write(context.tool_dir.join("config.json"), "{}")?;

    let mut registry = context.create_registry().await?;
    let tools = registry.discover_tools().await?;

    // Should only discover .rn files
    assert_eq!(tools.len(), 1, "Should only discover .rn files");
    assert_eq!(tools[0], "valid");

    Ok(())
}

#[tokio::test]
async fn test_rune_list_loaded_tools() -> Result<()> {
    let context = RuneTestContext::new()?;

    context.create_script("tool1", "pub fn main() { 1 }")?;
    context.create_script("tool2", "pub fn main() { 2 }")?;

    let mut registry = context.create_registry().await?;

    // List should be empty before loading
    assert!(
        registry.list_tools().is_empty(),
        "No tools loaded initially"
    );

    // Load one tool
    registry.load_tool("tool1").await?;
    let tools = registry.list_tools();
    assert_eq!(tools.len(), 1, "Should have 1 loaded tool");
    assert_eq!(tools[0], "tool1");

    // Load second tool
    registry.load_tool("tool2").await?;
    let tools = registry.list_tools();
    assert_eq!(tools.len(), 2, "Should have 2 loaded tools");

    Ok(())
}

// ============================================================================
// CLI Command Tests
// ============================================================================

#[tokio::test]
async fn test_rune_list_commands() -> Result<()> {
    use crucible_cli::commands::rune;

    let config = CliConfig::default();

    // Should succeed and display available commands
    let result = rune::list_commands(config).await;

    assert!(result.is_ok(), "List commands should succeed");

    Ok(())
}

#[tokio::test]
async fn test_rune_execute_via_cli_command() -> Result<()> {
    use crucible_cli::commands::rune;

    let context = RuneTestContext::new()?;
    let config = CliConfig::default();

    // Create a simple script
    let script_content = r#"
pub fn main() {
    "CLI execution test"
}
"#;

    let script_path = context.create_script("cli_test", script_content)?;

    // Execute via CLI command (simplified execution)
    let result = rune::execute(
        config,
        script_path.to_string_lossy().to_string(),
        None,
    )
    .await;

    // In Phase 2, execute() does basic validation, not full execution
    assert!(result.is_ok(), "CLI execute should succeed");

    Ok(())
}

#[tokio::test]
async fn test_rune_execute_script_not_found() -> Result<()> {
    use crucible_cli::commands::rune;

    let config = CliConfig::default();

    // Try to execute nonexistent script
    let result = rune::execute(config, "nonexistent.rn".to_string(), None).await;

    assert!(result.is_err(), "Should fail with nonexistent script");

    Ok(())
}

// ============================================================================
// Advanced Script Tests
// ============================================================================

#[tokio::test]
async fn test_rune_script_with_string_interpolation() -> Result<()> {
    let context = RuneTestContext::new()?;

    let script_content = r#"
pub fn main(name, age) {
    `${name} is ${age} years old`
}
"#;

    context.create_script("interpolate", script_content)?;

    let mut registry = context.create_registry().await?;
    registry.load_tool("interpolate").await?;

    let result = registry
        .execute_tool("interpolate", &["Alice".to_string(), "30".to_string()])
        .await?;

    assert!(result.is_success(), "String interpolation should work");
    assert!(result.output.contains("Alice"));
    assert!(result.output.contains("30"));

    Ok(())
}

#[tokio::test]
async fn test_rune_script_with_array() -> Result<()> {
    let context = RuneTestContext::new()?;

    let script_content = r#"
pub fn main() {
    let items = ["apple", "banana", "cherry"];
    items
}
"#;

    context.create_script("array_test", script_content)?;

    let mut registry = context.create_registry().await?;
    registry.load_tool("array_test").await?;

    let result = registry.execute_tool("array_test", &[]).await?;

    assert!(result.is_success(), "Array handling should work");
    // Output should contain array representation
    assert!(
        result.output.contains("apple") || result.output.contains("["),
        "Should show array content"
    );

    Ok(())
}

#[tokio::test]
async fn test_rune_script_with_control_flow() -> Result<()> {
    let context = RuneTestContext::new()?;

    let script_content = r#"
pub fn main(value) {
    let num = value.parse::<i64>()?;
    if num > 0 {
        "positive"
    } else if num < 0 {
        "negative"
    } else {
        "zero"
    }
}
"#;

    context.create_script("classify", script_content)?;

    let mut registry = context.create_registry().await?;
    registry.load_tool("classify").await?;

    // Test positive
    let result = registry
        .execute_tool("classify", &["5".to_string()])
        .await?;
    assert!(result.is_success());
    assert!(result.output.contains("positive"));

    // Test negative
    let result = registry
        .execute_tool("classify", &["-3".to_string()])
        .await?;
    assert!(result.is_success());
    assert!(result.output.contains("negative"));

    // Test zero
    let result = registry
        .execute_tool("classify", &["0".to_string()])
        .await?;
    assert!(result.is_success());
    assert!(result.output.contains("zero"));

    Ok(())
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_rune_empty_script() -> Result<()> {
    let context = RuneTestContext::new()?;

    context.create_script("empty", "")?;

    let mut registry = context.create_registry().await?;

    // Empty script might compile (valid Rust), but execution will fail
    // since there's no main() function
    let load_result = registry.load_tool("empty").await;

    // Either loading fails or execution fails
    if load_result.is_ok() {
        let exec_result = registry.execute_tool("empty", &[]).await;
        assert!(
            exec_result.is_err() || exec_result.unwrap().is_error(),
            "Empty script should fail to execute"
        );
    } else {
        // Loading failed, which is also acceptable
        assert!(load_result.is_err(), "Empty script failed to load");
    }

    Ok(())
}

#[tokio::test]
async fn test_rune_script_with_comments() -> Result<()> {
    let context = RuneTestContext::new()?;

    let script_content = r#"
// This is a comment
/* Multi-line
   comment */
pub fn main() {
    // Inline comment
    "comments work"  // End of line comment
}
"#;

    context.create_script("comments", script_content)?;

    let mut registry = context.create_registry().await?;
    registry.load_tool("comments").await?;

    let result = registry.execute_tool("comments", &[]).await?;

    assert!(result.is_success(), "Comments should be handled correctly");

    Ok(())
}

#[tokio::test]
async fn test_rune_script_with_unicode() -> Result<()> {
    let context = RuneTestContext::new()?;

    let script_content = r#"
pub fn main() {
    "Hello 世界 🌍"
}
"#;

    context.create_script("unicode", script_content)?;

    let mut registry = context.create_registry().await?;
    registry.load_tool("unicode").await?;

    let result = registry.execute_tool("unicode", &[]).await?;

    assert!(result.is_success(), "Unicode should be handled correctly");
    assert!(
        result.output.contains("世界") || result.output.contains("🌍"),
        "Should preserve Unicode characters"
    );

    Ok(())
}

#[tokio::test]
async fn test_rune_reload_tools() -> Result<()> {
    let context = RuneTestContext::new()?;

    context.create_script("tool1", "pub fn main() { 1 }")?;

    let mut registry = context.create_registry().await?;
    registry.load_tool("tool1").await?;

    assert_eq!(registry.list_tools().len(), 1);

    // Add another tool
    context.create_script("tool2", "pub fn main() { 2 }")?;

    // Reload should discover new tool
    registry.reload().await?;

    assert_eq!(
        registry.list_tools().len(),
        2,
        "Reload should discover new tools"
    );

    Ok(())
}
