//! Integration tests for the init command
use tempfile::TempDir;

#[tokio::test]
async fn test_init_creates_config_with_provider() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    // Run init (non-interactive mode with defaults)
    crucible_cli::commands::init::execute(
        Some(path.clone()),
        false,
        true,
        &temp_dir.path().join("global-config.toml"),
    )
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

    crucible_cli::commands::init::execute(
        Some(path.clone()),
        false,
        true,
        &temp_dir.path().join("global-config.toml"),
    )
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
    crucible_cli::commands::init::execute(
        Some(path.clone()),
        false,
        true,
        &temp_dir.path().join("global-config.toml"),
    )
    .await
    .unwrap();

    // Second init without force should succeed (idempotent — prints "already exists", returns Ok)
    let result = crucible_cli::commands::init::execute(
        Some(path.clone()),
        false,
        true,
        &temp_dir.path().join("global-config.toml"),
    )
    .await;
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
    crucible_cli::commands::init::execute(
        Some(path.clone()),
        false,
        true,
        &temp_dir.path().join("global-config.toml"),
    )
    .await
    .unwrap();

    // Create a marker file to verify directory is recreated
    let marker = path.join(".crucible/marker.txt");
    std::fs::write(&marker, "test").unwrap();
    assert!(marker.exists());

    // Force reinit should succeed and remove marker
    crucible_cli::commands::init::execute(
        Some(path.clone()),
        true,
        true,
        &temp_dir.path().join("global-config.toml"),
    )
    .await
    .unwrap();

    assert!(
        !marker.exists(),
        "marker should be removed after force reinit"
    );
}

/// `cru init` writes to the user's *global* config. A test that does not
/// isolate that path rewrites the developer's real `~/.config/crucible/config.toml`
/// — which is exactly what happened while this feature was being built: a run
/// of this suite replaced a working config's `kiln_path` and `default_kiln`
/// with deleted tempdirs and left 13 junk `[kilns]` entries behind.
///
/// The path is a parameter rather than a global lookup so this cannot recur
/// silently; this test pins the guarantee.
#[tokio::test]
async fn init_writes_only_to_the_config_path_it_was_given() {
    let temp_dir = TempDir::new().unwrap();
    let kiln = temp_dir.path().join("kiln");
    std::fs::create_dir_all(&kiln).unwrap();
    let global = temp_dir.path().join("global-config.toml");

    crucible_cli::commands::init::execute(Some(kiln.clone()), false, true, &global)
        .await
        .unwrap();

    assert!(
        global.exists(),
        "the kiln and provider must be registered in the config path passed in"
    );
    // Assert on the parsed value, not the text: the writer may emit either a
    // `[kilns]` section or an inline table depending on the existing file.
    let contents = std::fs::read_to_string(&global).unwrap();
    let parsed: crucible_core::config::CliAppConfig = toml::from_str(&contents).unwrap();
    assert!(
        !parsed.kilns.is_empty(),
        "the kiln must be registered in the config path passed in, got:\n{contents}"
    );
    assert!(
        parsed.llm.providers.contains_key("ollama"),
        "the provider selection must be registered too, got:\n{contents}"
    );
}
