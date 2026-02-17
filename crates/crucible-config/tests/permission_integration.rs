use crucible_config::{PermissionConfig, PermissionDecision, PermissionEngine, PermissionMode};

fn config_with_rules(
    default: PermissionMode,
    allow: &[&str],
    deny: &[&str],
    ask: &[&str],
) -> PermissionConfig {
    PermissionConfig {
        default,
        allow: allow.iter().map(|rule| (*rule).to_string()).collect(),
        deny: deny.iter().map(|rule| (*rule).to_string()).collect(),
        ask: ask.iter().map(|rule| (*rule).to_string()).collect(),
    }
}

#[test]
fn pipeline_auto_allow_via_config_rule_no_modal_needed() {
    let config = config_with_rules(PermissionMode::Ask, &["bash:cargo test*"], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "cargo test", true);
    assert_eq!(decision, PermissionDecision::Allow);
}

#[test]
fn pipeline_unconfigured_tool_returns_ask() {
    let config = config_with_rules(PermissionMode::Ask, &[], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("read", "docs/guide.md", true);
    assert_eq!(decision, PermissionDecision::Ask);
}

#[test]
fn pipeline_hardcoded_denial_overrides_config() {
    let config = config_with_rules(PermissionMode::Allow, &["bash:*"], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "rm -rf /", true);
    assert!(matches!(decision, PermissionDecision::Deny { .. }));
}

#[test]
fn pipeline_deny_rule_in_config_immediately_denies() {
    let config = config_with_rules(PermissionMode::Allow, &["bash:rm *"], &["bash:rm *"], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "rm tmp.log", true);
    assert_eq!(
        decision,
        PermissionDecision::Deny {
            reason: "Matched deny rule".to_string(),
        }
    );
}

#[test]
fn pipeline_non_interactive_ask_becomes_deny() {
    let config = config_with_rules(PermissionMode::Allow, &[], &[], &["bash:git push *"]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "git push origin main", false);
    assert_eq!(
        decision,
        PermissionDecision::Deny {
            reason: "Non-interactive mode: ask rules become deny".to_string(),
        }
    );
}

#[test]
fn pipeline_chained_bash_denies_when_any_subcommand_is_denied() {
    let config = config_with_rules(PermissionMode::Allow, &["bash:*"], &["bash:rm *"], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "cargo test && rm tmp.log", true);
    assert!(matches!(decision, PermissionDecision::Deny { .. }));
}

#[test]
fn pipeline_path_traversal_is_normalized_before_matching() {
    let config = config_with_rules(PermissionMode::Allow, &[], &["read:.env*"], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("read", "src/../.env", true);
    assert!(matches!(decision, PermissionDecision::Deny { .. }));
}

#[test]
fn pipeline_mcp_server_wildcard_matches_tool_name() {
    let config = config_with_rules(PermissionMode::Ask, &["mcp:github:*"], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("mcp", "github:create_issue", true);
    assert_eq!(decision, PermissionDecision::Allow);
}
