//! SKILL.md parser following agentskills.io specification

use crate::error::{SkillError, SkillResult};
use crate::types::{Skill, SkillFrontmatter, SkillSource};
use chrono::Utc;

/// Parser for SKILL.md files
pub struct SkillParser;

impl SkillParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse SKILL.md content into a Skill
    pub fn parse(&self, content: &str, source: SkillSource) -> SkillResult<Skill> {
        let (frontmatter, body) = self.split_frontmatter(content)?;
        let fm: SkillFrontmatter = serde_yaml::from_str(&frontmatter).map_err(|e| {
            SkillError::ParseError {
                path: source.path.clone(),
                source: e,
            }
        })?;

        // Validate required fields
        if fm.description.is_empty() {
            return Err(SkillError::ValidationError {
                reason: "description is required".to_string(),
            });
        }

        // Parse allowed-tools from space-delimited string
        let allowed_tools = fm.allowed_tools.map(|s| {
            s.split_whitespace()
                .map(|t| t.to_string())
                .collect()
        });

        Ok(Skill {
            name: fm.name,
            description: fm.description,
            body: body.trim().to_string(),
            license: fm.license,
            compatibility: fm.compatibility,
            allowed_tools,
            metadata: fm.metadata,
            source,
            indexed_at: Utc::now(),
        })
    }

    /// Split content into frontmatter and body
    fn split_frontmatter(&self, content: &str) -> SkillResult<(String, String)> {
        let content = content.trim();

        if !content.starts_with("---") {
            return Err(SkillError::ValidationError {
                reason: "SKILL.md must start with YAML frontmatter (---)".to_string(),
            });
        }

        // Find closing ---
        let rest = &content[3..];
        let end_idx = rest.find("\n---").ok_or_else(|| SkillError::ValidationError {
            reason: "Missing closing --- for frontmatter".to_string(),
        })?;

        let frontmatter = rest[..end_idx].trim().to_string();
        let body = rest[end_idx + 4..].to_string();

        Ok((frontmatter, body))
    }
}

impl Default for SkillParser {
    fn default() -> Self {
        Self::new()
    }
}
