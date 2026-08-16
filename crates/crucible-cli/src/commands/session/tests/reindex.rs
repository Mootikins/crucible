use super::super::reindex::reindex;
use super::{setup_test_session, test_config, test_sessions_dir};
use crucible_daemon::LogEvent;
use tempfile::TempDir;

/// The daemon retired `session.reindex` — dispatch answers METHOD_NOT_FOUND —
/// so the subcommand must never reach for it. The populated-sessions case is
/// the one that used to fall through to the RPC and could therefore only fail;
/// the two empty-directory cases never got that far, which is why the
/// regression went unnoticed.
#[tokio::test]
async fn reindex_reports_retirement_instead_of_calling_the_retired_rpc() {
    let tmp = TempDir::new().unwrap();
    let sessions_dir = test_sessions_dir(tmp.path());
    let _id = setup_test_session(&sessions_dir).await;

    let config = test_config(tmp.path());

    let result = reindex(config, false).await;
    assert!(result.is_ok(), "reindex should not error: {result:?}");
}

#[tokio::test]
async fn test_reindex_no_sessions_dir() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(tmp.path());

    let result = reindex(config, false).await;
    assert!(result.is_ok());
}

#[test]
fn test_extract_session_content_for_reindex() {
    use crucible_daemon::extract_session_content;

    let events = vec![
        LogEvent::system("You are helpful"),
        LogEvent::user("What is Rust?"),
        LogEvent::assistant("Rust is a systems programming language."),
        LogEvent::user("Tell me more"),
        LogEvent::assistant("It focuses on safety and performance."),
    ];

    let content = extract_session_content("test-sess", &events).unwrap();
    assert_eq!(content.user_messages.len(), 2);
    assert_eq!(content.session_id, "test-sess");

    let record = content.to_note_record(None);
    assert_eq!(record.path, "sessions/test-sess");
    assert!(record.tags.contains(&"session".to_string()));
    assert!(record.embedding.is_none());
}

#[test]
fn test_extract_session_content_skips_empty() {
    use crucible_daemon::extract_session_content;

    let events = vec![
        LogEvent::system("System prompt only"),
        LogEvent::assistant("Unprompted"),
    ];

    assert!(extract_session_content("empty-sess", &events).is_none());
}
