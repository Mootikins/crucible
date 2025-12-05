//! Agent card types for defining reusable agent configurations
//!
//! Agent cards are markdown files with YAML frontmatter that describe
//! an agent's purpose and system prompt. They follow the "Model Card"
//! pattern from HuggingFace - metadata about an agent, not the agent itself.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// An agent card - static definition of an agent's configuration
///
/// Agent cards are loaded from markdown files and contain:
/// - Identity (name, version, description)
/// - Discovery (tags)
/// - System prompt (markdown body)
/// - Optional MCP server references
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    /// Unique identifier for this agent card (generated on load)
    pub id: Uuid,

    /// Human-readable name of the agent
    pub name: String,

    /// Version of this agent card (semantic versioning)
    pub version: String,

    /// Brief description of what this agent does
    pub description: String,

    /// Tags for categorization and discovery
    pub tags: Vec<String>,

    /// System prompt (extracted from markdown body)
    pub system_prompt: String,

    /// Optional MCP servers this agent can use
    pub mcp_servers: Vec<String>,

    /// Default configuration values
    pub config: HashMap<String, serde_json::Value>,

    /// When this agent card was loaded
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}

/// Query for finding agent cards
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentCardQuery {
    /// Search by tags
    #[serde(default)]
    pub tags: Vec<String>,

    /// Text search in name and description
    pub text_search: Option<String>,
}

/// Result of matching an agent card to a query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCardMatch {
    /// The matched agent card
    pub card: AgentCard,

    /// Match score (0-100)
    pub score: u32,

    /// Which criteria matched
    pub matched_criteria: Vec<String>,
}

/// Frontmatter structure for parsing YAML frontmatter from markdown files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCardFrontmatter {
    /// Required: agent name
    pub name: String,

    /// Required: semantic version
    pub version: String,

    /// Required: brief description
    pub description: String,

    /// Optional: tags for discovery
    #[serde(default)]
    pub tags: Vec<String>,

    /// Optional: MCP servers this agent uses
    #[serde(default)]
    pub mcp_servers: Vec<String>,

    /// Optional: configuration values
    pub config: Option<HashMap<String, serde_json::Value>>,
}
