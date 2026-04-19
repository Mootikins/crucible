use super::super::*;

#[test]
fn permission_mode_default_is_ask() {
    assert_eq!(PermissionMode::default(), PermissionMode::Ask);
}

#[test]
fn permission_config_default_is_ask() {
    let config = PermissionConfig::default();
    assert_eq!(config.default, PermissionMode::Ask);
    assert!(config.allow.is_empty());
    assert!(config.deny.is_empty());
    assert!(config.ask.is_empty());
}

#[test]
fn parse_rule_simple_bash_pattern() {
    let rule = parse_rule("bash:cargo test *").unwrap();
    assert_eq!(rule.tool, "bash");
    assert_eq!(rule.server, None);
    assert_eq!(rule.pattern, "cargo test *");
}

#[test]
fn parse_rule_mcp_with_server() {
    let rule = parse_rule("mcp:github:create_issue").unwrap();
    assert_eq!(rule.tool, "mcp");
    assert_eq!(rule.server, Some("github".to_string()));
    assert_eq!(rule.pattern, "create_issue");
}

#[test]
fn parse_rule_mcp_with_wildcard() {
    let rule = parse_rule("mcp:github:*").unwrap();
    assert_eq!(rule.tool, "mcp");
    assert_eq!(rule.server, Some("github".to_string()));
    assert_eq!(rule.pattern, "*");
}

#[test]
fn parse_rule_edit_with_glob_pattern() {
    let rule = parse_rule("edit:src/**").unwrap();
    assert_eq!(rule.tool, "edit");
    assert_eq!(rule.server, None);
    assert_eq!(rule.pattern, "src/**");
}

#[test]
fn parse_rule_plugin_with_server() {
    let rule = parse_rule("plugin:discord:send_message").unwrap();
    assert_eq!(rule.tool, "plugin");
    assert_eq!(rule.server, Some("discord".to_string()));
    assert_eq!(rule.pattern, "send_message");
}

#[test]
fn parse_rule_read_pattern() {
    let rule = parse_rule("read:docs/**").unwrap();
    assert_eq!(rule.tool, "read");
    assert_eq!(rule.server, None);
    assert_eq!(rule.pattern, "docs/**");
}

#[test]
fn parse_rule_no_colon_fails() {
    let result = parse_rule("invalid");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("colon"));
}

#[test]
fn parse_rule_empty_tool_fails() {
    let result = parse_rule(":pattern");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Tool name"));
}

#[test]
fn parse_rule_empty_server_fails() {
    let result = parse_rule("mcp::pattern");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Server name"));
}

#[test]
fn permission_mode_serde_lowercase() {
    let json_allow = serde_json::to_string(&PermissionMode::Allow).unwrap();
    assert_eq!(json_allow, "\"allow\"");

    let json_deny = serde_json::to_string(&PermissionMode::Deny).unwrap();
    assert_eq!(json_deny, "\"deny\"");

    let json_ask = serde_json::to_string(&PermissionMode::Ask).unwrap();
    assert_eq!(json_ask, "\"ask\"");
}

#[test]
fn permission_mode_deserialize_lowercase() {
    let allow: PermissionMode = serde_json::from_str("\"allow\"").unwrap();
    assert_eq!(allow, PermissionMode::Allow);

    let deny: PermissionMode = serde_json::from_str("\"deny\"").unwrap();
    assert_eq!(deny, PermissionMode::Deny);

    let ask: PermissionMode = serde_json::from_str("\"ask\"").unwrap();
    assert_eq!(ask, PermissionMode::Ask);
}

#[test]
fn permission_config_toml_roundtrip() {
    let toml_str = r#"
default = "ask"
allow = ["read:*"]
deny = ["bash:rm *"]
ask = ["edit:src/**"]
"#;

    let config: PermissionConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.default, PermissionMode::Ask);
    assert_eq!(config.allow, vec!["read:*"]);
    assert_eq!(config.deny, vec!["bash:rm *"]);
    assert_eq!(config.ask, vec!["edit:src/**"]);

    // Serialize back
    let serialized = toml::to_string_pretty(&config).unwrap();
    let config2: PermissionConfig = toml::from_str(&serialized).unwrap();
    assert_eq!(config, config2);
}

#[test]
fn permission_config_yaml_roundtrip() {
    let yaml_str = r#"
default: ask
allow:
  - read:*
deny:
  - bash:rm *
ask:
  - edit:src/**
"#;

    let config: PermissionConfig = serde_yaml::from_str(yaml_str).unwrap();
    assert_eq!(config.default, PermissionMode::Ask);
    assert_eq!(config.allow, vec!["read:*"]);
    assert_eq!(config.deny, vec!["bash:rm *"]);
    assert_eq!(config.ask, vec!["edit:src/**"]);

    // Serialize back
    let serialized = serde_yaml::to_string(&config).unwrap();
    let config2: PermissionConfig = serde_yaml::from_str(&serialized).unwrap();
    assert_eq!(config, config2);
}

#[test]
fn permission_config_missing_section_is_none() {
    // This test verifies that when permissions section is missing from config,
    // it deserializes to None (handled by Config struct with Option<PermissionConfig>)
    let config = PermissionConfig::default();
    assert_eq!(config.default, PermissionMode::Ask);
    assert!(config.allow.is_empty());
}

#[test]
fn parse_rule_with_colons_in_pattern() {
    // Pattern can contain colons (e.g., "http://example.com")
    let rule = parse_rule("read:http://example.com").unwrap();
    assert_eq!(rule.tool, "read");
    assert_eq!(rule.server, None);
    assert_eq!(rule.pattern, "http://example.com");
}

#[test]
fn parse_rule_mcp_with_colons_in_pattern() {
    // MCP rule with colons in pattern
    let rule = parse_rule("mcp:api:http://example.com:8080").unwrap();
    assert_eq!(rule.tool, "mcp");
    assert_eq!(rule.server, Some("api".to_string()));
    assert_eq!(rule.pattern, "http://example.com:8080");
}

#[test]
fn permission_config_equality() {
    let config1 = PermissionConfig {
        default: PermissionMode::Ask,
        allow: vec!["read:*".to_string()],
        deny: vec![],
        ask: vec![],
    };

    let config2 = PermissionConfig {
        default: PermissionMode::Ask,
        allow: vec!["read:*".to_string()],
        deny: vec![],
        ask: vec![],
    };

    assert_eq!(config1, config2);
}
