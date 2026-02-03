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
        let fm: SkillFrontmatter =
            serde_yaml::from_str(&frontmatter).map_err(|e| SkillError::ParseError {
                path: source.path.clone(),
                source: e,
            })?;

        // Validate required fields
        if fm.description.is_empty() {
            return Err(SkillError::ValidationError {
                reason: "description is required".to_string(),
            });
        }

        // Parse allowed-tools from space-delimited string
        let allowed_tools = fm
            .allowed_tools
            .map(|s| s.split_whitespace().map(|t| t.to_string()).collect());

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
        let end_idx = rest
            .find("\n---")
            .ok_or_else(|| SkillError::ValidationError {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SkillScope, SkillSource};
    use std::path::PathBuf;

    fn test_source() -> SkillSource {
        SkillSource {
            agent: Some("crucible".to_string()),
            scope: SkillScope::Personal,
            path: PathBuf::from("/test/my-skill/SKILL.md"),
            content_hash: "deadbeef".to_string(),
        }
    }

    #[test]
    fn parse_valid_skill() {
        let content = r#"---
name: commit
description: Create well-formatted git commits
---

## Instructions

Always use conventional commits.
"#;
        let parser = SkillParser::new();
        let skill = parser.parse(content, test_source()).unwrap();

        assert_eq!(skill.name, "commit");
        assert_eq!(skill.description, "Create well-formatted git commits");
        assert!(skill.body.contains("Always use conventional commits."));
        assert!(skill.license.is_none());
        assert!(skill.allowed_tools.is_none());
    }

    #[test]
    fn parse_with_optional_fields() {
        let content = r#"---
name: code-review
description: Perform code quality review
license: MIT
compatibility: Works with all agents
allowed-tools: read_file write_file grep
---

Review the code thoroughly.
"#;
        let parser = SkillParser::new();
        let skill = parser.parse(content, test_source()).unwrap();

        assert_eq!(skill.name, "code-review");
        assert_eq!(skill.license.as_deref(), Some("MIT"));
        assert_eq!(
            skill.compatibility.as_deref(),
            Some("Works with all agents")
        );
        assert_eq!(
            skill.allowed_tools,
            Some(vec![
                "read_file".to_string(),
                "write_file".to_string(),
                "grep".to_string()
            ])
        );
    }

    #[test]
    fn parse_extra_metadata_captured() {
        let content = r#"---
name: test-skill
description: A test skill
version: "1.0"
author: tester
---

Body text.
"#;
        let parser = SkillParser::new();
        let skill = parser.parse(content, test_source()).unwrap();

        assert_eq!(skill.metadata.get("version").unwrap().as_str(), Some("1.0"));
        assert_eq!(
            skill.metadata.get("author").unwrap().as_str(),
            Some("tester")
        );
    }

    #[test]
    fn rejects_missing_frontmatter() {
        let content = "# No frontmatter here\n\nJust body text.";
        let parser = SkillParser::new();
        let err = parser.parse(content, test_source()).unwrap_err();

        match err {
            SkillError::ValidationError { reason } => {
                assert!(reason.contains("frontmatter"));
            }
            other => panic!("Expected ValidationError, got: {other}"),
        }
    }

    #[test]
    fn rejects_unclosed_frontmatter() {
        let content = "---\nname: broken\ndescription: no closing\n\nBody text.";
        let parser = SkillParser::new();
        let err = parser.parse(content, test_source()).unwrap_err();

        match err {
            SkillError::ValidationError { reason } => {
                assert!(reason.contains("closing ---"));
            }
            other => panic!("Expected ValidationError, got: {other}"),
        }
    }

    #[test]
    fn rejects_empty_description() {
        let content = "---\nname: empty-desc\ndescription: \"\"\n---\n\nBody.";
        let parser = SkillParser::new();
        let err = parser.parse(content, test_source()).unwrap_err();

        match err {
            SkillError::ValidationError { reason } => {
                assert!(reason.contains("description"));
            }
            other => panic!("Expected ValidationError, got: {other}"),
        }
    }

    #[test]
    fn rejects_invalid_yaml() {
        let content = "---\nname: [invalid yaml\n---\n\nBody.";
        let parser = SkillParser::new();
        let err = parser.parse(content, test_source()).unwrap_err();

        assert!(matches!(err, SkillError::ParseError { .. }));
    }

    #[test]
    fn body_is_trimmed() {
        let content =
            "---\nname: trim-test\ndescription: A test\n---\n\n   Body with spaces.   \n\n";
        let parser = SkillParser::new();
        let skill = parser.parse(content, test_source()).unwrap();

        assert_eq!(skill.body, "Body with spaces.");
    }

    #[test]
    fn source_preserved_in_parsed_skill() {
        let content = "---\nname: src-test\ndescription: Test source\n---\n\nBody.";
        let source = SkillSource {
            agent: Some("claude".to_string()),
            scope: SkillScope::Workspace,
            path: PathBuf::from("/workspace/.claude/skills/src-test/SKILL.md"),
            content_hash: "abc123".to_string(),
        };

        let parser = SkillParser::new();
        let skill = parser.parse(content, source).unwrap();

        assert_eq!(skill.source.agent.as_deref(), Some("claude"));
        assert_eq!(skill.source.scope, SkillScope::Workspace);
        assert_eq!(skill.source.content_hash, "abc123");
    }

    #[test]
    fn default_trait_works() {
        let parser = SkillParser::default();
        let content = "---\nname: default-test\ndescription: Works\n---\n\nBody.";
        assert!(parser.parse(content, test_source()).is_ok());
    }
}
