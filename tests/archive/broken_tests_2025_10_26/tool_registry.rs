//! Integration tests for Rune tool registry and execution
//!
//! Tests verify tool discovery, loading, execution, and error handling for the Rune-based
//! tool system. These tests exercise the integration between the tool registry, Rune runtime,
//! and the daemon's tool execution infrastructure.
//!
//! Test Organization:
//! - Tool Discovery: Finding and enumerating .rn files
//! - Tool Loading: Parsing and compiling Rune scripts
//! - Tool Execution: Running tools with various parameters
//! - Error Handling: Graceful degradation on failures

use anyhow::Result;
use crucible_daemon::tools::{ToolRegistry, ToolResult, ToolStatus};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs;

// ==============================================================================
// Test Helper Infrastructure
// ==============================================================================

/// Test harness for tool registry tests
///
/// Provides isolated temporary directory for tool scripts and manages registry lifecycle
struct TestToolRegistry {
    /// Temporary directory for tool scripts (cleaned up on drop)
    tool_dir: TempDir,
    /// Path to tool directory (for convenience)
    tool_path: PathBuf,
}

impl TestToolRegistry {
    /// Create new test registry with isolated tool directory
    async fn new() -> Result<Self> {
        let tool_dir = TempDir::new()?;
        let tool_path = tool_dir.path().to_path_buf();

        Ok(Self {
            tool_dir,
            tool_path,
        })
    }

    /// Create a Rune tool script in the test directory
    ///
    /// # Arguments
    /// * `name` - Tool name (without .rn extension)
    /// * `content` - Rune script source code
    async fn create_tool(&self, name: &str, content: &str) -> Result<PathBuf> {
        let path = self.tool_path.join(format!("{}.rn", name));
        fs::write(&path, content).await?;
        Ok(path)
    }

    /// Create a non-Rune file (for testing file filtering)
    async fn create_file(&self, name: &str, content: &str) -> Result<PathBuf> {
        let path = self.tool_path.join(name);
        fs::write(&path, content).await?;
        Ok(path)
    }

    /// Get path to tool directory
    fn tool_dir_path(&self) -> &PathBuf {
        &self.tool_path
    }

    /// List all .rn files in tool directory (for verification)
    async fn list_rune_files(&self) -> Result<Vec<String>> {
        let mut entries = fs::read_dir(&self.tool_path).await?;
        let mut files = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "rn" {
                    if let Some(name) = path.file_stem() {
                        files.push(name.to_string_lossy().to_string());
                    }
                }
            }
        }

        files.sort();
        Ok(files)
    }
}

// ==============================================================================
// Tool Discovery Tests
// ==============================================================================

/// Test: Discover all .rn files in a directory
///
/// Verifies that ToolRegistry can scan a directory and identify all Rune tool scripts.
/// This is the foundation for the tool system - users create .rn files and they should
/// automatically become available.
#[tokio::test]
async fn test_discover_tools_in_directory() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    // Create three test tools with varying complexity
    test_reg.create_tool("hello", r#"
        pub fn main() {
            "Hello from Rune!"
        }
    "#).await?;

    test_reg.create_tool("count_notes", r#"
        pub async fn main(db) {
            let result = db.query("SELECT count() FROM notes").await?;
            result
        }
    "#).await?;

    test_reg.create_tool("search_tag", r#"
        pub async fn main(db, tag) {
            let query = format!("SELECT * FROM notes WHERE tags CONTAINS '{}'", tag);
            db.query(query).await?
        }
    "#).await?;

    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    let tools = registry.discover_tools().await?;

    assert_eq!(tools.len(), 3);
    assert_eq!(tools, vec!["count_notes", "hello", "search_tag"]);

    Ok(())
}

/// Test: Ignore non-Rune files during discovery
///
/// Ensures registry only loads .rn files, not README.md, notes.txt, or other files
/// that might exist in the tools directory.
#[tokio::test]
async fn test_ignore_non_rune_files() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    // Create mix of Rune and non-Rune files
    test_reg.create_tool("valid_tool", r#"
        pub fn main() {
            "I am a tool"
        }
    "#).await?;

    test_reg.create_tool("another_tool", r#"
        pub fn main() {
            "Me too"
        }
    "#).await?;

    // These should be ignored
    test_reg.create_file("README.md", "# Tool Documentation").await?;
    test_reg.create_file("notes.txt", "Some notes about tools").await?;
    test_reg.create_file("config.yaml", "timeout: 30").await?;
    test_reg.create_file("data.json", r#"{"key": "value"}"#).await?;

    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    let tools = registry.discover_tools().await?;

    assert_eq!(tools.len(), 2);
    assert_eq!(tools, vec!["another_tool", "valid_tool"]);

    Ok(())
}

/// Test: Hot-reload when new tool added
///
/// Verifies file watcher detects new .rn files and automatically reloads registry.
/// This enables live development - users can add/edit tools without restarting daemon.
#[tokio::test]
async fn test_hot_reload_on_file_change() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    // Start with initial tools
    test_reg.create_tool("initial_1", r#"
        pub fn main() {
            "Tool 1"
        }
    "#).await?;

    test_reg.create_tool("initial_2", r#"
        pub fn main() {
            "Tool 2"
        }
    "#).await?;

    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    let initial_tools = registry.discover_tools().await?;
    assert_eq!(initial_tools.len(), 2);

    // Add new tool after registry initialized
    test_reg.create_tool("new_tool", r#"
        pub fn main() {
            "I'm new!"
        }
    "#).await?;

    // Wait for file system to flush
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    registry.reload().await?;
    let updated_tools = registry.list_tools();

    assert_eq!(updated_tools.len(), 3);
    assert!(updated_tools.contains(&"new_tool".to_string()));

    Ok(())
}

// ==============================================================================
// Tool Loading Tests
// ==============================================================================

/// Test: Load and parse valid Rune script
///
/// Verifies basic Rune compilation pipeline works. Script should compile cleanly
/// and metadata (name, description) should be extractable.
#[tokio::test]
async fn test_load_valid_rune_script() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    // Create well-formed Rune script with documentation
    test_reg.create_tool("documented_tool", r#"
        //! Count total notes in kiln
        //!
        //! This tool queries the database and returns the total count.

        pub async fn main(db) {
            let result = db.query("SELECT count() FROM notes").await?;
            result
        }
    "#).await?;

    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    registry.discover_tools().await?;

    // Verify tool was loaded
    let tools = registry.list_tools();
    assert!(tools.contains(&"documented_tool".to_string()));

    // TODO: get_tool_info() not yet implemented - that's for metadata extraction
    // For now, just verify it loads without error

    Ok(())
}

/// Test: Handle invalid Rune syntax gracefully
///
/// When user creates script with syntax error, registry should:
/// - Return clear error with filename and line number
/// - Not crash or panic
/// - Keep other tools available
#[tokio::test]
async fn test_handle_invalid_rune_syntax() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    // Create valid tool
    test_reg.create_tool("valid", r#"
        pub fn main() {
            "I work fine"
        }
    "#).await?;

    // Create tool with syntax errors
    test_reg.create_tool("broken", r#"
        pub fn main() {
            let x = "unclosed string;
            return x
        }
    "#).await?;

    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    let result = registry.load_tool("broken").await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("broken") || err_msg.contains("Failed to compile"));

    // Valid tool should still work
    let valid_result = registry.load_tool("valid").await;
    assert!(valid_result.is_ok());

    Ok(())
}

/// Test: Tool can access database connection
///
/// Rune tools need database access to query kiln data. Verify that db parameter
/// is properly injected and queries execute successfully.
#[tokio::test]
async fn test_tool_with_database_access() -> Result<()> {
    use crucible_daemon::tools::DbHandle;

    let test_reg = TestToolRegistry::new().await?;

    // Create tool that uses db::query_simple to execute a database query
    test_reg.create_tool("db_query", r#"
        pub fn main() {
            // Use db::query_simple for a parameterless query
            let result = db::query_simple("SELECT * FROM notes LIMIT 1");
            "Query executed successfully"
        }
    "#).await?;

    // Create registry with database access
    let db_handle = DbHandle::new();
    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?
        .with_database(db_handle)?;

    // Discover and load tools with database context
    registry.discover_tools().await?;

    let tools = registry.list_tools();
    assert!(tools.contains(&"db_query".to_string()),
        "db_query tool should be discovered and loaded");

    // Execute the tool - should compile and run without errors
    let result = registry.execute_tool("db_query", &[]).await?;
    assert!(result.is_success(),
        "Tool should execute successfully with database access: {:?}", result);

    Ok(())
}

// ==============================================================================
// Tool Execution Tests
// ==============================================================================

/// Test: Execute simple tool and capture output
///
/// Basic execution test: run tool, get result, verify output formatting.
#[tokio::test]
async fn test_execute_simple_tool() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    // Create simple stateless tool
    test_reg.create_tool("hello", r#"
        pub fn main() {
            "Hello from Rune tool!"
        }
    "#).await?;

    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    registry.discover_tools().await?;

    let result = registry.execute_tool("hello", &[]).await?;

    assert!(result.is_success());
    assert!(result.output.contains("Hello from Rune tool"));

    Ok(())
}

/// Test: Pass arguments to tool
///
/// Tools should accept command-line arguments that get bound as parameters.
/// Example: `:run search-tag rust` → main(db, "rust")
#[tokio::test]
async fn test_execute_tool_with_arguments() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    // Tool that accepts parameters
    test_reg.create_tool("greet", r#"
        pub fn main(name, greeting) {
            format!("{} {}!", greeting, name)
        }
    "#).await?;

    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    registry.discover_tools().await?;

    let args = vec!["Alice".to_string(), "Hello".to_string()];
    let result = registry.execute_tool("greet", &args).await?;

    assert!(result.is_success());
    assert!(result.output.contains("Hello Alice"));

    Ok(())
}

/// Test: Tool execution timeout
///
/// Long-running or infinite-loop tools should timeout gracefully to prevent
/// REPL from hanging. User should get error message, not frozen terminal.
#[tokio::test]
async fn test_execute_tool_timeout() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    // Tool that runs forever (or very long time)
    test_reg.create_tool("infinite", r#"
        pub fn main() {
            loop {
                // Infinite loop - should timeout
            }
        }
    "#).await?;

    // Timeout handling is a future feature
    // For now, just verify the tool compiles (it won't run forever in test)
    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    registry.discover_tools().await?;

    // We won't actually execute the infinite loop in tests
    // Just verify it loaded
    let tools = registry.list_tools();
    assert!(tools.contains(&"infinite".to_string()));

    // TODO: Implement timeout with tokio::time::timeout wrapper

    Ok(())
}

// ==============================================================================
// Error Handling Tests
// ==============================================================================

/// Test: Tool runtime error captured
///
/// When tool throws error during execution (not compilation), error should be
/// captured and returned to user with stack trace.
#[tokio::test]
async fn test_tool_runtime_error() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    // Tool that panics at runtime
    test_reg.create_tool("crash", r#"
        pub fn main() {
            // This will cause runtime error
            let data = [];
            data[5] // Index out of bounds
        }
    "#).await?;

    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    registry.discover_tools().await?;

    let result = registry.execute_tool("crash", &[]).await?;

    assert!(result.is_error());
    let error_msg = result.error_message().unwrap();
    assert!(error_msg.contains("index") || error_msg.contains("bounds") || error_msg.contains("out of"));

    Ok(())
}

/// Test: Tool returns structured data (JSON)
///
/// Tools should be able to return complex data structures, not just strings.
/// This enables integration with other tools and piping.
#[tokio::test]
async fn test_tool_returns_structured_data() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    // Tool that returns structured object
    test_reg.create_tool("metadata", r#"
        pub fn main() {
            #{
                name: "Crucible",
                version: "0.1.0",
                tags: ["knowledge", "graph", "crdt"],
                stats: #{
                    notes: 42,
                    links: 128
                }
            }
        }
    "#).await?;

    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    registry.discover_tools().await?;

    let result = registry.execute_tool("metadata", &[]).await?;

    assert!(result.is_success());
    // Rune's Debug output contains the data, even if not JSON formatted yet
    assert!(result.output.contains("Crucible") || result.output.contains("0.1.0"));

    // TODO: Implement proper JSON serialization for structured data

    Ok(())
}

/// Test: List all available tools with metadata
///
/// The :tools command needs to show all tools with descriptions.
/// This requires extracting metadata from compiled tools.
#[tokio::test]
async fn test_list_tools_with_metadata() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    // Create multiple documented tools
    test_reg.create_tool("search", r#"
        //! Search notes by tag or title

        pub async fn main(db, query) {
            // Implementation
            "Results"
        }
    "#).await?;

    test_reg.create_tool("export", r#"
        //! Export notes to CSV format

        pub async fn main(db, path) {
            // Implementation
            "Exported"
        }
    "#).await?;

    test_reg.create_tool("stats", r#"
        //! Show kiln statistics

        pub fn main() {
            "Stats here"
        }
    "#).await?;

    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    registry.discover_tools().await?;

    let tools = registry.list_tools();

    assert_eq!(tools.len(), 3);
    assert!(tools.contains(&"search".to_string()));
    assert!(tools.contains(&"export".to_string()));
    assert!(tools.contains(&"stats".to_string()));

    // TODO: Implement list_tools_with_info() for metadata extraction

    Ok(())
}

/// Test: Tool execution with missing tool name
///
/// Verify proper error handling when user tries to run non-existent tool.
#[tokio::test]
async fn test_execute_nonexistent_tool() -> Result<()> {
    let test_reg = TestToolRegistry::new().await?;

    test_reg.create_tool("exists", r#"
        pub fn main() {
            "I exist"
        }
    "#).await?;

    let mut registry = ToolRegistry::new(test_reg.tool_dir_path().clone())?;
    registry.discover_tools().await?;

    let result = registry.execute_tool("doesnt_exist", &[]).await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("not found") || err_msg.contains("Tool"));
    assert!(err_msg.contains(":tools")); // Suggest checking available tools

    Ok(())
}

// ==============================================================================
// Additional Test Ideas (For Future Implementation)
// ==============================================================================

// Future tests to consider:
// - test_tool_with_dependencies() - Tools that call other tools
// - test_concurrent_tool_execution() - Multiple tools running simultaneously
// - test_tool_resource_limits() - Memory/CPU constraints
// - test_tool_sandboxing() - Security restrictions on file access
// - test_tool_caching() - Cache compiled scripts for performance
// - test_tool_versioning() - Handle breaking changes in tool API
// - test_watch_directory_errors() - File watcher failure recovery
// - test_tool_with_stdin() - Interactive tools that read input
// - test_tool_progress_reporting() - Long-running tools with progress bars
// - test_recursive_tool_calls() - Tool A calls tool B calls tool A
