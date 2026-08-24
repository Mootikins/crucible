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

// --- `ChatMode::from_flags` and the `--replay` exclusivity ---

fn flags(
    query: Option<&str>,
    record: Option<&str>,
    replay: Option<&str>,
) -> anyhow::Result<ChatMode> {
    ChatMode::from_flags(
        query.map(String::from),
        record.map(std::path::PathBuf::from),
        replay.map(std::path::PathBuf::from),
        2.0,
        Some(500),
    )
}

#[test]
fn no_flags_select_the_interactive_tui_without_a_recording() {
    assert!(matches!(
        flags(None, None, None).unwrap(),
        ChatMode::Interactive { record: None }
    ));
}

#[test]
fn record_flag_selects_the_interactive_tui_with_a_recording_path() {
    match flags(None, Some("out.jsonl"), None).unwrap() {
        ChatMode::Interactive { record: Some(p) } => assert_eq!(p, PathBuf::from("out.jsonl")),
        _ => panic!("expected Interactive with a record path"),
    }
}

#[test]
fn query_selects_oneshot() {
    match flags(Some("hello"), None, None).unwrap() {
        ChatMode::Oneshot { query } => assert_eq!(query, "hello"),
        _ => panic!("expected Oneshot"),
    }
}

#[test]
fn replay_flag_carries_its_path_speed_and_auto_exit() {
    match flags(None, None, Some("in.jsonl")).unwrap() {
        ChatMode::Replay {
            path,
            speed,
            auto_exit,
        } => {
            assert_eq!(path, PathBuf::from("in.jsonl"));
            assert_eq!(speed, 2.0);
            assert_eq!(auto_exit, Some(500));
        }
        _ => panic!("expected Replay"),
    }
}

#[test]
fn replay_rejects_a_query_argument() {
    let err = flags(Some("hello"), None, Some("in.jsonl")).unwrap_err();
    assert!(err.to_string().contains("--replay"), "got {err}");
    assert!(err.to_string().contains("query"), "got {err}");
}

#[test]
fn replay_rejects_record() {
    let err = flags(None, Some("out.jsonl"), Some("in.jsonl")).unwrap_err();
    assert!(err.to_string().contains("--record"), "got {err}");
}

// --- A piped stdin query turns the TUI into a oneshot run ---

#[test]
fn piped_query_turns_interactive_into_oneshot() {
    let mode = ChatMode::Interactive { record: None };
    match apply_piped_query(mode, || Some("from stdin".to_string())).unwrap() {
        ChatMode::Oneshot { query } => assert_eq!(query, "from stdin"),
        _ => panic!("expected Oneshot"),
    }
}

#[test]
fn piped_query_with_record_is_an_error_not_a_silent_drop() {
    // Oneshot has no TUI to record. Before this check the recording path
    // vanished without a word.
    let mode = ChatMode::Interactive {
        record: Some(PathBuf::from("out.jsonl")),
    };
    let err = apply_piped_query(mode, || Some("from stdin".to_string())).unwrap_err();
    assert!(err.to_string().contains("--record"), "got {err}");
}

#[test]
fn no_piped_query_leaves_the_mode_alone() {
    let mode = ChatMode::Interactive {
        record: Some(PathBuf::from("out.jsonl")),
    };
    assert!(matches!(
        apply_piped_query(mode, || None).unwrap(),
        ChatMode::Interactive { record: Some(_) }
    ));
}

#[test]
fn explicit_query_ignores_a_piped_query() {
    let mode = ChatMode::Oneshot {
        query: "explicit".to_string(),
    };
    match apply_piped_query(mode, || Some("piped".to_string())).unwrap() {
        ChatMode::Oneshot { query } => assert_eq!(query, "explicit"),
        _ => panic!("expected Oneshot"),
    }
}

#[test]
fn explicit_query_does_not_read_stdin() {
    // `cru chat "a"` under a shell `while read` loop must leave the rest
    // of the pipe to the next iteration. A supervisor that holds the
    // stdin pipe open must not block the run.
    let mode = ChatMode::Oneshot {
        query: "explicit".to_string(),
    };
    let result = apply_piped_query(mode, || panic!("stdin was read"));
    match result.unwrap() {
        ChatMode::Oneshot { query } => assert_eq!(query, "explicit"),
        _ => panic!("expected Oneshot"),
    }
}

// --- `--plan` names the mode both chat paths start in ---

#[test]
fn plan_flag_selects_plan_mode_and_its_absence_normal() {
    assert_eq!(initial_mode(true), "plan");
    assert_eq!(initial_mode(false), "normal");
}

#[test]
fn oneshot_applies_plan_mode_only_when_the_flag_is_given() {
    // Without `--plan` a resumed session keeps the mode it has.
    assert_eq!(oneshot_mode_override(true), Some("plan"));
    assert_eq!(oneshot_mode_override(false), None);
}
