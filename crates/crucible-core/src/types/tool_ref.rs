//! Tool reference abstraction
//!
//! Provides a unified representation for tools from various sources
//! (core, Crucible, MCP gateway, plugins) that can be indexed and searched.
//!
//! ## Design Principles
//!
//! - Uses `rmcp::model::Tool` directly for schema (no duplication)
//! - Adds metadata for grouping, indexing, and source tracking
//! - Works in both ACP (MCP transport) and internal agent (direct call) modes

use rmcp::model::Tool;
use serde::{Deserialize, Serialize};

/// Reference to a tool with source and indexing metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRef {
    /// Canonical name (e.g., "read_file", "semantic_search", "gmail__send_email")
    pub name: String,

    /// Where this tool comes from
    pub source: ToolSource,

    /// The actual tool definition (rmcp type)
    /// Serializes/deserializes via rmcp's serde impl
    #[serde(with = "tool_serde")]
    pub definition: Tool,

    /// Tags for indexing and search (e.g., ["file", "read", "workspace"])
    #[serde(default)]
    pub tags: Vec<String>,

    /// Whether this tool is always available (core) or discovered via search
    #[serde(default)]
    pub always_available: bool,
}

/// Source of a tool
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolSource {
    /// Core workspace tools (read_file, edit_file, bash, glob, grep)
    Core,

    /// Crucible knowledge tools (semantic_search, notes, etc.)
    Crucible,

    /// Tool from MCP gateway
    Mcp {
        /// MCP server name
        server: String,
    },

    /// Plugin-provided tool (future: Rune scripts)
    Plugin {
        /// Plugin name
        name: String,
    },
}

impl ToolRef {
    /// Create a new core tool reference
    pub fn core(name: impl Into<String>, definition: Tool) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            source: ToolSource::Core,
            definition,
            tags: vec!["core".to_string()],
            always_available: true,
        }
    }

    /// Create a new Crucible tool reference
    pub fn crucible(name: impl Into<String>, definition: Tool) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            source: ToolSource::Crucible,
            definition,
            tags: vec!["crucible".to_string()],
            always_available: true,
        }
    }

    /// Create a tool reference from an MCP server
    pub fn from_mcp(server: impl Into<String>, definition: Tool) -> Self {
        let server = server.into();
        let name = definition.name.to_string();
        Self {
            name: format!("{}_{}", server, name),
            source: ToolSource::Mcp {
                server: server.clone(),
            },
            definition,
            tags: vec!["mcp".to_string(), server],
            always_available: false, // Discovered via search
        }
    }

    /// Create a tool reference from a plugin
    pub fn from_plugin(plugin: impl Into<String>, definition: Tool) -> Self {
        let plugin = plugin.into();
        let name = definition.name.to_string();
        Self {
            name: format!("{}_{}", plugin, name),
            source: ToolSource::Plugin {
                name: plugin.clone(),
            },
            definition,
            tags: vec!["plugin".to_string(), plugin],
            always_available: false,
        }
    }

    /// Add tags for indexing
    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags.extend(tags.into_iter().map(|t| t.into()));
        self
    }

    /// Mark as always available (included in agent context)
    pub fn with_always_available(mut self, always: bool) -> Self {
        self.always_available = always;
        self
    }

    /// Get the tool's description for search indexing
    pub fn description(&self) -> &str {
        self.definition
            .description
            .as_ref()
            .map(|s| s.as_ref())
            .unwrap_or("")
    }

    /// Get searchable text (name + description + tags)
    pub fn searchable_text(&self) -> String {
        let mut text = self.name.clone();
        text.push(' ');
        text.push_str(self.description());
        for tag in &self.tags {
            text.push(' ');
            text.push_str(tag);
        }
        text
    }

    /// Check if this tool matches a source type
    pub fn is_core(&self) -> bool {
        matches!(self.source, ToolSource::Core)
    }

    pub fn is_crucible(&self) -> bool {
        matches!(self.source, ToolSource::Crucible)
    }

    pub fn is_mcp(&self) -> bool {
        matches!(self.source, ToolSource::Mcp { .. })
    }

    pub fn is_plugin(&self) -> bool {
        matches!(self.source, ToolSource::Plugin { .. })
    }
}

impl From<Tool> for ToolRef {
    /// Convert an rmcp Tool to ToolRef (assumes Core source)
    fn from(tool: Tool) -> Self {
        let name = tool.name.to_string();
        Self {
            name,
            source: ToolSource::Core,
            definition: tool,
            tags: Vec::new(),
            always_available: true,
        }
    }
}

impl AsRef<Tool> for ToolRef {
    fn as_ref(&self) -> &Tool {
        &self.definition
    }
}

/// Serde helper for rmcp::model::Tool
///
/// Tool contains Cow<'static, str> which needs special handling
mod tool_serde {
    use rmcp::model::Tool;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(tool: &Tool, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Tool implements Serialize
        tool.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Tool, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Tool implements Deserialize
        Tool::deserialize(deserializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    fn make_test_tool(name: &str, desc: &str) -> Tool {
        Tool {
            name: Cow::Owned(name.to_string()),
            title: None,
            description: Some(Cow::Owned(desc.to_string())),
            input_schema: Default::default(),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    #[test]
    fn test_core_tool_ref() {
        let tool = make_test_tool("read_file", "Read file contents");
        let tool_ref = ToolRef::core("read_file", tool);

        assert_eq!(tool_ref.name, "read_file");
        assert!(tool_ref.is_core());
        assert!(tool_ref.always_available);
        assert!(tool_ref.tags.contains(&"core".to_string()));
    }

    #[test]
    fn test_crucible_tool_ref() {
        let tool = make_test_tool("semantic_search", "Search notes semantically");
        let tool_ref = ToolRef::crucible("semantic_search", tool);

        assert_eq!(tool_ref.name, "semantic_search");
        assert!(tool_ref.is_crucible());
        assert!(tool_ref.always_available);
    }

    #[test]
    fn test_mcp_tool_ref() {
        let tool = make_test_tool("send_email", "Send an email");
        let tool_ref = ToolRef::from_mcp("gmail", tool);

        assert_eq!(tool_ref.name, "gmail_send_email");
        assert!(tool_ref.is_mcp());
        assert!(!tool_ref.always_available);
        assert!(tool_ref.tags.contains(&"mcp".to_string()));
        assert!(tool_ref.tags.contains(&"gmail".to_string()));
    }

    #[test]
    fn test_searchable_text() {
        let tool = make_test_tool("read_file", "Read file contents from disk");
        let tool_ref = ToolRef::core("read_file", tool).with_tags(["filesystem", "io"]);

        let text = tool_ref.searchable_text();
        assert!(text.contains("read_file"));
        assert!(text.contains("Read file contents"));
        assert!(text.contains("filesystem"));
        assert!(text.contains("io"));
    }

    #[test]
    fn test_from_tool() {
        let tool = make_test_tool("test", "Test tool");
        let tool_ref: ToolRef = tool.into();

        assert_eq!(tool_ref.name, "test");
        assert!(tool_ref.is_core()); // Default source
    }

    #[test]
    fn test_tool_source_serialization() {
        let source = ToolSource::Mcp {
            server: "gmail".to_string(),
        };
        let json = serde_json::to_string(&source).unwrap();
        assert!(json.contains("\"type\":\"mcp\""));
        assert!(json.contains("\"server\":\"gmail\""));

        let parsed: ToolSource = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, source);
    }
}
