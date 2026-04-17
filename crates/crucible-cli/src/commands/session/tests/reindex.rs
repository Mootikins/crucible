use super::super::reindex::reindex;
use crate::config::CliConfig;
use crucible_daemon::LogEvent;
use tempfile::TempDir;

#[tokio::test]
async fn test_reindex_no_sessions_dir() {
    let tmp = TempDir::new().unwrap();
    let config = CliConfig {
        kiln_path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let result = reindex(config, false).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_reindex_empty_sessions_dir() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let config = CliConfig {
        kiln_path: tmp.path().to_path_buf(),
        ..Default::default()
    };

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
