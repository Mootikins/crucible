//! Integration tests for the init command
use tempfile::TempDir;

#[tokio::test]
async fn test_init_creates_kiln_init_lua() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    // Run init (non-interactive mode with defaults)
    crucible_cli::commands::init::execute(Some(path.clone()), false, true)
        .await
        .unwrap();

    // Verify .crucible directory was created
    let crucible_dir = path.join(".crucible");
    assert!(crucible_dir.exists(), ".crucible directory should exist");

    // Verify the kiln-local init.lua was created; the TOML template is gone.
    let init_lua = crucible_dir.join("init.lua");
    assert!(init_lua.exists(), "init.lua should exist");
    assert!(
        !crucible_dir.join("config.toml").exists(),
        "no kiln-local config.toml is generated any more"
    );

    // The scaffold names the provider selection for the reader.
    let content = std::fs::read_to_string(&init_lua).unwrap();
    assert!(
        content.contains("model"),
        "the scaffold should name the model selection"
    );
}

#[tokio::test]
async fn test_init_creates_required_directories() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    crucible_cli::commands::init::execute(Some(path.clone()), false, true)
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
    crucible_cli::commands::init::execute(Some(path.clone()), false, true)
        .await
        .unwrap();

    // Second init without force should succeed (idempotent — prints "already exists", returns Ok)
    let result = crucible_cli::commands::init::execute(Some(path.clone()), false, true).await;
    assert!(
        result.is_ok(),
        "re-init on existing kiln should be idempotent (Ok)"
    );

    // Config should still be intact
    let init_lua = path.join(".crucible/init.lua");
    assert!(
        init_lua.exists(),
        "the kiln-local init.lua should still exist after re-init"
    );
}

#[tokio::test]
async fn test_init_force_reinitializes() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    // First init
    crucible_cli::commands::init::execute(Some(path.clone()), false, true)
        .await
        .unwrap();

    // Create a marker file to verify directory is recreated
    let marker = path.join(".crucible/marker.txt");
    std::fs::write(&marker, "test").unwrap();
    assert!(marker.exists());

    // Force reinit should succeed and remove marker
    crucible_cli::commands::init::execute(Some(path.clone()), true, true)
        .await
        .unwrap();

    assert!(
        !marker.exists(),
        "marker should be removed after force reinit"
    );
}

/// `cru init` on a kiln writes NOTHING to the user's global config.
///
/// A PROJECT init still writes there, and a test that does not isolate that
/// path rewrites the developer's real `~/.config/crucible/config.toml` — which
/// is exactly what happened while this feature was being built: a run of this
/// suite replaced a working config's `kiln_path` and `default_kiln` with
/// deleted tempdirs and left 13 junk `[kilns]` entries behind. The path stays a
/// parameter rather than a global lookup so that cannot recur silently.
///
/// It used to write two things there: a `[kilns]` entry and an
/// `[llm.providers.*]` selection. Both are state the daemon owns now —
/// `<data_home>/kilns.json` and `<data_home>/llm.json` — and both go over RPC.
///
/// The property this test has always been about is the PATH: whatever `init`
/// writes, it writes where it was told. That property now has a sharper form,
/// because the correct number of global-config writes from a kiln init is
/// zero. A file that appears here means a writer came back.
///
/// No daemon is running in this test, which is the other half of what it pins:
/// `init` must still create the kiln, and must not fall back to editing the
/// config when it cannot reach the daemon.
#[tokio::test]
async fn a_kiln_init_writes_nothing_to_the_global_config() {
    let temp_dir = TempDir::new().unwrap();
    let kiln = temp_dir.path().join("kiln");
    std::fs::create_dir_all(&kiln).unwrap();
    let global = temp_dir.path().join("global-config.toml");

    crucible_cli::commands::init::execute(Some(kiln.clone()), false, true)
        .await
        .unwrap();

    assert!(
        kiln.join(".crucible").join("init.lua").is_file(),
        "the kiln itself is still created"
    );
    assert!(
        !global.exists(),
        "a kiln init must not write the user's global config; found:\n{}",
        std::fs::read_to_string(&global).unwrap_or_default()
    );
}
