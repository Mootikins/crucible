use crucible_skills::{SkillParser, SkillScope, SkillSource};
use std::path::PathBuf;

#[test]
fn test_parse_valid_skill_md() {
    let content = r#"---
name: git-commit
description: Create well-formatted git commits with conventional messages
license: MIT
---

# Git Commit Skill

Follow these steps to create commits:

1. Stage changes
2. Write message
3. Commit
"#;

    let source = SkillSource {
        agent: Some("crucible".to_string()),
        scope: SkillScope::Personal,
        path: PathBuf::from("/test/skills/git-commit/SKILL.md"),
        content_hash: String::new(),
    };

    let parser = SkillParser::new();
    let skill = parser
        .parse(content, source)
        .expect("Should parse valid skill");

    assert_eq!(skill.name, "git-commit");
    assert_eq!(
        skill.description,
        "Create well-formatted git commits with conventional messages"
    );
    assert_eq!(skill.license, Some("MIT".to_string()));
    assert!(skill.body.contains("# Git Commit Skill"));
}

#[test]
fn test_parse_missing_required_fields() {
    let content = r#"---
name: incomplete
---

Body without description.
"#;

    let source = SkillSource {
        agent: None,
        scope: SkillScope::Workspace,
        path: PathBuf::from("/test/SKILL.md"),
        content_hash: String::new(),
    };

    let parser = SkillParser::new();
    let result = parser.parse(content, source);

    assert!(result.is_err(), "Should fail without description");
}

#[test]
fn test_parse_allowed_tools() {
    let content = r#"---
name: test-skill
description: Test skill with allowed tools
allowed-tools: Bash Read Write
---

Body content.
"#;

    let source = SkillSource {
        agent: None,
        scope: SkillScope::Kiln,
        path: PathBuf::from("/test/SKILL.md"),
        content_hash: String::new(),
    };

    let parser = SkillParser::new();
    let skill = parser.parse(content, source).expect("Should parse");

    assert_eq!(
        skill.allowed_tools,
        Some(vec![
            "Bash".to_string(),
            "Read".to_string(),
            "Write".to_string()
        ])
    );
}

#[test]
fn test_parse_no_frontmatter() {
    let content = "Just body content without frontmatter";

    let source = SkillSource {
        agent: None,
        scope: SkillScope::Personal,
        path: PathBuf::from("/test/SKILL.md"),
        content_hash: String::new(),
    };

    let parser = SkillParser::new();
    let result = parser.parse(content, source);

    assert!(result.is_err(), "Should fail without frontmatter");
}

#[test]
fn test_parse_unclosed_frontmatter() {
    let content = r#"---
name: unclosed
description: Missing closing delimiter

Body content here"#;

    let source = SkillSource {
        agent: None,
        scope: SkillScope::Personal,
        path: PathBuf::from("/test/SKILL.md"),
        content_hash: String::new(),
    };

    let parser = SkillParser::new();
    let result = parser.parse(content, source);

    assert!(result.is_err(), "Should fail with unclosed frontmatter");
}
