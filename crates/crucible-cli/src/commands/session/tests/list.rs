use super::super::io::read_session_events;
use super::super::list::list_persisted;
use super::{setup_test_session, test_config, test_sessions_dir};
use crucible_daemon::LogEvent;
use tempfile::TempDir;

#[tokio::test]
async fn test_list_sessions_empty() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(tmp.path());

    // Should not error with empty sessions
    let result = list_persisted(config, 10, None, "table".to_string()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_list_sessions_with_data() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = test_sessions_dir(tmp.path());

    let _id = setup_test_session(&sessions_path).await;

    let config = test_config(tmp.path());

    let result = list_persisted(config, 10, None, "table".to_string()).await;
    assert!(result.is_ok());
}

/// The `(0 messages)` / `(empty)` symptom, pinned at the reader rather than at
/// `list_persisted`'s stdout. `list_persisted` derives its count and title
/// from exactly this call, and it reported both wrong for every real session
/// because the hand-rolled parser it used dropped every wire-format line.
#[tokio::test]
async fn read_session_events_counts_the_messages_a_real_log_holds() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = test_sessions_dir(tmp.path());

    let id = setup_test_session(&sessions_path).await;
    let events = read_session_events(&sessions_path.join(id.as_str()))
        .await
        .unwrap();

    let msg_count = events
        .iter()
        .filter(|e| matches!(e, LogEvent::User { .. } | LogEvent::Assistant { .. }))
        .count();
    assert_eq!(msg_count, 2, "one user turn and one assistant turn");

    let title = events.iter().find_map(|e| match e {
        LogEvent::User { content, .. } => Some(content.clone()),
        _ => None,
    });
    assert_eq!(title.as_deref(), Some("how do I read a file"));
}
