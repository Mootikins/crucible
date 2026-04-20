use crate::cli::*;
use clap::Parser;

#[test]
fn test_session_list_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "list"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Session(SessionCommands::List { .. }))
    ));
}

#[test]
fn test_session_list_with_options() {
    let cli = Cli::try_parse_from(["cru", "session", "list", "-n", "10", "-t", "chat"]).unwrap();
    if let Some(Commands::Session(SessionCommands::List {
        limit,
        session_type,
        format,
        ..
    })) = cli.command
    {
        assert_eq!(limit, 10);
        assert_eq!(session_type, Some("chat".to_string()));
        assert_eq!(format, "text");
    } else {
        panic!("Expected Session List command");
    }
}

#[test]
fn test_session_show_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "show", "chat-20260104-1530-a1b2"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Show { id, .. })) = cli.command {
        assert_eq!(id, Some("chat-20260104-1530-a1b2".to_string()));
    } else {
        panic!("Expected Session Show command");
    }
}

#[test]
fn test_session_open_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "open", "chat-20260104-1530-a1b2"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Open { id })) = cli.command {
        assert_eq!(id, Some("chat-20260104-1530-a1b2".to_string()));
    } else {
        panic!("Expected Session Open command");
    }
}

#[test]
fn test_session_resume_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "resume", "chat-20260104-1530-a1b2"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Resume { session_id, format })) = cli.command {
        assert_eq!(session_id, Some("chat-20260104-1530-a1b2".to_string()));
        assert_eq!(format, "text");
    } else {
        panic!("Expected Session Resume command");
    }
}

#[test]
fn test_session_export_parses() {
    let cli = Cli::try_parse_from([
        "cru",
        "session",
        "export",
        "chat-20260104-1530-a1b2",
        "--timestamps",
    ])
    .unwrap();
    if let Some(Commands::Session(SessionCommands::Export { id, timestamps, .. })) = cli.command {
        assert_eq!(id, Some("chat-20260104-1530-a1b2".to_string()));
        assert!(timestamps);
    } else {
        panic!("Expected Session Export command");
    }
}

#[test]
fn test_session_search_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "search", "rust"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Search { query, .. })) = cli.command {
        assert_eq!(query, "rust");
    } else {
        panic!("Expected Session Search command");
    }
}

#[test]
fn test_session_cleanup_parses() {
    let cli = Cli::try_parse_from([
        "cru",
        "session",
        "cleanup",
        "--older-than",
        "60",
        "--dry-run",
    ])
    .unwrap();
    if let Some(Commands::Session(SessionCommands::Cleanup {
        older_than,
        dry_run,
    })) = cli.command
    {
        assert_eq!(older_than, 60);
        assert!(dry_run);
    } else {
        panic!("Expected Session Cleanup command");
    }
}

#[test]
fn test_session_reindex_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "reindex"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Session(SessionCommands::Reindex { force: false }))
    ));
}

#[test]
fn test_session_list_with_state_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "list", "--state", "active"]).unwrap();
    if let Some(Commands::Session(SessionCommands::List { state, .. })) = cli.command {
        assert_eq!(state, Some("active".to_string()));
    } else {
        panic!("Expected Session List command");
    }
}

#[test]
fn test_session_list_with_all_flag_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "list", "--all"]).unwrap();
    if let Some(Commands::Session(SessionCommands::List { all, .. })) = cli.command {
        assert!(all);
    } else {
        panic!("Expected Session List command with --all flag");
    }
}

#[test]
fn test_session_list_accepts_agent_type() {
    let cli = Cli::try_parse_from(["cru", "session", "list", "-t", "agent"]).unwrap();
    if let Some(Commands::Session(SessionCommands::List { session_type, .. })) = cli.command {
        assert_eq!(session_type, Some("agent".to_string()));
    } else {
        panic!("Expected Session List command");
    }
}

#[test]
fn test_session_create_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "create"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Create {
        session_type,
        agent,
        recording_mode,
        quiet,
        format,
        title,
        workspace,
        permissions,
    })) = cli.command
    {
        assert_eq!(session_type, "chat");
        assert_eq!(agent, None);
        assert_eq!(recording_mode, None);
        assert!(!quiet);
        assert_eq!(format, "text");
        assert_eq!(title, None);
        assert_eq!(workspace, None);
        assert_eq!(permissions, None);
    } else {
        panic!("Expected Session Create command");
    }
}

#[test]
fn test_session_create_with_type_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "create", "-t", "workflow"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Create {
        session_type,
        agent,
        recording_mode,
        quiet,
        format,
        title,
        workspace,
        permissions: _,
    })) = cli.command
    {
        assert_eq!(session_type, "workflow");
        assert_eq!(agent, None);
        assert_eq!(recording_mode, None);
        assert!(!quiet);
        assert_eq!(format, "text");
        assert_eq!(title, None);
        assert_eq!(workspace, None);
    } else {
        panic!("Expected Session Create command");
    }
}

#[test]
fn test_session_create_accepts_mcp_type() {
    // `-t mcp` is accepted for backward compat but canonicalizes to `chat`
    // at the clap-parser level (with an stderr deprecation warning).
    let cli = Cli::try_parse_from(["cru", "session", "create", "-t", "mcp"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Create { session_type, .. })) = cli.command {
        assert_eq!(session_type, "chat");
    } else {
        panic!("Expected Session Create command");
    }
}

#[test]
fn test_session_create_with_quiet_flag() {
    let cli = Cli::try_parse_from(["cru", "session", "create", "-q"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Create { quiet, .. })) = cli.command {
        assert!(quiet);
    } else {
        panic!("Expected Session Create command");
    }
}

#[test]
fn test_session_create_with_format_json() {
    let cli = Cli::try_parse_from(["cru", "session", "create", "-f", "json"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Create { format, .. })) = cli.command {
        assert_eq!(format, "json");
    } else {
        panic!("Expected Session Create command");
    }
}

#[test]
fn test_session_create_with_title() {
    let cli = Cli::try_parse_from(["cru", "session", "create", "--title", "My Session"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Create { title, .. })) = cli.command {
        assert_eq!(title, Some("My Session".to_string()));
    } else {
        panic!("Expected Session Create command");
    }
}

#[test]
fn test_session_create_with_workspace() {
    let cli = Cli::try_parse_from(["cru", "session", "create", "--workspace", "/tmp"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Create { workspace, .. })) = cli.command {
        assert_eq!(workspace, Some(std::path::PathBuf::from("/tmp")));
    } else {
        panic!("Expected Session Create command");
    }
}

#[test]
fn test_session_pause_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "pause", "session-123"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Pause { session_id, format })) = cli.command {
        assert_eq!(session_id, Some("session-123".to_string()));
        assert_eq!(format, "text");
    } else {
        panic!("Expected Session Pause command");
    }
}

#[test]
fn test_session_unpause_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "unpause", "session-123"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Unpause { session_id })) = cli.command {
        assert_eq!(session_id, Some("session-123".to_string()));
    } else {
        panic!("Expected Session Unpause command");
    }
}

#[test]
fn test_session_end_parses() {
    let cli = Cli::try_parse_from(["cru", "session", "end", "session-123"]).unwrap();
    if let Some(Commands::Session(SessionCommands::End { session_id, format })) = cli.command {
        assert_eq!(session_id, Some("session-123".to_string()));
        assert_eq!(format, "text");
    } else {
        panic!("Expected Session End command");
    }
}

#[test]
fn test_session_pause_with_format_json() {
    let cli =
        Cli::try_parse_from(["cru", "session", "pause", "session-123", "-f", "json"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Pause { session_id, format })) = cli.command {
        assert_eq!(session_id, Some("session-123".to_string()));
        assert_eq!(format, "json");
    } else {
        panic!("Expected Session Pause command");
    }
}

#[test]
fn test_session_resume_with_format_json() {
    let cli = Cli::try_parse_from([
        "cru",
        "session",
        "resume",
        "chat-20260104-1530-a1b2",
        "-f",
        "json",
    ])
    .unwrap();
    if let Some(Commands::Session(SessionCommands::Resume { session_id, format })) = cli.command {
        assert_eq!(session_id, Some("chat-20260104-1530-a1b2".to_string()));
        assert_eq!(format, "json");
    } else {
        panic!("Expected Session Resume command");
    }
}

#[test]
fn test_session_end_with_format_json() {
    let cli = Cli::try_parse_from(["cru", "session", "end", "session-123", "-f", "json"]).unwrap();
    if let Some(Commands::Session(SessionCommands::End { session_id, format })) = cli.command {
        assert_eq!(session_id, Some("session-123".to_string()));
        assert_eq!(format, "json");
    } else {
        panic!("Expected Session End command");
    }
}

#[test]
fn test_session_send_positional_id_and_message() {
    let cli = Cli::try_parse_from(["cru", "session", "send", "chat-123", "hello"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Send {
        session_id_pos,
        message,
        session_id_flag,
        raw,
        permissions,
    })) = cli.command
    {
        assert_eq!(session_id_pos, Some("chat-123".to_string()));
        assert_eq!(message, Some("hello".to_string()));
        assert_eq!(session_id_flag, None);
        assert!(!raw);
        assert_eq!(permissions, None);
    } else {
        panic!("Expected Session Send command");
    }
}

#[test]
fn test_session_send_deprecated_session_flag() {
    let cli =
        Cli::try_parse_from(["cru", "session", "send", "--session", "chat-123", "hello"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Send {
        session_id_pos,
        message,
        session_id_flag,
        raw,
        permissions,
    })) = cli.command
    {
        assert_eq!(session_id_pos, Some("hello".to_string()));
        assert_eq!(message, None);
        assert_eq!(session_id_flag, Some("chat-123".to_string()));
        assert!(!raw);
        assert_eq!(permissions, None);
    } else {
        panic!("Expected Session Send command");
    }
}

#[test]
fn test_session_send_single_positional_message_only() {
    let cli = Cli::try_parse_from(["cru", "session", "send", "hello"]).unwrap();
    if let Some(Commands::Session(SessionCommands::Send {
        session_id_pos,
        message,
        session_id_flag,
        raw,
        permissions: _,
    })) = cli.command
    {
        assert_eq!(session_id_pos, Some("hello".to_string()));
        assert_eq!(message, None);
        assert_eq!(session_id_flag, None);
        assert!(!raw);
    } else {
        panic!("Expected Session Send command");
    }
}
