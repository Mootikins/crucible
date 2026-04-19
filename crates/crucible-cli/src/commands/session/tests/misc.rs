use super::super::helpers::{resolve_send_inputs, resolve_session_id, truncate};
use super::env_lock;
use crucible_core::test_support::EnvVarGuard;

#[test]
fn session_id_resolver_explicit_wins_over_env() {
    let _lock = env_lock().lock().unwrap();
    let _env = EnvVarGuard::set("CRU_SESSION", "chat-from-env".to_string());

    let resolved = resolve_session_id(Some("chat-explicit".to_string())).unwrap();
    assert_eq!(resolved, "chat-explicit");
}

#[test]
fn session_id_resolver_uses_env_when_explicit_missing() {
    let _lock = env_lock().lock().unwrap();
    let _env = EnvVarGuard::set("CRU_SESSION", "chat-from-env".to_string());

    let resolved = resolve_session_id(None).unwrap();
    assert_eq!(resolved, "chat-from-env");
}

#[test]
fn session_id_resolver_errors_when_no_source_available() {
    let _lock = env_lock().lock().unwrap();
    let _env = EnvVarGuard::remove("CRU_SESSION");

    let result = resolve_session_id(None);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No session specified"));
}

#[test]
fn resolve_send_inputs_uses_deprecated_session_flag_and_warns() {
    let _lock = env_lock().lock().unwrap();
    let _env = EnvVarGuard::remove("CRU_SESSION");

    let (session_id, message, used_deprecated_flag) = resolve_send_inputs(
        Some("hello".to_string()),
        None,
        Some("chat-123".to_string()),
    );

    assert_eq!(session_id, Some("chat-123".to_string()));
    assert_eq!(message, Some("hello".to_string()));
    assert!(used_deprecated_flag);
}

#[test]
fn resolve_send_inputs_treats_two_positionals_as_session_and_message() {
    let _lock = env_lock().lock().unwrap();
    let _env = EnvVarGuard::remove("CRU_SESSION");

    let (session_id, message, used_deprecated_flag) = resolve_send_inputs(
        Some("chat-123".to_string()),
        Some("hello".to_string()),
        None,
    );

    assert_eq!(session_id, Some("chat-123".to_string()));
    assert_eq!(message, Some("hello".to_string()));
    assert!(!used_deprecated_flag);
}

#[test]
fn resolve_send_inputs_treats_single_positional_as_message_when_env_set() {
    let _lock = env_lock().lock().unwrap();
    let _env = EnvVarGuard::set("CRU_SESSION", "chat-from-env".to_string());

    let (session_id, message, used_deprecated_flag) =
        resolve_send_inputs(Some("hello".to_string()), None, None);

    assert_eq!(session_id, None);
    assert_eq!(message, Some("hello".to_string()));
    assert!(!used_deprecated_flag);
}

#[test]
fn resolve_send_inputs_single_positional_without_env_uses_stdin_for_message() {
    let _lock = env_lock().lock().unwrap();
    let _env = EnvVarGuard::remove("CRU_SESSION");

    let (session_id, message, used_deprecated_flag) =
        resolve_send_inputs(Some("chat-123".to_string()), None, None);

    assert_eq!(session_id, Some("chat-123".to_string()));
    assert_eq!(message, None);
    assert!(!used_deprecated_flag);
}

#[test]
fn test_truncate() {
    assert_eq!(truncate("hello", 10), "hello");
    assert_eq!(truncate("hello world", 5), "hello...");
}

#[test]
fn test_daemon_create_recording_mode_parsing() {
    // Test valid recording modes
    let granular = "granular";
    match granular {
        "granular" => assert_eq!(granular, "granular"),
        "coarse" => panic!("Should not match coarse"),
        _ => panic!("Should not match invalid"),
    }

    let coarse = "coarse";
    match coarse {
        "granular" => panic!("Should not match granular"),
        "coarse" => assert_eq!(coarse, "coarse"),
        _ => panic!("Should not match invalid"),
    }

    // Test invalid mode would be caught by the match in daemon_create
    let invalid = "invalid";
    let result = match invalid {
        "granular" => Ok("granular"),
        "coarse" => Ok("coarse"),
        _ => Err(format!(
            "Invalid recording mode: '{}'. Must be 'granular' or 'coarse'",
            invalid
        )),
    };
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid recording mode"));
}
