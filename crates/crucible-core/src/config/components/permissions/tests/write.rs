use super::super::*;

#[test]
fn permission_mode_from_str_valid() {
    assert_eq!(
        "allow".parse::<PermissionMode>().unwrap(),
        PermissionMode::Allow
    );
    assert_eq!(
        "deny".parse::<PermissionMode>().unwrap(),
        PermissionMode::Deny
    );
    assert_eq!(
        "ask".parse::<PermissionMode>().unwrap(),
        PermissionMode::Ask
    );
    assert_eq!(
        "ALLOW".parse::<PermissionMode>().unwrap(),
        PermissionMode::Allow
    );
    assert_eq!(
        "Allow".parse::<PermissionMode>().unwrap(),
        PermissionMode::Allow
    );
}

#[test]
fn permission_mode_from_str_invalid() {
    let err = "bogus".parse::<PermissionMode>().unwrap_err();
    assert!(err.contains("Invalid permission mode"));
    assert!(err.contains("bogus"));
}

#[test]
fn permission_mode_display_roundtrip() {
    assert_eq!(PermissionMode::Allow.to_string(), "allow");
    assert_eq!(PermissionMode::Deny.to_string(), "deny");
    assert_eq!(PermissionMode::Ask.to_string(), "ask");
}

#[test]
fn write_permission_rule_creates_new_file() {
    let dir = tempfile::tempdir().unwrap();
    write_permission_rule(
        PermissionScope::Project,
        "bash:cargo test *",
        Some(dir.path()),
    )
    .unwrap();

    let content = std::fs::read_to_string(dir.path().join("crucible.toml")).unwrap();
    let table: toml::Table = toml::from_str(&content).unwrap();
    let allow = table["permissions"]["allow"].as_array().unwrap();
    assert_eq!(allow.len(), 1);
    assert_eq!(allow[0].as_str().unwrap(), "bash:cargo test *");
}

#[test]
fn write_permission_rule_appends_to_existing_allow_array() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crucible.toml");
    std::fs::write(&path, "[permissions]\nallow = [\"read:*\"]\n").unwrap();

    write_permission_rule(
        PermissionScope::Project,
        "bash:cargo test *",
        Some(dir.path()),
    )
    .unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let table: toml::Table = toml::from_str(&content).unwrap();
    let allow = table["permissions"]["allow"].as_array().unwrap();
    assert_eq!(allow.len(), 2);
    assert_eq!(allow[0].as_str().unwrap(), "read:*");
    assert_eq!(allow[1].as_str().unwrap(), "bash:cargo test *");
}

#[test]
fn write_permission_rule_skips_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    write_permission_rule(
        PermissionScope::Project,
        "bash:cargo test *",
        Some(dir.path()),
    )
    .unwrap();
    write_permission_rule(
        PermissionScope::Project,
        "bash:cargo test *",
        Some(dir.path()),
    )
    .unwrap();

    let content = std::fs::read_to_string(dir.path().join("crucible.toml")).unwrap();
    let table: toml::Table = toml::from_str(&content).unwrap();
    let allow = table["permissions"]["allow"].as_array().unwrap();
    assert_eq!(allow.len(), 1);
}

#[test]
fn write_permission_rule_adds_section_to_existing_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crucible.toml");
    std::fs::write(&path, "[llm]\nmodel = \"gpt-4\"\n").unwrap();

    write_permission_rule(PermissionScope::Project, "read:*", Some(dir.path())).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let table: toml::Table = toml::from_str(&content).unwrap();
    assert_eq!(table["llm"]["model"].as_str().unwrap(), "gpt-4");
    let allow = table["permissions"]["allow"].as_array().unwrap();
    assert_eq!(allow.len(), 1);
    assert_eq!(allow[0].as_str().unwrap(), "read:*");
}

#[test]
fn write_permission_rule_user_scope_uses_config_toml() {
    let dir = tempfile::tempdir().unwrap();
    write_permission_rule(PermissionScope::User, "bash:*", Some(dir.path())).unwrap();

    let path = dir.path().join("config.toml");
    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    let table: toml::Table = toml::from_str(&content).unwrap();
    let allow = table["permissions"]["allow"].as_array().unwrap();
    assert_eq!(allow[0].as_str().unwrap(), "bash:*");
}
