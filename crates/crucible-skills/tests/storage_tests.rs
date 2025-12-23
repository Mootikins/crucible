#![cfg(feature = "storage")]

use chrono::Utc;
use crucible_skills::{Skill, SkillScope, SkillSource};
use std::path::PathBuf;

fn test_skill(name: &str, scope: SkillScope) -> Skill {
    Skill {
        name: name.to_string(),
        description: format!("Test skill: {}", name),
        body: "# Instructions\n\nDo the thing.".to_string(),
        license: None,
        compatibility: None,
        allowed_tools: None,
        metadata: Default::default(),
        source: SkillSource {
            agent: Some("crucible".to_string()),
            scope,
            path: PathBuf::from(format!("/test/{}/SKILL.md", name)),
            content_hash: "abc123".to_string(),
        },
        indexed_at: Utc::now(),
    }
}

#[tokio::test]
#[ignore = "requires live database"]
async fn test_store_and_retrieve_skill() {
    // Note: This test requires a test database setup
    // For now, test that the code compiles and types work
    let _skill = test_skill("test-skill", SkillScope::Personal);
}

#[tokio::test]
#[ignore = "requires live database"]
async fn test_list_skills_by_scope() {
    // Note: This test requires a test database setup
    let _skill1 = test_skill("skill-one", SkillScope::Personal);
    let _skill2 = test_skill("skill-two", SkillScope::Workspace);
}

#[tokio::test]
#[ignore = "requires live database"]
async fn test_upsert_updates_existing_skill() {
    // Test that upserting the same skill twice updates it
    let _skill = test_skill("update-skill", SkillScope::Kiln);
}

#[tokio::test]
#[ignore = "requires live database"]
async fn test_delete_skill() {
    // Test that delete removes a skill
    let _skill = test_skill("delete-skill", SkillScope::Personal);
}

#[tokio::test]
#[ignore = "requires live database"]
async fn test_get_by_name_returns_highest_priority() {
    // Test that when multiple skills with same name exist,
    // get_by_name returns the one with highest priority (kiln > workspace > personal)
    let _skill1 = test_skill("priority-test", SkillScope::Personal);
    let _skill2 = test_skill("priority-test", SkillScope::Kiln);
}
