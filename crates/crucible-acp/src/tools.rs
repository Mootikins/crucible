//! Tool discovery and registration for ACP integration
//!
//! This module provides functionality to discover and register Crucible tools
//! with ACP agents via the Model Context Protocol (MCP).
//!
//! ## Responsibilities
//!
//! - Discover available tools from crucible-tools
//! - Register tools with agent sessions
//! - Provide tool catalog for agent capabilities
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on tool registration and discovery
//! - **Dependency Inversion**: Uses traits for extensibility
//! - **Open/Closed**: New tool types can be added without modification

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::{AcpError, Result};

/// Descriptor for a registered tool
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDescriptor {
    /// Unique identifier for the tool
    pub name: String,

    /// Human-readable description of what the tool does
    pub description: String,

    /// Category of the tool (e.g., "notes", "search", "kiln")
    pub category: String,

    /// JSON schema for the tool's input parameters
    pub input_schema: serde_json::Value,
}

/// Registry for managing available tools
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    /// Map of tool name to descriptor
    tools: HashMap<String, ToolDescriptor>,
}

impl ToolRegistry {
    /// Create a new empty tool registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool with the registry
    ///
    /// # Arguments
    ///
    /// * `descriptor` - The tool descriptor to register
    ///
    /// # Errors
    ///
    /// Returns an error if a tool with the same name is already registered
    pub fn register(&mut self, descriptor: ToolDescriptor) -> Result<()> {
        if self.tools.contains_key(&descriptor.name) {
            return Err(AcpError::InvalidConfig(format!(
                "Tool already registered: {}",
                descriptor.name
            )));
        }
        self.tools.insert(descriptor.name.clone(), descriptor);
        Ok(())
    }

    /// Get a tool descriptor by name
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the tool to retrieve
    ///
    /// # Returns
    ///
    /// The tool descriptor if found, None otherwise
    pub fn get(&self, name: &str) -> Option<&ToolDescriptor> {
        self.tools.get(name)
    }

    /// List all registered tools
    ///
    /// # Returns
    ///
    /// A vector of all tool descriptors
    pub fn list(&self) -> Vec<&ToolDescriptor> {
        self.tools.values().collect()
    }

    /// Get the number of registered tools
    pub fn count(&self) -> usize {
        self.tools.len()
    }

    /// Check if a tool is registered
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to create a tool descriptor with less boilerplate
///
/// # Arguments
///
/// * `name` - Tool name
/// * `description` - Tool description
/// * `category` - Tool category
/// * `schema` - JSON schema for input parameters
fn create_tool(
    name: impl Into<String>,
    description: impl Into<String>,
    category: impl Into<String>,
    schema: serde_json::Value,
) -> ToolDescriptor {
    ToolDescriptor {
        name: name.into(),
        description: description.into(),
        category: category.into(),
        input_schema: schema,
    }
}

/// Discover and register all Crucible tools
///
/// This function scans the crucible-tools crate and registers all available
/// tools with the provided registry.
///
/// # Arguments
///
/// * `registry` - The registry to populate with discovered tools
/// * `kiln_path` - The path to the kiln for tool initialization
///
/// # Returns
///
/// The number of tools discovered and registered
///
/// # Errors
///
/// Returns an error if tool discovery or registration fails
pub fn discover_crucible_tools(registry: &mut ToolRegistry, _kiln_path: &str) -> Result<usize> {
    // TDD Cycle 9 - GREEN: Implement tool discovery from crucible-tools
    //
    // Crucible tools are organized into 3 categories:
    // - NoteTools (6 tools): CRUD operations for notes
    // - SearchTools (3 tools): Text, property, and semantic search
    // - KilnTools (1 tool): Kiln metadata retrieval
    //
    // Since the tools use compile-time macros (#[tool]), we manually enumerate them here.

    let mut count = 0;

    // Register NoteTools (6 tools)
    let note_tools = vec![
        create_tool("create_note", "Create a new note in the kiln", "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the note file relative to the kiln"},
                    "content": {"type": "string", "description": "Content of the note"},
                    "frontmatter": {"type": "object", "description": "Optional YAML frontmatter metadata"}
                },
                "required": ["path", "content"]
            })),
        create_tool("read_note", "Read the contents of a note", "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the note file"},
                    "start_line": {"type": "integer", "description": "Optional starting line number"},
                    "end_line": {"type": "integer", "description": "Optional ending line number"}
                },
                "required": ["path"]
            })),
        create_tool("read_metadata", "Read only the frontmatter metadata of a note", "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the note file"}
                },
                "required": ["path"]
            })),
        create_tool("update_note", "Update an existing note's content or metadata", "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the note file"},
                    "content": {"type": "string", "description": "New content for the note"},
                    "frontmatter": {"type": "object", "description": "Updated frontmatter metadata"}
                },
                "required": ["path"]
            })),
        create_tool("delete_note", "Delete a note from the kiln", "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the note file to delete"}
                },
                "required": ["path"]
            })),
        create_tool("list_notes", "List all notes in the kiln or a specific folder", "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "folder": {"type": "string", "description": "Optional folder path to list notes from"},
                    "include_frontmatter": {"type": "boolean", "description": "Whether to include frontmatter in results"},
                    "recursive": {"type": "boolean", "description": "Whether to recursively list notes in subfolders"}
                }
            })),
    ];

    for tool in note_tools {
        registry.register(tool)?;
        count += 1;
    }

    // Register SearchTools (3 tools)
    let search_tools = vec![
        create_tool("text_search", "Search for notes by text content", "search",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Text query to search for"},
                    "limit": {"type": "integer", "description": "Maximum number of results to return"}
                },
                "required": ["query"]
            })),
        create_tool("property_search", "Search for notes by frontmatter properties", "search",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "property": {"type": "string", "description": "Property name to search"},
                    "value": {"type": "string", "description": "Value to match"}
                },
                "required": ["property", "value"]
            })),
        create_tool("semantic_search", "Search for notes by semantic similarity using embeddings", "search",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Query text to find semantically similar notes"},
                    "limit": {"type": "integer", "description": "Maximum number of results to return"}
                },
                "required": ["query"]
            })),
    ];

    for tool in search_tools {
        registry.register(tool)?;
        count += 1;
    }

    // Register KilnTools (1 tool)
    registry.register(create_tool(
        "get_kiln_info",
        "Get metadata and information about the kiln",
        "kiln",
        serde_json::json!({"type": "object", "properties": {}})
    ))?;
    count += 1;

    Ok(count)
}

/// Executes tool calls by routing to the appropriate crucible-tools implementation
#[derive(Debug)]
pub struct ToolExecutor {
    kiln_path: PathBuf,
}

impl ToolExecutor {
    /// Create a new tool executor
    ///
    /// # Arguments
    ///
    /// * `kiln_path` - Path to the kiln for tool operations
    pub fn new(kiln_path: PathBuf) -> Self {
        Self { kiln_path }
    }

    /// Execute a tool by name with the given parameters
    ///
    /// # Arguments
    ///
    /// * `tool_name` - Name of the tool to execute
    /// * `params` - JSON parameters for the tool
    ///
    /// # Returns
    ///
    /// The tool execution result as JSON
    ///
    /// # Errors
    ///
    /// Returns an error if the tool doesn't exist or execution fails
    pub async fn execute(&self, tool_name: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        // TDD Cycle 10 - GREEN: Implement tool routing to crucible-tools
        //
        // Route tool calls to the appropriate tool implementation based on category
        match tool_name {
            // NoteTools (6 tools)
            "create_note" => self.execute_note_tool(tool_name, params).await,
            "read_note" => self.execute_note_tool(tool_name, params).await,
            "read_metadata" => self.execute_note_tool(tool_name, params).await,
            "update_note" => self.execute_note_tool(tool_name, params).await,
            "delete_note" => self.execute_note_tool(tool_name, params).await,
            "list_notes" => self.execute_note_tool(tool_name, params).await,

            // SearchTools (3 tools)
            "text_search" => self.execute_search_tool(tool_name, params).await,
            "property_search" => self.execute_search_tool(tool_name, params).await,
            "semantic_search" => self.execute_search_tool(tool_name, params).await,

            // KilnTools (1 tool)
            "get_kiln_info" => self.execute_kiln_tool(tool_name, params).await,

            // Unknown tool
            _ => Err(AcpError::NotFound(format!("Unknown tool: {}", tool_name))),
        }
    }

    /// Helper to extract a required string parameter
    fn get_required_param(params: &serde_json::Value, name: &str) -> Result<String> {
        params.get(name)
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| AcpError::InvalidConfig(format!("Missing required parameter: {}", name)))
    }

    /// Execute a note tool
    async fn execute_note_tool(&self, tool_name: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        // For TDD purposes, we manually implement the tool logic
        // In a full implementation, this would use the rmcp tool router from crucible-tools
        match tool_name {
            "create_note" => {
                // Extract parameters
                let path = Self::get_required_param(&params, "path")?;
                let content = Self::get_required_param(&params, "content")?;

                // Execute the tool by directly writing the file
                let full_path = self.kiln_path.join(&path);
                tokio::fs::write(&full_path, &content).await
                    .map_err(|e| AcpError::FileSystem(format!("Failed to create note: {}", e)))?;

                Ok(serde_json::json!({
                    "path": path,
                    "full_path": full_path.to_string_lossy(),
                    "status": "created"
                }))
            },
            "read_note" => {
                // Extract parameters
                let path = Self::get_required_param(&params, "path")?;

                // Read the file
                let full_path = self.kiln_path.join(&path);
                if !full_path.exists() {
                    return Err(AcpError::NotFound(format!("File not found: {}", path)));
                }

                let content = tokio::fs::read_to_string(&full_path).await
                    .map_err(|e| AcpError::FileSystem(format!("Failed to read note: {}", e)))?;

                Ok(serde_json::json!({
                    "path": path,
                    "content": content,
                    "lines": content.lines().count()
                }))
            },
            _ => Err(AcpError::NotFound(format!("Note tool not implemented: {}", tool_name))),
        }
    }

    /// Execute a search tool
    async fn execute_search_tool(&self, tool_name: &str, _params: serde_json::Value) -> Result<serde_json::Value> {
        // For now, return a placeholder for search tools
        // Full implementation would integrate with crucible-tools search
        Ok(serde_json::json!({
            "tool": tool_name,
            "results": [],
            "count": 0
        }))
    }

    /// Execute a kiln tool
    async fn execute_kiln_tool(&self, _tool_name: &str, _params: serde_json::Value) -> Result<serde_json::Value> {
        // Return basic kiln information
        Ok(serde_json::json!({
            "kiln_path": self.kiln_path.to_string_lossy(),
            "exists": self.kiln_path.exists(),
            "is_directory": self.kiln_path.is_dir()
        }))
    }

    /// Get the kiln path
    pub fn kiln_path(&self) -> &PathBuf {
        &self.kiln_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TDD Cycle 9 - RED: Test expects tool registry to work
    #[test]
    fn test_tool_registry_creation() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.count(), 0);
    }

    // TDD Cycle 9 - RED: Test expects tool registration
    #[test]
    fn test_tool_registration() {
        let mut registry = ToolRegistry::new();

        let descriptor = ToolDescriptor {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            category: "test".to_string(),
            input_schema: serde_json::json!({}),
        };

        let result = registry.register(descriptor.clone());
        assert!(result.is_ok());
        assert_eq!(registry.count(), 1);
        assert!(registry.contains("test_tool"));

        let retrieved = registry.get("test_tool");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &descriptor);
    }

    // TDD Cycle 9 - RED: Test expects duplicate registration to fail
    #[test]
    fn test_duplicate_registration_fails() {
        let mut registry = ToolRegistry::new();

        let descriptor = ToolDescriptor {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            category: "test".to_string(),
            input_schema: serde_json::json!({}),
        };

        registry.register(descriptor.clone()).unwrap();
        let result = registry.register(descriptor);
        assert!(result.is_err());
    }

    // TDD Cycle 9 - RED: Test expects tool listing
    #[test]
    fn test_tool_listing() {
        let mut registry = ToolRegistry::new();

        let descriptor1 = ToolDescriptor {
            name: "tool1".to_string(),
            description: "First tool".to_string(),
            category: "test".to_string(),
            input_schema: serde_json::json!({}),
        };

        let descriptor2 = ToolDescriptor {
            name: "tool2".to_string(),
            description: "Second tool".to_string(),
            category: "test".to_string(),
            input_schema: serde_json::json!({}),
        };

        registry.register(descriptor1).unwrap();
        registry.register(descriptor2).unwrap();

        let tools = registry.list();
        assert_eq!(tools.len(), 2);
    }

    // TDD Cycle 9 - RED: Test expects crucible tools to be discovered
    #[test]
    fn test_discover_crucible_tools() {
        let mut registry = ToolRegistry::new();
        let result = discover_crucible_tools(&mut registry, "/test/kiln");

        // This should fail because discovery is not yet implemented
        // Once implemented, it should discover 10 tools from crucible-tools:
        // - 6 NoteTools: create_note, read_note, read_metadata, update_note, delete_note, list_notes
        // - 3 SearchTools: text_search, property_search, semantic_search
        // - 1 KilnTools: get_kiln_info
        assert!(result.is_ok());
        let count = result.unwrap();
        assert_eq!(count, 10, "Should discover 10 Crucible tools");
        assert_eq!(registry.count(), 10);

        // Verify specific tools are present
        assert!(registry.contains("create_note"));
        assert!(registry.contains("read_note"));
        assert!(registry.contains("text_search"));
        assert!(registry.contains("get_kiln_info"));
    }

    // TDD Cycle 9 - RED: Test expects tool categories
    #[test]
    fn test_tool_categories() {
        let mut registry = ToolRegistry::new();
        discover_crucible_tools(&mut registry, "/test/kiln").unwrap();

        // Check that tools have correct categories
        let create_note = registry.get("create_note");
        assert!(create_note.is_some());
        assert_eq!(create_note.unwrap().category, "notes");

        let text_search = registry.get("text_search");
        assert!(text_search.is_some());
        assert_eq!(text_search.unwrap().category, "search");

        let kiln_info = registry.get("get_kiln_info");
        assert!(kiln_info.is_some());
        assert_eq!(kiln_info.unwrap().category, "kiln");
    }

    // TDD Cycle 9 - RED: Test expects tools to have descriptions
    #[test]
    fn test_tool_descriptions() {
        let mut registry = ToolRegistry::new();
        discover_crucible_tools(&mut registry, "/test/kiln").unwrap();

        let create_note = registry.get("create_note");
        assert!(create_note.is_some());
        assert!(!create_note.unwrap().description.is_empty());

        let read_note = registry.get("read_note");
        assert!(read_note.is_some());
        assert!(!read_note.unwrap().description.is_empty());
    }

    // TDD Cycle 10 - RED: Test expects tool executor creation
    #[test]
    fn test_tool_executor_creation() {
        let executor = ToolExecutor::new(PathBuf::from("/test/kiln"));
        assert_eq!(executor.kiln_path(), &PathBuf::from("/test/kiln"));
    }

    // TDD Cycle 10 - RED: Test expects tool execution to work
    #[tokio::test]
    async fn test_execute_get_kiln_info() {
        let executor = ToolExecutor::new(PathBuf::from("/test/kiln"));

        // Execute get_kiln_info tool (simplest tool with no parameters)
        let params = serde_json::json!({});
        let result = executor.execute("get_kiln_info", params).await;

        // Should succeed and return kiln information
        assert!(result.is_ok(), "get_kiln_info should execute successfully");

        let value = result.unwrap();
        assert!(value.is_object(), "Result should be a JSON object");
    }

    // TDD Cycle 10 - RED: Test expects tool execution with parameters
    #[tokio::test]
    async fn test_execute_read_note() {
        // Create a temporary test kiln
        let temp_dir = std::env::temp_dir().join("crucible-test-kiln");
        std::fs::create_dir_all(&temp_dir).ok();

        // Create a test note
        let test_note = temp_dir.join("test.md");
        std::fs::write(&test_note, "# Test Note\n\nThis is a test.").ok();

        let executor = ToolExecutor::new(temp_dir.clone());

        // Execute read_note tool
        let params = serde_json::json!({
            "path": "test.md"
        });
        let result = executor.execute("read_note", params).await;

        // Should succeed and return the note content
        assert!(result.is_ok(), "read_note should execute successfully");

        let value = result.unwrap();
        assert!(value.is_object(), "Result should be a JSON object");

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    // TDD Cycle 10 - RED: Test expects error for unknown tool
    #[tokio::test]
    async fn test_execute_unknown_tool() {
        let executor = ToolExecutor::new(PathBuf::from("/test/kiln"));

        let params = serde_json::json!({});
        let result = executor.execute("nonexistent_tool", params).await;

        // Should fail with NotFound error
        assert!(result.is_err(), "Unknown tool should return error");
        let err = result.unwrap_err();
        assert!(matches!(err, AcpError::NotFound(_)));
    }

    // TDD Cycle 10 - RED: Test expects error handling for invalid parameters
    #[tokio::test]
    async fn test_execute_invalid_parameters() {
        let executor = ToolExecutor::new(PathBuf::from("/test/kiln"));

        // Try to call read_note without required path parameter
        let params = serde_json::json!({});
        let result = executor.execute("read_note", params).await;

        // Should fail due to missing required parameter
        assert!(result.is_err(), "Invalid parameters should return error");
    }
}
