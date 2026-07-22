//! Agent card types for defining reusable agent configurations
//!
//! Agent cards are markdown files with YAML frontmatter that describe
//! an agent's purpose and system prompt. They follow the "Model Card"
//! pattern from HuggingFace - metadata about an agent, not the agent itself.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Per-tool permission level declared by an agent card.
///
/// Serialized forms accepted (matching the documented card format):
/// `true`/`"allow"` → Allow, `"ask"` → Ask, `false`/`"deny"` → Deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolPolicy {
    /// Auto-approve: the tool never prompts for this agent.
    Allow,
    /// Always prompt, even for tools that are safe by default.
    Ask,
    /// The tool is not advertised to and cannot be executed by this agent.
    Deny,
}

impl<'de> Deserialize<'de> for ToolPolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Bool(bool),
            Str(String),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Bool(true) => Ok(ToolPolicy::Allow),
            Raw::Bool(false) => Ok(ToolPolicy::Deny),
            Raw::Str(s) => match s.as_str() {
                "allow" | "true" => Ok(ToolPolicy::Allow),
                "ask" => Ok(ToolPolicy::Ask),
                "deny" | "false" => Ok(ToolPolicy::Deny),
                other => Err(D::Error::custom(format!(
                    "invalid tool policy '{other}' (expected true/false/allow/ask/deny)"
                ))),
            },
        }
    }
}

/// Tool-name → permission map from a card's `tools:` block.
pub type ToolPolicyMap = HashMap<String, ToolPolicy>;

/// An agent card - static definition of an agent's configuration
///
/// Agent cards are loaded from markdown files and contain:
/// - Identity (name, version, description)
/// - Discovery (tags, specialty)
/// - System prompt (markdown body)
/// - Model selection (provider/model) and generation knobs
/// - Tool policy and MCP server references
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

    /// Informational specialty label (e.g. "reasoning", "coder"). Kept as
    /// metadata; model selection uses explicit `provider`/`model`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specialty: Option<String>,

    /// System prompt (extracted from markdown body)
    pub system_prompt: String,

    /// Optional MCP servers this agent can use
    pub mcp_servers: Vec<String>,

    /// Provider override (e.g. "ollama", "anthropic"). `None` inherits the
    /// spawning context's provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Model override. `None` inherits the spawning context's model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Sampling temperature override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Max output tokens override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Max tool-loop turns (maps to the session's `max_iterations`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,

    /// Initial mode ("auto"/"plan").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    /// Per-tool permission policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolPolicyMap>,

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

/// Frontmatter structure for parsing YAML frontmatter from markdown files.
///
/// Only `description` is required; `name` defaults to the file stem and
/// `version` to `0.1.0`, so the documented minimal card parses as written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCardFrontmatter {
    /// Agent name (defaults to the card's file stem)
    #[serde(default)]
    pub name: Option<String>,

    /// Semantic version (defaults to "0.1.0")
    #[serde(default)]
    pub version: Option<String>,

    /// Required: brief description
    pub description: String,

    /// Optional: tags for discovery
    #[serde(default)]
    pub tags: Vec<String>,

    /// Optional: informational specialty label
    #[serde(default)]
    pub specialty: Option<String>,

    /// Optional: MCP servers this agent uses (doc form `mcps:` accepted)
    #[serde(default, alias = "mcps")]
    pub mcp_servers: Vec<String>,

    /// Optional: provider override
    #[serde(default)]
    pub provider: Option<String>,

    /// Optional: model override
    #[serde(default)]
    pub model: Option<String>,

    /// Optional: sampling temperature
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Optional: max output tokens
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Optional: max tool-loop turns
    #[serde(default)]
    pub max_turns: Option<u32>,

    /// Optional: initial mode ("auto"/"plan")
    #[serde(default)]
    pub mode: Option<String>,

    /// Optional: per-tool permissions (`true`/`false`/`allow`/`ask`/`deny`)
    #[serde(default)]
    pub tools: Option<ToolPolicyMap>,

    /// Optional: configuration values
    pub config: Option<HashMap<String, serde_json::Value>>,
}
