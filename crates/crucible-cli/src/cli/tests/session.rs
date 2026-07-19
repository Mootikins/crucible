use super::parse;
use crate::cli::*;
use test_case::test_case;

#[test]
fn test_session_list_parses() {
    assert!(matches!(
        parse(&["cru", "session", "list"]),
        Commands::Session(SessionCommands::List { .. })
    ));
}

#[test]
fn test_session_list_with_options() {
    let Commands::Session(SessionCommands::List {
        limit,
        session_type,
        format,
        ..
    }) = parse(&["cru", "session", "list", "-n", "10", "-t", "chat"])
    else {
        panic!("Expected Session List command");
    };
    assert_eq!(limit, 10);
    assert_eq!(session_type, Some("chat".to_string()));
    assert_eq!(format, "text");
}

#[test]
fn test_session_show_parses() {
    let Commands::Session(SessionCommands::Show { id, .. }) =
        parse(&["cru", "session", "show", "chat-20260104-1530-a1b2"])
    else {
        panic!("Expected Session Show command");
    };
    assert_eq!(id, Some("chat-20260104-1530-a1b2".to_string()));
}

#[test]
fn test_session_open_parses() {
    let Commands::Session(SessionCommands::Open { id }) =
        parse(&["cru", "session", "open", "chat-20260104-1530-a1b2"])
    else {
        panic!("Expected Session Open command");
    };
    assert_eq!(id, Some("chat-20260104-1530-a1b2".to_string()));
}

#[test_case(&["cru", "session", "resume", "chat-20260104-1530-a1b2"], "text" ; "default format")]
#[test_case(&["cru", "session", "resume", "chat-20260104-1530-a1b2", "-f", "json"], "json" ; "json format")]
fn test_session_resume_parses(args: &[&str], expected_format: &str) {
    let Commands::Session(SessionCommands::Resume { session_id, format }) = parse(args) else {
        panic!("Expected Session Resume command");
    };
    assert_eq!(session_id, Some("chat-20260104-1530-a1b2".to_string()));
    assert_eq!(format, expected_format);
}

#[test]
fn test_session_export_parses() {
    let Commands::Session(SessionCommands::Export { id, timestamps, .. }) = parse(&[
        "cru",
        "session",
        "export",
        "chat-20260104-1530-a1b2",
        "--timestamps",
    ]) else {
        panic!("Expected Session Export command");
    };
    assert_eq!(id, Some("chat-20260104-1530-a1b2".to_string()));
    assert!(timestamps);
}

#[test]
fn test_session_search_parses() {
    let Commands::Session(SessionCommands::Search { query, .. }) =
        parse(&["cru", "session", "search", "rust"])
    else {
        panic!("Expected Session Search command");
    };
    assert_eq!(query, "rust");
}

#[test]
fn test_session_cleanup_parses() {
    let Commands::Session(SessionCommands::Cleanup {
        older_than,
        dry_run,
    }) = parse(&[
        "cru",
        "session",
        "cleanup",
        "--older-than",
        "60",
        "--dry-run",
    ])
    else {
        panic!("Expected Session Cleanup command");
    };
    assert_eq!(older_than, 60);
    assert!(dry_run);
}

#[test]
fn test_session_reindex_parses() {
    assert!(matches!(
        parse(&["cru", "session", "reindex"]),
        Commands::Session(SessionCommands::Reindex { force: false })
    ));
}

#[test]
fn test_session_list_with_state_parses() {
    let Commands::Session(SessionCommands::List { state, .. }) =
        parse(&["cru", "session", "list", "--state", "active"])
    else {
        panic!("Expected Session List command");
    };
    assert_eq!(state, Some("active".to_string()));
}

#[test]
fn test_session_list_with_all_flag_parses() {
    let Commands::Session(SessionCommands::List { all, .. }) =
        parse(&["cru", "session", "list", "--all"])
    else {
        panic!("Expected Session List command with --all flag");
    };
    assert!(all);
}

#[test]
fn test_session_list_accepts_agent_type() {
    let Commands::Session(SessionCommands::List { session_type, .. }) =
        parse(&["cru", "session", "list", "-t", "agent"])
    else {
        panic!("Expected Session List command");
    };
    assert_eq!(session_type, Some("agent".to_string()));
}

#[test]
fn test_session_create_parses() {
    let Commands::Session(SessionCommands::Create {
        session_type,
        agent,
        recording_mode,
        quiet,
        format,
        title,
        workspace,
        permissions,
    }) = parse(&["cru", "session", "create"])
    else {
        panic!("Expected Session Create command");
    };
    assert_eq!(session_type, "chat");
    assert_eq!(agent, None);
    assert_eq!(recording_mode, None);
    assert!(!quiet);
    assert_eq!(format, "text");
    assert_eq!(title, None);
    assert_eq!(workspace, None);
    assert_eq!(permissions, None);
}

#[test]
fn test_session_create_with_type_parses() {
    let Commands::Session(SessionCommands::Create {
        session_type,
        agent,
        recording_mode,
        quiet,
        format,
        title,
        workspace,
        ..
    }) = parse(&["cru", "session", "create", "-t", "workflow"])
    else {
        panic!("Expected Session Create command");
    };
    assert_eq!(session_type, "workflow");
    assert_eq!(agent, None);
    assert_eq!(recording_mode, None);
    assert!(!quiet);
    assert_eq!(format, "text");
    assert_eq!(title, None);
    assert_eq!(workspace, None);
}

#[test]
fn test_session_create_accepts_mcp_type() {
    // `-t mcp` is accepted for backward compat but canonicalizes to `chat`
    // at the clap-parser level (with an stderr deprecation warning).
    let Commands::Session(SessionCommands::Create { session_type, .. }) =
        parse(&["cru", "session", "create", "-t", "mcp"])
    else {
        panic!("Expected Session Create command");
    };
    assert_eq!(session_type, "chat");
}

#[test]
fn test_session_create_with_quiet_flag() {
    let Commands::Session(SessionCommands::Create { quiet, .. }) =
        parse(&["cru", "session", "create", "-q"])
    else {
        panic!("Expected Session Create command");
    };
    assert!(quiet);
}

#[test]
fn test_session_create_with_format_json() {
    let Commands::Session(SessionCommands::Create { format, .. }) =
        parse(&["cru", "session", "create", "-f", "json"])
    else {
        panic!("Expected Session Create command");
    };
    assert_eq!(format, "json");
}

#[test]
fn test_session_create_with_title() {
    let Commands::Session(SessionCommands::Create { title, .. }) =
        parse(&["cru", "session", "create", "--title", "My Session"])
    else {
        panic!("Expected Session Create command");
    };
    assert_eq!(title, Some("My Session".to_string()));
}

#[test]
fn test_session_create_with_workspace() {
    let Commands::Session(SessionCommands::Create { workspace, .. }) =
        parse(&["cru", "session", "create", "--workspace", "/tmp"])
    else {
        panic!("Expected Session Create command");
    };
    assert_eq!(workspace, Some(std::path::PathBuf::from("/tmp")));
}

#[test_case(&["cru", "session", "pause", "session-123"], "text" ; "default format")]
#[test_case(&["cru", "session", "pause", "session-123", "-f", "json"], "json" ; "json format")]
fn test_session_pause_parses(args: &[&str], expected_format: &str) {
    let Commands::Session(SessionCommands::Pause { session_id, format }) = parse(args) else {
        panic!("Expected Session Pause command");
    };
    assert_eq!(session_id, Some("session-123".to_string()));
    assert_eq!(format, expected_format);
}

#[test]
fn test_session_unpause_parses() {
    let Commands::Session(SessionCommands::Unpause { session_id }) =
        parse(&["cru", "session", "unpause", "session-123"])
    else {
        panic!("Expected Session Unpause command");
    };
    assert_eq!(session_id, Some("session-123".to_string()));
}

#[test_case(&["cru", "session", "end", "session-123"], "text" ; "default format")]
#[test_case(&["cru", "session", "end", "session-123", "-f", "json"], "json" ; "json format")]
fn test_session_end_parses(args: &[&str], expected_format: &str) {
    let Commands::Session(SessionCommands::End { session_id, format }) = parse(args) else {
        panic!("Expected Session End command");
    };
    assert_eq!(session_id, Some("session-123".to_string()));
    assert_eq!(format, expected_format);
}

#[test]
fn test_session_send_positional_id_and_message() {
    let Commands::Session(SessionCommands::Send {
        session_id_pos,
        message,
        session_id_flag,
        raw,
        permissions,
    }) = parse(&["cru", "session", "send", "chat-123", "hello"])
    else {
        panic!("Expected Session Send command");
    };
    assert_eq!(session_id_pos, Some("chat-123".to_string()));
    assert_eq!(message, Some("hello".to_string()));
    assert_eq!(session_id_flag, None);
    assert!(!raw);
    assert_eq!(permissions, None);
}

#[test]
fn test_session_send_deprecated_session_flag() {
    let Commands::Session(SessionCommands::Send {
        session_id_pos,
        message,
        session_id_flag,
        raw,
        permissions,
    }) = parse(&["cru", "session", "send", "--session", "chat-123", "hello"])
    else {
        panic!("Expected Session Send command");
    };
    assert_eq!(session_id_pos, Some("hello".to_string()));
    assert_eq!(message, None);
    assert_eq!(session_id_flag, Some("chat-123".to_string()));
    assert!(!raw);
    assert_eq!(permissions, None);
}

#[test]
fn test_session_send_single_positional_message_only() {
    let Commands::Session(SessionCommands::Send {
        session_id_pos,
        message,
        session_id_flag,
        raw,
        ..
    }) = parse(&["cru", "session", "send", "hello"])
    else {
        panic!("Expected Session Send command");
    };
    assert_eq!(session_id_pos, Some("hello".to_string()));
    assert_eq!(message, None);
    assert_eq!(session_id_flag, None);
    assert!(!raw);
}
