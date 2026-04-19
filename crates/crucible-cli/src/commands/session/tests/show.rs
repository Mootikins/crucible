use super::super::export::export;
use super::super::show::show;
use super::setup_test_session;
use crate::config::CliConfig;
use tempfile::TempDir;

#[tokio::test]
async fn test_show_session() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let id = setup_test_session(&sessions_path).await;

    let config = CliConfig {
        kiln_path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let result = show(config, id.to_string(), "text".to_string()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_show_session_not_found() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let config = CliConfig {
        kiln_path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let result = show(
        config,
        "chat-20260104-1530-a1b2".to_string(),
        "text".to_string(),
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_export_session() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let id = setup_test_session(&sessions_path).await;

    let config = CliConfig {
        kiln_path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let output_path = tmp.path().join("exported.md");
    let result = export(config, id.to_string(), Some(output_path.clone()), false).await;
    assert!(result.is_ok());
    assert!(output_path.exists());

    let content = std::fs::read_to_string(output_path).unwrap();
    assert!(content.contains("## User"));
    assert!(content.contains("Hello, how are you?"));
}
