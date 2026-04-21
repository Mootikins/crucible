use super::super::*;

#[test]
fn permission_matcher_matches_bash_pattern() {
    let matcher = PermissionMatcher::new("bash:cargo test *").unwrap();
    assert!(matcher.matches("bash", "cargo test integration"));
}

#[test]
fn permission_matcher_rejects_non_matching_bash_pattern() {
    let matcher = PermissionMatcher::new("bash:cargo test *").unwrap();
    assert!(!matcher.matches("bash", "npm run test"));
}

#[test]
fn permission_matcher_matches_bash_wildcard() {
    let matcher = PermissionMatcher::new("bash:*").unwrap();
    assert!(matcher.matches("bash", "anything"));
}

#[test]
fn permission_matcher_matches_mcp_server_wildcard_pattern() {
    let matcher = PermissionMatcher::new("mcp:github:*").unwrap();
    assert!(matcher.matches("mcp", "github:create_issue"));
}

#[test]
fn permission_matcher_rejects_mcp_wrong_server() {
    let matcher = PermissionMatcher::new("mcp:github:*").unwrap();
    assert!(!matcher.matches("mcp", "gitlab:create_issue"));
}

#[test]
fn permission_matcher_matches_mcp_exact_pattern() {
    let matcher = PermissionMatcher::new("mcp:github:create_issue").unwrap();
    assert!(matcher.matches("mcp", "github:create_issue"));
}

#[test]
fn permission_matcher_matches_edit_glob() {
    let matcher = PermissionMatcher::new("edit:src/**").unwrap();
    assert!(matcher.matches("edit", "src/deep/nested/file.rs"));
}

#[test]
fn permission_matcher_rejects_edit_outside_glob() {
    let matcher = PermissionMatcher::new("edit:src/**").unwrap();
    assert!(!matcher.matches("edit", "test/file.rs"));
}

#[test]
fn permission_matcher_matches_read_any() {
    let matcher = PermissionMatcher::new("read:*").unwrap();
    assert!(matcher.matches("read", "/any/path"));
}

#[test]
fn permission_matcher_invalid_glob_returns_error() {
    let result = PermissionMatcher::new("bash:invalid[");
    assert!(result.is_err());
}

#[test]
fn permission_matcher_rejects_wrong_tool() {
    let matcher = PermissionMatcher::new("bash:cargo test *").unwrap();
    assert!(!matcher.matches("edit", "cargo test"));
}

#[test]
fn permission_matcher_matches_wildcard_tool() {
    let matcher = PermissionMatcher::new("*:*").unwrap();
    assert!(matcher.matches("bash", "anything"));
}

#[test]
fn compiled_permissions_returns_warnings_for_invalid_patterns() {
    let config = PermissionConfig {
        default: PermissionMode::Ask,
        allow: vec!["read:*".to_string(), "bash:invalid[".to_string()],
        deny: vec!["mcp:github:*".to_string(), "invalid".to_string()],
        ask: vec!["edit:src/**".to_string()],
    };

    let (compiled, warnings) = CompiledPermissions::from_config(&config);

    assert_eq!(compiled.default, PermissionMode::Ask);
    assert_eq!(compiled.allow.len(), 1);
    assert_eq!(compiled.deny.len(), 1);
    assert_eq!(compiled.ask.len(), 1);
    assert_eq!(warnings.len(), 2);
}
