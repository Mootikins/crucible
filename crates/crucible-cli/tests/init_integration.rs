//! Integration tests for the init command
use tempfile::TempDir;

#[tokio::test]
async fn test_init_creates_config_with_provider() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    // Run init (non-interactive mode with defaults)
    crucible_cli::commands::init::execute(Some(path.clone()), false, false)
        .await
        .unwrap();

    // Verify .crucible directory was created
    let crucible_dir = path.join(".crucible");
    assert!(crucible_dir.exists(), ".crucible directory should exist");

    // Verify config.toml was created
    let config_path = crucible_dir.join("config.toml");
    assert!(config_path.exists(), "config.toml should exist");

    // Verify config contains expected sections
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        content.contains("[chat]"),
        "config should have [chat] section"
    );
    assert!(
        content.contains("provider"),
        "config should have provider setting"
    );
    assert!(
        content.contains("model"),
        "config should have model setting"
    );
}

#[tokio::test]
async fn test_init_creates_required_directories() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    crucible_cli::commands::init::execute(Some(path.clone()), false, false)
        .await
        .unwrap();

    // Verify required subdirectories
    let crucible_dir = path.join(".crucible");
    assert!(
        crucible_dir.join("sessions").exists(),
        "sessions dir should exist"
    );
    assert!(
        crucible_dir.join("plugins").exists(),
        "plugins dir should exist"
    );
}

#[tokio::test]
async fn test_init_is_idempotent_on_existing_kiln() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    // First init should succeed
    crucible_cli::commands::init::execute(Some(path.clone()), false, false)
        .await
        .unwrap();

    // Second init without force should succeed (idempotent — prints "already exists", returns Ok)
    let result = crucible_cli::commands::init::execute(Some(path.clone()), false, false).await;
    assert!(
        result.is_ok(),
        "re-init on existing kiln should be idempotent (Ok)"
    );

    // Config should still be intact
    let config_path = path.join(".crucible/config.toml");
    assert!(
        config_path.exists(),
        "config should still exist after re-init"
    );
}

#[tokio::test]
async fn test_init_force_reinitializes() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    // First init
    crucible_cli::commands::init::execute(Some(path.clone()), false, false)
        .await
        .unwrap();

    // Create a marker file to verify directory is recreated
    let marker = path.join(".crucible/marker.txt");
    std::fs::write(&marker, "test").unwrap();
    assert!(marker.exists());

    // Force reinit should succeed and remove marker
    crucible_cli::commands::init::execute(Some(path.clone()), true, false)
        .await
        .unwrap();

    assert!(
        !marker.exists(),
        "marker should be removed after force reinit"
    );
}
