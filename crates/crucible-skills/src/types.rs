//! Core types for Agent Skills

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Scope/priority level for a skill
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SkillScope {
    /// Personal skills (~/.config/crucible/skills/)
    Personal,
    /// Workspace skills (.<agent>/skills/ in project)
    Workspace,
    /// Kiln-specific skills (<kiln>/skills/)
    Kiln,
}

impl std::fmt::Display for SkillScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillScope::Personal => write!(f, "personal"),
            SkillScope::Workspace => write!(f, "workspace"),
            SkillScope::Kiln => write!(f, "kiln"),
        }
    }
}

/// Source information for a discovered skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSource {
    /// Which agent's directory this came from (claude, codex, crucible, etc.)
    pub agent: Option<String>,
    /// Scope level
    pub scope: SkillScope,
    /// Full path to SKILL.md
    pub path: PathBuf,
    /// Content hash for change detection
    pub content_hash: String,
}

/// Parsed skill from SKILL.md
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Skill name (from frontmatter, must match directory name)
    pub name: String,
    /// Description (from frontmatter)
    pub description: String,
    /// Full markdown body (instructions)
    pub body: String,
    /// Optional license
    pub license: Option<String>,
    /// Optional compatibility notes
    pub compatibility: Option<String>,
    /// Optional allowed tools list
    pub allowed_tools: Option<Vec<String>>,
    /// Arbitrary metadata from frontmatter
    pub metadata: HashMap<String, serde_json::Value>,
    /// Source information
    pub source: SkillSource,
    /// When this skill was indexed
    pub indexed_at: DateTime<Utc>,
}

impl Skill {
    /// Get unique identifier (scope + name)
    pub fn id(&self) -> String {
        format!("{}:{}", self.source.scope, self.name)
    }
}

/// A skill after priority resolution (may shadow others)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSkill {
    /// The winning skill
    pub skill: Skill,
    /// Paths of lower-priority skills this shadows
    pub shadowed: Vec<PathBuf>,
}

/// SKILL.md frontmatter structure (per agentskills.io spec)
#[derive(Debug, Clone, Deserialize)]
pub struct SkillFrontmatter {
    /// Required: 1-64 chars, lowercase alphanumeric + hyphens
    pub name: String,
    /// Required: 1-1024 chars
    pub description: String,
    /// Optional license
    pub license: Option<String>,
    /// Optional compatibility notes
    pub compatibility: Option<String>,
    /// Optional allowed tools (space-delimited in YAML)
    #[serde(rename = "allowed-tools")]
    pub allowed_tools: Option<String>,
    /// Catch-all for other metadata
    #[serde(flatten)]
    pub metadata: HashMap<String, serde_json::Value>,
}
