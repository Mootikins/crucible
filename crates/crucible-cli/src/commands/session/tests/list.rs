use super::setup_test_session;
use super::super::list::list_persisted;
use crate::config::CliConfig;
use tempfile::TempDir;

#[tokio::test]
async fn test_list_sessions_empty() {
    let tmp = TempDir::new().unwrap();
    let config = CliConfig {
        kiln_path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    // Should not error with empty sessions
    let result = list_persisted(config, 10, None, "table".to_string()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_list_sessions_with_data() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let _id = setup_test_session(&sessions_path).await;

    let config = CliConfig {
        kiln_path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let result = list_persisted(config, 10, None, "table".to_string()).await;
    assert!(result.is_ok());
}
