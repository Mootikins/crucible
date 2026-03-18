/// Regression tests for demo configuration files
///
/// These tests ensure that the demo config files (used in VHS tape recordings)
/// continue to parse correctly and don't regress due to legacy field removals.
///
/// Background: demo-acp-config.toml previously had legacy [providers] section
/// and chat.provider field which were removed in favor of [llm.providers.<name>].
/// These tests prevent future regressions.
use crucible_config::CliAppConfig;
use std::path::PathBuf;

/// Helper to get the workspace root
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crucible-config should be in crates/")
        .parent()
        .expect("crates/ should be in workspace root")
        .to_path_buf()
}

#[test]
fn demo_config_parses_without_error() {
    let config_path = workspace_root().join("assets/demo-config.toml.example");

    assert!(
        config_path.exists(),
        "demo-config.toml should exist at {}",
        config_path.display()
    );

    let result = CliAppConfig::load(Some(config_path.clone()), None, None);

    assert!(
        result.is_ok(),
        "demo-config.toml should parse without error. Error: {:?}",
        result.err()
    );

    let config = result.unwrap();

    // Verify basic structure - kiln_path should end with "docs"
    assert!(
        config.kiln_path.ends_with("docs"),
        "kiln_path should end with 'docs', got: {:?}",
        config.kiln_path
    );
}

#[test]
fn demo_acp_config_parses_without_error() {
    let config_path = workspace_root().join("assets/demo-acp-config.toml.example");

    assert!(
        config_path.exists(),
        "demo-acp-config.toml should exist at {}",
        config_path.display()
    );

    let result = CliAppConfig::load(Some(config_path.clone()), None, None);

    assert!(
        result.is_ok(),
        "demo-acp-config.toml should parse without error. Error: {:?}",
        result.err()
    );

    let config = result.unwrap();

    // Verify basic structure - kiln_path should end with "docs"
    assert!(
        config.kiln_path.ends_with("docs"),
        "kiln_path should end with 'docs', got: {:?}",
        config.kiln_path
    );
}

#[test]
fn demo_acp_config_has_valid_delegation_config() {
    let config_path = workspace_root().join("assets/demo-acp-config.toml.example");

    let config = CliAppConfig::load(Some(config_path), None, None)
        .expect("demo-acp-config.toml should parse");

    // Verify ACP config exists and has agents
    assert!(
        !config.acp.agents.is_empty(),
        "ACP config should have agent profiles"
    );

    // Verify claude agent exists
    assert!(
        config.acp.agents.contains_key("claude"),
        "ACP config should have 'claude' agent profile"
    );

    let claude_agent = &config.acp.agents["claude"];

    // Verify delegation config exists and is enabled
    assert!(
        claude_agent.delegation.is_some(),
        "claude agent should have delegation config"
    );

    let delegation = claude_agent.delegation.as_ref().unwrap();
    assert!(
        delegation.enabled,
        "claude agent delegation should be enabled"
    );
    assert_eq!(
        delegation.max_depth, 1,
        "claude agent delegation max_depth should be 1"
    );

    // Verify allowed targets
    assert!(
        delegation.allowed_targets.is_some(),
        "claude agent should have allowed_targets"
    );

    let allowed = delegation.allowed_targets.as_ref().unwrap();
    assert!(
        allowed.contains(&"cursor".to_string()),
        "cursor should be in allowed_targets"
    );
    assert!(
        allowed.contains(&"opencode".to_string()),
        "opencode should be in allowed_targets"
    );
}

#[test]
fn demo_acp_config_no_legacy_providers_section() {
    let config_path = workspace_root().join("assets/demo-acp-config.toml.example");

    // This test verifies that the config file doesn't have the legacy [providers] section
    // by ensuring it parses successfully (the parser rejects [providers] with an error)
    let result = CliAppConfig::load(Some(config_path), None, None);

    assert!(
        result.is_ok(),
        "demo-acp-config.toml should not have legacy [providers] section"
    );
}

#[test]
fn demo_acp_config_no_legacy_chat_provider_field() {
    let config_path = workspace_root().join("assets/demo-acp-config.toml.example");

    // This test verifies that the config file doesn't have the legacy chat.provider field
    // by ensuring it parses successfully (the parser rejects chat.provider with an error)
    let result = CliAppConfig::load(Some(config_path), None, None);

    assert!(
        result.is_ok(),
        "demo-acp-config.toml should not have legacy chat.provider field"
    );
}
