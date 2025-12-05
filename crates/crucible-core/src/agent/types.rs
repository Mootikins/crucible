//! Agent card types for defining reusable agent configurations
//!
//! Agent cards are markdown files with YAML frontmatter that describe
//! an agent's purpose, capabilities, and system prompt. They follow
//! the "Model Card" pattern from HuggingFace - metadata about an agent,
//! not the agent itself.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// An agent card - static definition of an agent's configuration
///
/// Agent cards are loaded from markdown files and contain:
/// - Metadata (name, description, tags)
/// - System prompt template
/// - Required/optional tools
/// - Capabilities for matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    /// Unique identifier for this agent card
    pub id: Uuid,

    /// Human-readable name of the agent
    pub name: String,

    /// Version of this agent card (semantic versioning)
    pub version: String,

    /// Brief description of what this agent does
    pub description: String,

    /// Detailed capabilities and specializations
    pub capabilities: Vec<Capability>,

    /// MCP tools required by this agent
    pub required_tools: Vec<String>,

    /// Optional MCP tools that enhance functionality
    pub optional_tools: Vec<String>,

    /// Tags for categorization and discovery
    pub tags: Vec<String>,

    /// System prompt template (can use placeholders)
    pub system_prompt: String,

    /// Skills this agent possesses (for matching)
    pub skills: Vec<Skill>,

    /// Default configuration values
    pub config: HashMap<String, serde_json::Value>,

    /// Dependencies on other agent cards (by ID or name)
    pub dependencies: Vec<String>,

    /// When this agent card was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// When this agent card was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,

    /// Status of the agent card (active, deprecated, experimental)
    pub status: AgentCardStatus,

    /// Author/maintainer information
    pub author: Option<String>,

    /// Documentation URL (optional)
    pub documentation_url: Option<String>,
}

/// A capability that an agent card provides
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// Name of the capability
    pub name: String,

    /// Description of what this capability enables
    pub description: String,

    /// Tool requirements for this specific capability
    pub required_tools: Vec<String>,
}

/// A skill that an agent possesses (for matching purposes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Name of the skill
    pub name: String,

    /// Category of the skill (e.g., "programming", "analysis", "communication")
    pub category: String,
}

/// Status of an agent card
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentCardStatus {
    Active,
    Deprecated,
    Experimental,
    Disabled,
}

/// Query for finding agent cards
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCardQuery {
    /// Search by capabilities
    pub capabilities: Vec<String>,

    /// Search by tags
    pub tags: Vec<String>,

    /// Search by skills
    pub skills: Vec<String>,

    /// Required tools
    pub required_tools: Vec<String>,

    /// Status filter
    pub status: Option<AgentCardStatus>,

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

    /// Missing requirements (if any)
    pub missing_requirements: Vec<String>,
}

/// Frontmatter structure for parsing YAML frontmatter from markdown files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCardFrontmatter {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub capabilities: Vec<CapabilityFrontmatter>,
    #[serde(default)]
    pub required_tools: Vec<String>,
    pub optional_tools: Option<Vec<String>>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub skills: Vec<SkillFrontmatter>,
    pub config: Option<HashMap<String, serde_json::Value>>,
    pub dependencies: Option<Vec<String>>,
    pub status: Option<AgentCardStatus>,
    pub author: Option<String>,
    pub documentation_url: Option<String>,
}

/// Capability definition in frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityFrontmatter {
    pub name: String,
    pub description: String,
    pub required_tools: Option<Vec<String>>,
}

/// Skill definition in frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub category: String,
}
