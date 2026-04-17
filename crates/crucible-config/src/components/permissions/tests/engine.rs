use super::super::*;
use super::config_with_rules;

#[test]
fn engine_layer_0_denies_destructive_bash() {
    let engine = PermissionEngine::new(None);
    let decision = engine.evaluate("bash", "rm -rf /", true);
    assert!(matches!(decision, PermissionDecision::Deny { .. }));
}

#[test]
fn engine_deny_rule_denies_matching_input() {
    let config = config_with_rules(PermissionMode::Ask, &[], &["bash:rm *"], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "rm something", true);
    assert!(matches!(decision, PermissionDecision::Deny { .. }));
}

#[test]
fn engine_ask_rule_returns_ask() {
    let config = config_with_rules(PermissionMode::Ask, &[], &[], &["bash:git push *"]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "git push origin main", true);
    assert_eq!(decision, PermissionDecision::Ask);
}

#[test]
fn engine_allow_rule_returns_allow() {
    let config = config_with_rules(PermissionMode::Ask, &["bash:cargo test*"], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "cargo test", true);
    assert_eq!(decision, PermissionDecision::Allow);
}

#[test]
fn engine_deny_wins_over_allow() {
    let config = config_with_rules(PermissionMode::Ask, &["bash:rm *"], &["bash:rm *"], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "rm some-file", true);
    assert!(matches!(decision, PermissionDecision::Deny { .. }));
}

#[test]
fn engine_chained_command_denies_on_layer_0_subcommand() {
    let config = config_with_rules(PermissionMode::Ask, &["bash:cargo test*"], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "cargo test && rm -rf /", true);
    assert!(matches!(decision, PermissionDecision::Deny { .. }));
}

#[test]
fn engine_chained_command_allows_only_when_all_subcommands_allowed() {
    let config = config_with_rules(
        PermissionMode::Ask,
        &["bash:cargo test*", "bash:cargo build"],
        &[],
        &[],
    );
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "cargo test && cargo build", true);
    assert_eq!(decision, PermissionDecision::Allow);
}

#[test]
fn engine_chained_command_partial_allow_falls_back_to_ask() {
    let config = config_with_rules(PermissionMode::Ask, &["bash:cargo test*"], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "cargo test && npm run build", true);
    assert_eq!(decision, PermissionDecision::Ask);
}

#[test]
fn engine_file_path_normalization_blocks_traversal_input() {
    let config = config_with_rules(PermissionMode::Ask, &[], &["read:.env*"], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("read", "src/../.env", true);
    assert!(matches!(decision, PermissionDecision::Deny { .. }));
}

#[test]
fn engine_non_interactive_ask_becomes_deny() {
    let config = config_with_rules(PermissionMode::Ask, &[], &[], &["bash:*"]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "ls", false);
    assert_eq!(
        decision,
        PermissionDecision::Deny {
            reason: "Non-interactive mode: ask rules become deny".to_string()
        }
    );
}

#[test]
fn engine_non_interactive_allow_still_allows() {
    let config = config_with_rules(PermissionMode::Ask, &["bash:*"], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "ls", false);
    assert_eq!(decision, PermissionDecision::Allow);
}

#[test]
fn engine_with_no_config_defaults_to_ask() {
    let engine = PermissionEngine::new(None);
    let decision = engine.evaluate("bash", "ls", true);
    assert_eq!(decision, PermissionDecision::Ask);
}

#[test]
fn engine_default_allow_when_no_rules_match() {
    let config = config_with_rules(PermissionMode::Allow, &[], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "unknown", true);
    assert_eq!(decision, PermissionDecision::Allow);
}

#[test]
fn engine_default_deny_when_no_rules_match() {
    let config = config_with_rules(PermissionMode::Deny, &[], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "unknown", true);
    assert!(matches!(decision, PermissionDecision::Deny { .. }));
}

#[test]
fn engine_allows_matching_mcp_server_rule() {
    let config = config_with_rules(PermissionMode::Ask, &["mcp:github:*"], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("mcp", "github:create_issue", true);
    assert_eq!(decision, PermissionDecision::Allow);
}

#[test]
fn engine_file_rules_use_most_restrictive_raw_or_normalized_match() {
    let config = config_with_rules(PermissionMode::Ask, &[], &["write:src/../secret.txt"], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("write", "src/../secret.txt", true);
    assert!(matches!(decision, PermissionDecision::Deny { .. }));
}

// --- Headless permission flow tests ---
// These test the PermissionConfig::default() path (no explicit rules),
// verifying is_interactive=false converts Ask→Deny at the engine level.

#[test]
fn non_interactive_ask_default_becomes_deny() {
    // Default config has default=Ask with no rules.
    // A non-interactive evaluation should convert Ask→Deny.
    let config = PermissionConfig::default();
    let engine = PermissionEngine::new(Some(&config));
    let decision = engine.evaluate("dangerous_tool", "{}", false);
    assert_eq!(
        decision,
        PermissionDecision::Deny {
            reason: "Non-interactive mode: ask rules become deny".to_string()
        }
    );
}

#[test]
fn non_interactive_allow_default_stays_allow() {
    // When default mode is Allow and no deny rules match,
    // non-interactive should still allow (only Ask→Deny conversion).
    let mut config = PermissionConfig::default();
    config.default = PermissionMode::Allow;
    let engine = PermissionEngine::new(Some(&config));
    let decision = engine.evaluate("some_tool", "{}", false);
    assert_eq!(decision, PermissionDecision::Allow);
}

#[test]
fn deny_rules_enforced_even_with_allow_default() {
    // Explicit deny rules should fire even when default=Allow.
    let mut config = PermissionConfig::default();
    config.default = PermissionMode::Allow;
    config.deny = vec!["bash:rm *".to_string()];
    let engine = PermissionEngine::new(Some(&config));
    let decision = engine.evaluate("bash", "rm /tmp/test.txt", false);
    assert!(
        matches!(decision, PermissionDecision::Deny { .. }),
        "deny rule should override allow default, got: {decision:?}"
    );
}

#[test]
fn non_interactive_deny_default_stays_deny() {
    // When default mode is Deny and no allow rules match,
    // non-interactive should deny (no conversion needed, already Deny).
    let mut config = PermissionConfig::default();
    config.default = PermissionMode::Deny;
    let engine = PermissionEngine::new(Some(&config));
    let decision = engine.evaluate("dangerous_tool", "{}", false);
    assert!(matches!(decision, PermissionDecision::Deny { .. }));
}

#[test]
fn interactive_ask_default_returns_ask() {
    // Same default config but interactive=true should return Ask, not Deny.
    let config = PermissionConfig::default();
    let engine = PermissionEngine::new(Some(&config));
    let decision = engine.evaluate("dangerous_tool", "{}", true);
    assert_eq!(decision, PermissionDecision::Ask);
}
