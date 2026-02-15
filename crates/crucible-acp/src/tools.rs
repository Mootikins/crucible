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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::{ClientError, Result};

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
            return Err(ClientError::InvalidConfig(format!(
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

/// Get the system prompt for Crucible
///
/// Provides concise context about the knowledge base and tools.
/// Designed to be low-token while covering essential information.
pub fn get_crucible_system_prompt() -> String {
    "You are working with Crucible, a knowledge management system. \
You have 10 tools for working with markdown notes in a 'kiln' (knowledge repository):

Notes: read_note, create_note, update_note, delete_note, list_notes, read_metadata
Search: text_search, property_search, semantic_search
Kiln: get_kiln_info

When referencing notes, use simple names (\"My Note\") or wikilinks (\"[[My Note]]\"). \
The system finds notes anywhere in the kiln. Full paths (\"folder/note.md\") also work. \
Notes support YAML frontmatter for metadata."
        .to_string()
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
        create_tool(
            "create_note",
            "Create a new note in the kiln",
            "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the note file relative to the kiln"},
                    "content": {"type": "string", "description": "Content of the note"},
                    "frontmatter": {"type": "object", "description": "Optional YAML frontmatter metadata"}
                },
                "required": ["path", "content"]
            }),
        ),
        create_tool(
            "read_note",
            "Read the contents of a note",
            "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the note file"},
                    "start_line": {"type": "integer", "description": "Optional starting line number"},
                    "end_line": {"type": "integer", "description": "Optional ending line number"}
                },
                "required": ["path"]
            }),
        ),
        create_tool(
            "read_metadata",
            "Read only the frontmatter metadata of a note",
            "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the note file"}
                },
                "required": ["path"]
            }),
        ),
        create_tool(
            "update_note",
            "Update an existing note's content or metadata",
            "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the note file"},
                    "content": {"type": "string", "description": "New content for the note"},
                    "frontmatter": {"type": "object", "description": "Updated frontmatter metadata"}
                },
                "required": ["path"]
            }),
        ),
        create_tool(
            "delete_note",
            "Delete a note from the kiln",
            "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the note file to delete"}
                },
                "required": ["path"]
            }),
        ),
        create_tool(
            "list_notes",
            "List all notes in the kiln or a specific folder",
            "notes",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "folder": {"type": "string", "description": "Optional folder path to list notes from"},
                    "include_frontmatter": {"type": "boolean", "description": "Whether to include frontmatter in results"},
                    "recursive": {"type": "boolean", "description": "Whether to recursively list notes in subfolders"}
                }
            }),
        ),
    ];

    for tool in note_tools {
        registry.register(tool)?;
        count += 1;
    }

    // Register SearchTools (3 tools)
    let search_tools = vec![
        create_tool(
            "text_search",
            "Search for notes by text content",
            "search",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Text query to search for"},
                    "limit": {"type": "integer", "description": "Maximum number of results to return"}
                },
                "required": ["query"]
            }),
        ),
        create_tool(
            "property_search",
            "Search for notes by frontmatter properties",
            "search",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "property": {"type": "string", "description": "Property name to search"},
                    "value": {"type": "string", "description": "Value to match"}
                },
                "required": ["property", "value"]
            }),
        ),
        create_tool(
            "semantic_search",
            "Search for notes by semantic similarity using embeddings",
            "search",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Query text to find semantically similar notes"},
                    "limit": {"type": "integer", "description": "Maximum number of results to return"}
                },
                "required": ["query"]
            }),
        ),
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
        serde_json::json!({"type": "object", "properties": {}}),
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
    pub async fn execute(
        &self,
        tool_name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
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
            _ => Err(ClientError::NotFound(format!(
                "Unknown tool: {}",
                tool_name
            ))),
        }
    }

    /// Helper to extract a required string parameter
    fn get_required_param(params: &serde_json::Value, name: &str) -> Result<String> {
        params
            .get(name)
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| {
                ClientError::InvalidConfig(format!("Missing required parameter: {}", name))
            })
    }

    /// Resolve a note name or path to a full path
    ///
    /// Supports:
    /// - Full paths: "folder/note.md" → kiln_path/folder/note.md
    /// - Note names: "Note Name" → finds first matching note
    /// - Note names with extension: "Note Name.md" → finds first matching note
    /// - Wikilink format: "[[Note Name]]" → strips brackets and finds note
    async fn resolve_note_path(&self, name_or_path: &str) -> Result<PathBuf> {
        let name_or_path = name_or_path.trim();

        // Strip wikilink brackets if present
        let cleaned = if name_or_path.starts_with("[[") && name_or_path.ends_with("]]") {
            &name_or_path[2..name_or_path.len() - 2]
        } else {
            name_or_path
        };

        // If it looks like a path (contains / or \), use it directly
        if cleaned.contains('/') || cleaned.contains('\\') {
            let full_path = self.kiln_path.join(cleaned);
            if full_path.exists() {
                return Ok(full_path);
            }
            return Err(ClientError::NotFound(format!(
                "Note not found at path: {}",
                cleaned
            )));
        }

        // Otherwise, search for the note by name
        let search_name = if cleaned.ends_with(".md") {
            cleaned.to_string()
        } else {
            format!("{}.md", cleaned)
        };

        // Search the kiln for matching note
        self.find_note_by_name(&search_name).await
    }

    /// Find a note by name (recursively searches kiln)
    async fn find_note_by_name(&self, name: &str) -> Result<PathBuf> {
        use tokio::fs;

        // Try direct path first (most common case)
        let direct_path = self.kiln_path.join(name);
        if direct_path.exists() {
            return Ok(direct_path);
        }

        // Recursively search for the note
        let mut stack = vec![self.kiln_path.clone()];

        while let Some(dir) = stack.pop() {
            let mut entries = fs::read_dir(&dir)
                .await
                .map_err(|e| ClientError::FileSystem(format!("Failed to read directory: {}", e)))?;

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|e| ClientError::FileSystem(format!("Failed to read entry: {}", e)))?
            {
                let path = entry.path();

                if path.is_dir() {
                    // Add subdirectory to search
                    stack.push(path);
                } else if let Some(file_name) = path.file_name() {
                    if file_name == name {
                        return Ok(path);
                    }
                }
            }
        }

        Err(ClientError::NotFound(format!("Note not found: {}", name)))
    }

    /// Execute a note tool
    async fn execute_note_tool(
        &self,
        tool_name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        match tool_name {
            "create_note" => {
                // Extract parameters
                let path = Self::get_required_param(&params, "path")?;
                let content = Self::get_required_param(&params, "content")?;

                // Resolve the path (supports note names)
                let full_path = if path.contains('/') || path.contains('\\') {
                    // Looks like a path, use directly
                    self.kiln_path.join(&path)
                } else {
                    // Note name - create in root with .md extension
                    let file_name = if path.ends_with(".md") {
                        path.clone()
                    } else {
                        format!("{}.md", path)
                    };
                    self.kiln_path.join(file_name)
                };

                // Execute the tool by directly writing the file
                tokio::fs::write(&full_path, &content).await.map_err(|e| {
                    ClientError::FileSystem(format!("Failed to create note: {}", e))
                })?;

                Ok(serde_json::json!({
                    "path": path,
                    "full_path": full_path.to_string_lossy(),
                    "status": "created"
                }))
            }
            "read_note" => {
                // Extract parameters
                let path = Self::get_required_param(&params, "path")?;

                // Resolve note name to path
                let full_path = self.resolve_note_path(&path).await?;

                let content = tokio::fs::read_to_string(&full_path)
                    .await
                    .map_err(|e| ClientError::FileSystem(format!("Failed to read note: {}", e)))?;

                Ok(serde_json::json!({
                    "path": path,
                    "full_path": full_path.to_string_lossy(),
                    "content": content,
                    "lines": content.lines().count()
                }))
            }
            _ => Err(ClientError::NotFound(format!(
                "Note tool not implemented: {}",
                tool_name
            ))),
        }
    }

    /// Execute a search tool
    async fn execute_search_tool(
        &self,
        tool_name: &str,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        // For now, return a placeholder for search tools
        // Full implementation would integrate with crucible-tools search
        Ok(serde_json::json!({
            "tool": tool_name,
            "results": [],
            "count": 0
        }))
    }

    /// Execute a kiln tool
    async fn execute_kiln_tool(
        &self,
        _tool_name: &str,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
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
    use async_trait::async_trait;
    use crucible_core::traits::acp::{AcpError, ToolBridge};
    use crucible_core::traits::tools::{
        ExecutionContext, ToolDefinition, ToolError, ToolExecutor as CoreToolExecutor,
    };
    use crucible_core::types::acp::{ToolInvocation, ToolOutput};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tempfile::TempDir;

    fn test_tool(name: &str, category: &str, input_schema: serde_json::Value) -> ToolDescriptor {
        ToolDescriptor {
            name: name.to_string(),
            description: format!("{} description", name),
            category: category.to_string(),
            input_schema,
        }
    }

    fn object_schema(required: &[&str]) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            },
            "required": required,
        })
    }

    fn is_valid_input_schema(schema: &serde_json::Value) -> bool {
        schema.get("type").and_then(serde_json::Value::as_str) == Some("object")
    }

    struct MockCoreExecutor {
        tools: Vec<ToolDefinition>,
    }

    #[async_trait]
    impl CoreToolExecutor for MockCoreExecutor {
        async fn execute_tool(
            &self,
            name: &str,
            params: serde_json::Value,
            _context: &ExecutionContext,
        ) -> std::result::Result<serde_json::Value, ToolError> {
            if name == "echo" {
                return Ok(params);
            }
            Err(ToolError::NotFound(name.to_string()))
        }

        async fn list_tools(&self) -> std::result::Result<Vec<ToolDefinition>, ToolError> {
            Ok(self.tools.clone())
        }
    }

    struct PermissionedToolBridge {
        registry: ToolRegistry,
        allow_unsafe: bool,
        permission_checks: Arc<AtomicUsize>,
    }

    impl PermissionedToolBridge {
        fn new(allow_unsafe: bool) -> Self {
            let mut registry = ToolRegistry::new();
            registry
                .register(test_tool("read_note", "notes", object_schema(&["path"])))
                .unwrap();
            registry
                .register(test_tool(
                    "create_note",
                    "notes",
                    object_schema(&["path", "content"]),
                ))
                .unwrap();

            Self {
                registry,
                allow_unsafe,
                permission_checks: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn requires_permission_check(tool_name: &str) -> bool {
            !matches!(tool_name, "read_note" | "list_notes" | "read_metadata")
        }
    }

    #[async_trait]
    impl ToolBridge for PermissionedToolBridge {
        type ToolCall = ToolInvocation;
        type ToolResult = ToolOutput;
        type ToolDescriptor = ToolDescriptor;

        async fn execute_tool(
            &self,
            call: Self::ToolCall,
        ) -> std::result::Result<Self::ToolResult, AcpError> {
            if !self.registry.contains(&call.tool_name) {
                return Err(AcpError::NotFound(call.tool_name));
            }

            if Self::requires_permission_check(&call.tool_name) {
                self.permission_checks.fetch_add(1, Ordering::SeqCst);
                if !self.allow_unsafe {
                    return Err(AcpError::PermissionDenied(format!(
                        "Tool '{}' denied by permission gate",
                        call.tool_name
                    )));
                }
            }

            Ok(ToolOutput::success(json!({
                "tool": call.tool_name,
                "parameters": call.parameters,
            })))
        }

        async fn list_tools(&self) -> std::result::Result<Vec<Self::ToolDescriptor>, AcpError> {
            Ok(self
                .registry
                .list()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>())
        }

        async fn get_tool_schema(
            &self,
            tool_name: &str,
        ) -> std::result::Result<serde_json::Value, AcpError> {
            self.registry
                .get(tool_name)
                .map(|tool| tool.input_schema.clone())
                .ok_or_else(|| AcpError::NotFound(tool_name.to_string()))
        }
    }

    #[test]
    fn register_tool_happy_path() {
        let mut registry = ToolRegistry::new();
        let descriptor = test_tool("test_tool", "test", object_schema(&["path"]));

        registry.register(descriptor.clone()).unwrap();

        assert_eq!(registry.count(), 1);
        assert_eq!(registry.get("test_tool"), Some(&descriptor));
    }

    #[test]
    fn list_tools_returns_registered_tools() {
        let mut registry = ToolRegistry::new();
        registry
            .register(test_tool("tool_one", "test", object_schema(&[])))
            .unwrap();
        registry
            .register(test_tool("tool_two", "test", object_schema(&[])))
            .unwrap();

        let mut names = registry
            .list()
            .into_iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();

        assert_eq!(names, vec!["tool_one", "tool_two"]);
    }

    #[tokio::test]
    async fn execute_tool_happy_path() {
        let temp = TempDir::new().unwrap();
        let executor = ToolExecutor::new(temp.path().to_path_buf());

        let output = executor.execute("get_kiln_info", json!({})).await.unwrap();
        assert_eq!(output["exists"], json!(true));
        assert_eq!(output["is_directory"], json!(true));
    }

    #[test]
    fn register_duplicate_tool_name_fails() {
        let mut registry = ToolRegistry::new();
        let descriptor = test_tool("duplicate", "test", object_schema(&[]));
        registry.register(descriptor.clone()).unwrap();

        let err = registry.register(descriptor).unwrap_err();
        assert!(matches!(err, ClientError::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn execute_unknown_tool_name_fails() {
        let executor = ToolExecutor::new(PathBuf::from("/tmp"));
        let err = executor
            .execute("does_not_exist", json!({}))
            .await
            .unwrap_err();

        assert!(matches!(err, ClientError::NotFound(_)));
    }

    #[tokio::test]
    async fn execute_tool_with_invalid_input_schema_fails() {
        let executor = ToolExecutor::new(PathBuf::from("/tmp"));

        let err = executor
            .execute("read_note", json!({ "path": 123 }))
            .await
            .unwrap_err();

        assert!(matches!(err, ClientError::InvalidConfig(_)));
    }

    #[test]
    fn invalid_input_schema_is_detectable() {
        let invalid = test_tool("bad_schema", "test", json!("not-an-object"));
        assert!(!is_valid_input_schema(&invalid.input_schema));
    }

    #[test]
    fn tool_descriptor_serialization_round_trip() {
        let descriptor = test_tool("round_trip", "test", object_schema(&["path"]));

        let serialized = serde_json::to_string(&descriptor).unwrap();
        let deserialized: ToolDescriptor = serde_json::from_str(&serialized).unwrap();

        assert_eq!(descriptor, deserialized);
    }

    #[test]
    fn crucible_system_prompt_is_non_empty_and_well_formed() {
        let prompt = get_crucible_system_prompt();
        assert!(!prompt.trim().is_empty());
        assert!(prompt.contains("You are working with Crucible"));
        assert!(prompt.contains("read_note"));
        assert!(prompt.contains("semantic_search"));
    }

    #[tokio::test]
    async fn core_tool_executor_trait_contract_is_satisfied() {
        let executor = MockCoreExecutor {
            tools: vec![ToolDefinition::new("echo", "Echo input")],
        };

        let listed = executor.list_tools().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "echo");

        let context = ExecutionContext::new();
        let output = executor
            .execute_tool("echo", json!({"value": 1}), &context)
            .await
            .unwrap();
        assert_eq!(output, json!({"value": 1}));
    }

    #[tokio::test]
    async fn permission_gate_allowed_passes() {
        let bridge = PermissionedToolBridge::new(true);

        let output = bridge
            .execute_tool(ToolInvocation::new(
                "create_note",
                json!({"path": "x.md", "content": "ok"}),
            ))
            .await
            .unwrap();

        assert!(output.success);
        assert_eq!(bridge.permission_checks.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn permission_gate_denied_fails_with_permission_denied() {
        let bridge = PermissionedToolBridge::new(false);

        let err = bridge
            .execute_tool(ToolInvocation::new(
                "create_note",
                json!({"path": "x.md", "content": "blocked"}),
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, AcpError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn permission_patterns_safe_tool_skips_permission_check() {
        let bridge = PermissionedToolBridge::new(false);

        let output = bridge
            .execute_tool(ToolInvocation::new("read_note", json!({"path": "x.md"})))
            .await
            .unwrap();

        assert!(output.success);
        assert_eq!(bridge.permission_checks.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn permission_patterns_unsafe_tool_triggers_check() {
        let bridge = PermissionedToolBridge::new(true);

        bridge
            .execute_tool(ToolInvocation::new(
                "create_note",
                json!({"path": "x.md", "content": "ok"}),
            ))
            .await
            .unwrap();

        assert_eq!(bridge.permission_checks.load(Ordering::SeqCst), 1);
    }
}
