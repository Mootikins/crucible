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

    /// Plugin-provided tool
    Plugin {
        /// Plugin name
        name: String,
    },

    /// Tool executed inside a delegated ACP agent's own tool loop.
    ///
    /// Never appears in our tool registry — the agent owns the tool and ran
    /// it in its own process under its own permission gate. It exists as a
    /// `ToolSource` because it is provenance the user must see on the card,
    /// and provenance has exactly one wire grammar (`format_tool_source`).
    Acp {
        /// Configured ACP agent name (`claude`, `opencode`, …)
        agent: String,
    },
}

impl ToolRef {
    /// Add tags for indexing
    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags.extend(tags.into_iter().map(|t| t.into()));
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
    use std::sync::Arc;

    fn make_test_tool(name: &str, desc: &str) -> Tool {
        Tool::new(name.to_string(), desc.to_string(), Arc::default())
    }

    #[test]
    fn test_from_tool() {
        let tool = make_test_tool("test", "Test tool");
        let tool_ref: ToolRef = tool.into();

        assert_eq!(tool_ref.name, "test");
        assert!(matches!(tool_ref.source, ToolSource::Core)); // Default source
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

    /// `Acp` has no constructor on `ToolRef` — it is built directly by the ACP
    /// translator and reaches the TUI and the web view as wire data, so its tag
    /// and field name are part of the protocol even though no `from_acp` pins
    /// them.
    #[test]
    fn test_acp_tool_source_serialization() {
        let source = ToolSource::Acp {
            agent: "claude".to_string(),
        };
        let json = serde_json::to_string(&source).unwrap();
        assert!(
            json.contains("\"type\":\"acp\""),
            "unexpected tag in {json}"
        );
        assert!(
            json.contains("\"agent\":\"claude\""),
            "unexpected payload in {json}"
        );

        let parsed: ToolSource = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, source);
    }
}
