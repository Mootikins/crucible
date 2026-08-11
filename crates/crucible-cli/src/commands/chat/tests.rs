use crate::commands::chat::*;

#[test]
fn test_parse_env_overrides_empty() {
    let result = parse_env_overrides(&[]);
    assert!(result.is_empty());
}

#[test]
fn test_parse_env_overrides_single() {
    let result = parse_env_overrides(&["FOO=bar".to_string()]);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get("FOO"), Some(&"bar".to_string()));
}

#[test]
fn test_parse_env_overrides_multiple() {
    let result = parse_env_overrides(&["FOO=bar".to_string(), "BAZ=qux".to_string()]);
    assert_eq!(result.len(), 2);
    assert_eq!(result.get("FOO"), Some(&"bar".to_string()));
    assert_eq!(result.get("BAZ"), Some(&"qux".to_string()));
}

#[test]
fn test_parse_env_overrides_with_equals_in_value() {
    let result = parse_env_overrides(&["KEY=value=with=equals".to_string()]);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get("KEY"), Some(&"value=with=equals".to_string()));
}

#[test]
fn test_parse_env_overrides_empty_key_ignored() {
    let result = parse_env_overrides(&["=value".to_string()]);
    assert!(result.is_empty());
}

#[test]
fn test_parse_env_overrides_no_equals_ignored() {
    let result = parse_env_overrides(&["INVALID".to_string()]);
    assert!(result.is_empty());
}

#[test]
fn test_parse_env_overrides_mixed_valid_invalid() {
    let result = parse_env_overrides(&[
        "VALID=value".to_string(),
        "INVALID".to_string(),
        "=nokey".to_string(),
        "ALSO_VALID=123".to_string(),
    ]);
    assert_eq!(result.len(), 2);
    assert_eq!(result.get("VALID"), Some(&"value".to_string()));
    assert_eq!(result.get("ALSO_VALID"), Some(&"123".to_string()));
}

#[test]
fn test_parse_env_overrides_empty_value() {
    let result = parse_env_overrides(&["KEY=".to_string()]);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get("KEY"), Some(&"".to_string()));
}

// --- `--no-context` / `--context-size` -> session precognition state ---
//
// Both flags live on the shared `Commands::Chat` variant, so interactive
// chat must honour them exactly as `cru chat -q` does. The asymmetry
// below (absent flag sends nothing) is the point of these tests.

use crate::tui::oil::commands::{SetEffect, SetRpcAction};

#[test]
fn no_context_flag_disables_precognition() {
    assert_eq!(
        precognition_flag_actions(true, None),
        vec![SetRpcAction::SetPrecognition(false)]
    );
}

#[test]
fn context_size_flag_sets_the_result_count() {
    assert_eq!(
        precognition_flag_actions(false, Some(3)),
        vec![SetRpcAction::SetPrecognitionResults(3)]
    );
}

#[test]
fn absent_context_flags_send_no_precognition_rpc() {
    // Sending `set_precognition(true)` here would clobber a user's
    // `:set noprecognition`, and a `set_precognition_results` default
    // would clobber the daemon's own default.
    assert!(precognition_flag_actions(false, None).is_empty());
}

#[test]
fn no_context_flag_wins_over_context_size() {
    // A disabled searcher has no result count to set. This was
    // `run_oneshot_chat`'s `if no_context { .. } else if ..` before both
    // paths shared this function; interactive inherits it.
    assert_eq!(
        precognition_flag_actions(true, Some(9)),
        vec![SetRpcAction::SetPrecognition(false)]
    );
}

#[test]
fn interactive_initial_sets_append_the_context_flags_after_set_overrides() {
    let sets = build_initial_sets(&["precognition=on".to_string()], true, None)
        .expect("valid --set input");
    assert_eq!(
        sets,
        vec![
            SetEffect::DaemonRpc(SetRpcAction::SetPrecognition(true)),
            SetEffect::DaemonRpc(SetRpcAction::SetPrecognition(false)),
        ],
        "flags must be applied last so `--no-context` wins, as in oneshot"
    );
}

#[test]
fn interactive_initial_sets_reject_invalid_set_overrides() {
    let err = build_initial_sets(&["definitely_not_a_key=1".to_string()], false, None)
        .expect_err("unknown key must not be silently dropped");
    assert!(err.contains("definitely_not_a_key"), "got {err}");
}
