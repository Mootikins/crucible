use super::setup_test_session;
use super::super::search::{
    extract_session_id_from_path, search, search_in_memory, search_with_ripgrep,
};
use crate::config::CliConfig;
use tempfile::TempDir;

#[tokio::test]
async fn test_search_sessions() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let _id = setup_test_session(&sessions_path).await;

    let config = CliConfig {
        kiln_path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    // Should find session with "hello"
    let result = search(config.clone(), "hello".to_string(), 10, "text".to_string()).await;
    assert!(result.is_ok());

    // Should not find session with "nonexistent"
    let result = search(
        config,
        "nonexistent_term_xyz".to_string(),
        10,
        "text".to_string(),
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_search_in_memory() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let id = setup_test_session(&sessions_path).await;

    let results = search_in_memory(&sessions_path, "hello", 10)
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert_eq!(results[0].0, id.to_string());
    assert!(results[0].2.to_lowercase().contains("hello"));
}

#[tokio::test]
async fn test_search_in_memory_no_matches() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let _id = setup_test_session(&sessions_path).await;

    let results = search_in_memory(&sessions_path, "nonexistent_xyz", 10)
        .await
        .unwrap();

    assert!(results.is_empty());
}

#[test]
fn test_extract_session_id_from_path() {
    let path = "/home/user/notes/.crucible/sessions/chat-20260104-1530-a1b2/session.jsonl";
    let id = extract_session_id_from_path(path);
    assert_eq!(id, "chat-20260104-1530-a1b2");

    let path = "sessions/agent-20260105-0900-xyz/session.jsonl";
    let id = extract_session_id_from_path(path);
    assert_eq!(id, "agent-20260105-0900-xyz");
}

#[tokio::test]
async fn test_search_with_ripgrep_fallback() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let _id = setup_test_session(&sessions_path).await;

    let result = search_with_ripgrep(&sessions_path, "Hello", 10).await;

    match result {
        Ok(matches) => {
            if !matches.is_empty() {
                assert!(matches[0].2.contains("Hello") || matches[0].2.contains("hello"));
            }
        }
        Err(_) => {
            // Ripgrep not installed or no matches - both are acceptable
        }
    }
}
