use crucible_skills::discovery::{FolderDiscovery, SearchPath};
use crucible_skills::SkillScope;
use std::fs;
use tempfile::TempDir;

fn create_test_skill(dir: &std::path::Path, name: &str) {
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();
    let content = format!(
        r#"---
name: {name}
description: Test skill {name}
---

# {name}

Instructions here.
"#,
        name = name
    );
    fs::write(skill_dir.join("SKILL.md"), content).unwrap();
}

#[test]
fn test_discover_skills_in_single_directory() {
    let temp = TempDir::new().unwrap();
    create_test_skill(temp.path(), "skill-one");
    create_test_skill(temp.path(), "skill-two");

    let discovery = FolderDiscovery::new(vec![SearchPath::new(
        temp.path().to_path_buf(),
        SkillScope::Personal,
    )]);

    let discovered = discovery.discover().expect("Should discover skills");

    assert_eq!(discovered.len(), 2);
    assert!(discovered.contains_key("skill-one"));
    assert!(discovered.contains_key("skill-two"));
}

#[test]
fn test_priority_ordering_higher_scope_wins() {
    let personal_dir = TempDir::new().unwrap();
    let kiln_dir = TempDir::new().unwrap();

    // Same skill name in both directories
    create_test_skill(personal_dir.path(), "shared-skill");
    create_test_skill(kiln_dir.path(), "shared-skill");

    let discovery = FolderDiscovery::new(vec![
        SearchPath::new(personal_dir.path().to_path_buf(), SkillScope::Personal),
        SearchPath::new(kiln_dir.path().to_path_buf(), SkillScope::Kiln),
    ]);

    let discovered = discovery.discover().expect("Should discover");

    let resolved = discovered.get("shared-skill").expect("Should find skill");
    assert_eq!(
        resolved.skill.source.scope,
        SkillScope::Kiln,
        "Kiln should win"
    );
    assert_eq!(resolved.shadowed.len(), 1, "Should shadow personal");
}
