//! Context formatting utilities for skill injection
//!
//! This module provides functions for formatting discovered skills
//! into context-friendly text for LLM system prompts.

use std::collections::HashMap;

use crate::skills::types::ResolvedSkill;

/// Format discovered skills into a tier-1 catalog for an LLM system prompt.
///
/// Emits only name + description per skill (progressive disclosure tier 1).
/// The agent loads a skill's full instructions on demand via the `skill_view`
/// tool, so the catalog stays small and cache-friendly. Returns an empty
/// string when there are no skills, so callers can append unconditionally.
pub fn format_skills_for_context(skills: &HashMap<String, ResolvedSkill>) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut output = String::from("# Available Skills\n\n");
    output.push_str("Load a skill's full instructions with `skill_view(name)` when relevant.\n\n");

    // Sort skills by name for consistent (cache-stable) output
    let mut skill_names: Vec<_> = skills.keys().collect();
    skill_names.sort();

    for name in skill_names {
        if let Some(resolved) = skills.get(name) {
            output.push_str(&format!("## {}\n", name));
            output.push_str(&format!("{}\n\n", resolved.skill.description));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::types::{Skill, SkillScope, SkillSource};
    use chrono::Utc;
    use std::path::PathBuf;

    fn make_skill(name: &str, description: &str) -> ResolvedSkill {
        ResolvedSkill {
            skill: Skill {
                name: name.to_string(),
                description: description.to_string(),
                body: String::new(),
                license: None,
                compatibility: None,
                allowed_tools: None,
                metadata: HashMap::new(),
                source: SkillSource {
                    agent: None,
                    scope: SkillScope::Personal,
                    path: PathBuf::from("/test/SKILL.md"),
                    content_hash: "abc123".to_string(),
                },
                indexed_at: Utc::now(),
            },
            shadowed: vec![],
        }
    }

    #[test]
    fn test_format_empty_skills_is_blank() {
        let skills = HashMap::new();
        let output = format_skills_for_context(&skills);

        // Empty catalog is the empty string so callers can append unconditionally.
        assert!(output.is_empty());
    }

    #[test]
    fn test_format_single_skill() {
        let mut skills = HashMap::new();
        skills.insert(
            "commit".to_string(),
            make_skill("commit", "Create well-formatted git commits"),
        );

        let output = format_skills_for_context(&skills);

        assert!(output.contains("# Available Skills"));
        assert!(output.contains("skill_view(name)"));
        assert!(output.contains("## commit"));
        assert!(output.contains("Create well-formatted git commits"));
    }

    #[test]
    fn test_format_multiple_skills_sorted() {
        let mut skills = HashMap::new();
        skills.insert(
            "zebra".to_string(),
            make_skill("zebra", "A skill starting with z"),
        );
        skills.insert(
            "alpha".to_string(),
            make_skill("alpha", "A skill starting with a"),
        );
        skills.insert(
            "middle".to_string(),
            make_skill("middle", "A skill in the middle"),
        );

        let output = format_skills_for_context(&skills);

        // Verify alphabetical ordering
        let alpha_pos = output.find("## alpha").unwrap();
        let middle_pos = output.find("## middle").unwrap();
        let zebra_pos = output.find("## zebra").unwrap();

        assert!(alpha_pos < middle_pos);
        assert!(middle_pos < zebra_pos);
    }
}
