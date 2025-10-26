use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// Unique identifier for this agent
    pub id: Uuid,

    /// Human-readable name of the agent
    pub name: String,

    /// Version of this agent definition (semantic versioning)
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

    /// Personality and behavior configuration
    pub personality: Personality,

    /// System prompt template (can use placeholders)
    pub system_prompt: String,

    /// Skills this agent possesses
    pub skills: Vec<Skill>,

    /// Default configuration values
    pub config: HashMap<String, serde_json::Value>,

    /// Dependencies on other agents (by ID or name)
    pub dependencies: Vec<String>,

    /// When this agent was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// When this agent was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,

    /// Status of the agent (active, deprecated, experimental)
    pub status: AgentStatus,

    /// Author/maintainer information
    pub author: Option<String>,

    /// Documentation URL (optional)
    pub documentation_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// Name of the capability
    pub name: String,

    /// Description of what this capability enables
    pub description: String,

    /// Skill level (beginner, intermediate, advanced, expert)
    pub skill_level: SkillLevel,

    /// Tool requirements for this specific capability
    pub required_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Name of the skill
    pub name: String,

    /// Category of the skill (e.g., "programming", "analysis", "communication")
    pub category: String,

    /// Proficiency level (1-10)
    pub proficiency: u8,

    /// Years of experience (simulated)
    pub experience_years: f32,

    /// Certifications or special qualifications
    pub certifications: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Personality {
    /// Tone of communication (professional, casual, friendly, etc.)
    pub tone: String,

    /// Communication style (concise, detailed, formal, etc.)
    pub style: String,

    /// Response verbosity (brief, moderate, detailed)
    pub verbosity: Verbosity,

    /// Key personality traits
    pub traits: Vec<String>,

    /// Behavioral preferences
    pub preferences: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillLevel {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Verbosity {
    Brief,
    Moderate,
    Detailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentStatus {
    Active,
    Deprecated,
    Experimental,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentQuery {
    /// Search by capabilities
    pub capabilities: Vec<String>,

    /// Search by tags
    pub tags: Vec<String>,

    /// Search by skills
    pub skills: Vec<String>,

    /// Required tools
    pub required_tools: Vec<String>,

    /// Skill level filter
    pub min_skill_level: Option<SkillLevel>,

    /// Status filter
    pub status: Option<AgentStatus>,

    /// Text search in name and description
    pub text_search: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMatch {
    /// The matched agent
    pub agent: AgentDefinition,

    /// Match score (0-100)
    pub score: u32,

    /// Which criteria matched
    pub matched_criteria: Vec<String>,

    /// Missing requirements (if any)
    pub missing_requirements: Vec<String>,
}

/// Frontmatter structure for parsing YAML frontmatter from markdown files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFrontmatter {
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<CapabilityFrontmatter>,
    pub required_tools: Vec<String>,
    pub optional_tools: Option<Vec<String>>,
    pub tags: Vec<String>,
    pub personality: PersonalityFrontmatter,
    pub skills: Vec<SkillFrontmatter>,
    pub config: Option<HashMap<String, serde_json::Value>>,
    pub dependencies: Option<Vec<String>>,
    pub status: Option<AgentStatus>,
    pub author: Option<String>,
    pub documentation_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityFrontmatter {
    pub name: String,
    pub description: String,
    pub skill_level: SkillLevel,
    pub required_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub category: String,
    pub proficiency: u8,
    pub experience_years: Option<f32>,
    pub certifications: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityFrontmatter {
    pub tone: String,
    pub style: String,
    pub verbosity: Verbosity,
    pub traits: Vec<String>,
    pub preferences: Option<HashMap<String, String>>,
}
