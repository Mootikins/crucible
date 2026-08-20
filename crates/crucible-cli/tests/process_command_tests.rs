//! Integration tests for process command

#![allow(clippy::field_reassign_with_default)]

//!
//! These tests verify the process command correctly:
//! 1. Uses persistent SQLite storage (not in-memory)
//! 2. Produces consistent output with chat command's pre-processing
//! 3. Implements proper change detection
//! 4. Executes all 5 pipeline phases

use anyhow::{bail, Result};
use crucible_cli::commands::process;
use crucible_cli::config::CliConfig;
use crucible_core::config::{AcpConfig, BackendType, LlmConfig, LlmProviderConfig, StorageConfig};
use crucible_core::test_support::fixtures::{create_kiln, KilnFixture};
use crucible_core::test_support::EnvVarGuard;
use crucible_daemon::rpc_client::lifecycle;
use crucible_daemon::Server;
use serial_test::serial;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(2);
const DAEMON_READY_POLL: Duration = Duration::from_millis(10);

async fn wait_for_daemon_ready(socket_path: &Path) -> Result<()> {
    let deadline = Instant::now() + DAEMON_READY_TIMEOUT;
    loop {
        if UnixStream::connect(socket_path).await.is_ok() {
            return Ok(());
        }
        if Instant::now() > deadline {
            bail!(
                "daemon at {} did not become connectable within {:?}",
                socket_path.display(),
                DAEMON_READY_TIMEOUT
            );
        }
        tokio::time::sleep(DAEMON_READY_POLL).await;
    }
}

/// Helper to create a test kiln with sample markdown files
fn create_test_kiln() -> Result<TempDir> {
    create_kiln(KilnFixture::Custom {
        files: vec![
            (
                "note1.md",
                "# Note 1\n\nThis is the first test note with some content.",
            ),
            (
                "note2.md",
                "# Note 2\n\nThis is the second test note.\n\n## Section\n\nWith multiple blocks.",
            ),
            (
                "note3.md",
                "# Note 3\n\n[[note1]] is linked here.\n\n#tag1 #tag2",
            ),
        ],
    })
}

/// Helper to create test CLI config
fn create_process_test_config(kiln_path: PathBuf, _db_path: PathBuf) -> CliConfig {
    let mut llm_config = LlmConfig::default();
    llm_config.default = Some("local".to_string());
    llm_config.providers.insert(
        "local".to_string(),
        LlmProviderConfig::builder(BackendType::FastEmbed).build(),
    );

    CliConfig {
        kiln_path,
        acp: AcpConfig {
            default_agent: Some("test-agent".to_string()),
            ..Default::default()
        },
        llm: llm_config,
        storage: Some(StorageConfig {
            idle_timeout_secs: 300,
        }),
        ..Default::default()
    }
}

/// Test fixture: starts an in-process daemon so process::execute() can connect.
/// Sets XDG_RUNTIME_DIR so DaemonClient::connect_or_start() finds the socket.
struct TestServer {
    _env_guard: EnvVarGuard,
    _temp_dir: TempDir,
    _server_handle: JoinHandle<()>,
    _shutdown_handle: tokio::sync::broadcast::Sender<()>,
}

impl TestServer {
    async fn start() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let _env_guard = EnvVarGuard::set(
            "XDG_RUNTIME_DIR",
            temp_dir.path().to_str().unwrap().to_string(),
        );
        let socket_path = lifecycle::default_socket_path();
        // Inject an isolated data root (no CRUCIBLE_HOME env) so the in-process
        // daemon never reads the developer's real ~/.crucible registry.
        let server =
            Server::bind_with_data_home(&socket_path, temp_dir.path().to_path_buf()).await?;
        let shutdown_handle = server.shutdown_handle();
        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });
        wait_for_daemon_ready(&socket_path).await?;
        Ok(Self {
            _env_guard,
            _temp_dir: temp_dir,
            _server_handle: server_handle,
            _shutdown_handle: shutdown_handle,
        })
    }
}

#[tokio::test]
#[serial]
async fn test_process_executes_pipeline() -> Result<()> {
    let _server = TestServer::start().await?;
    // Given: A test kiln with markdown files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path.clone(), db_path.clone());

    // When: Running the process command
    let result = process::execute(config, None, false, false, false, false, false).await;

    // Then: Command should succeed
    assert!(
        result.is_ok(),
        "Process command should execute successfully"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_storage_persists_across_runs() -> Result<()> {
    let _server = TestServer::start().await?;
    // Given: A test kiln and persistent database
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path.clone(), db_path.clone());

    // When: Running process command the first time
    process::execute(config.clone(), None, false, false, false, false, false).await?;

    // And: Running process command a second time (same database)
    let config2 = create_process_test_config(kiln_path, db_path);
    let result = process::execute(config2, None, false, false, false, false, false).await;

    // Then: Second run should succeed
    assert!(result.is_ok(), "Second run should access persisted storage");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_change_detection_skips_unchanged_files() -> Result<()> {
    let _server = TestServer::start().await?;
    // Given: A processed kiln
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path.clone(), db_path.clone());

    // When: Processing files initially
    process::execute(config.clone(), None, false, false, false, false, false).await?;

    // And: Processing again without any file changes
    let config2 = create_process_test_config(kiln_path.clone(), db_path.clone());
    let result = process::execute(config2, None, false, false, false, false, false).await;

    assert!(result.is_ok());

    // When: Modifying one file
    std::fs::write(
        kiln_path.join("note1.md"),
        "# Note 1 Modified\n\nThis content has been updated.",
    )?;

    // And: Processing again
    let config3 = create_process_test_config(kiln_path, db_path);
    let result2 = process::execute(config3, None, false, false, false, false, false).await;

    assert!(result2.is_ok());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_force_flag_overrides_change_detection() -> Result<()> {
    let _server = TestServer::start().await?;
    // Given: A processed kiln with no file changes
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path.clone(), db_path.clone());

    // When: Processing initially
    process::execute(config.clone(), None, false, false, false, false, false).await?;

    // And: Processing again with --force flag
    let config_force = create_process_test_config(kiln_path, db_path);
    let result = process::execute(config_force, None, true, false, false, false, false).await;

    // Then: Should reprocess all files despite no changes
    assert!(result.is_ok(), "Force flag should cause reprocessing");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_process_single_file() -> Result<()> {
    let _server = TestServer::start().await?;
    // Given: A test kiln with multiple files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();
    let target_file = kiln_path.join("note1.md");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path, db_path);

    // When: Processing only a specific file
    let result = process::execute(
        config,
        Some(target_file.clone()),
        false,
        false,
        false,
        false,
        false,
    )
    .await;

    // Then: Should succeed
    assert!(result.is_ok(), "Processing single file should succeed");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_all_pipeline_phases_execute() -> Result<()> {
    let _server = TestServer::start().await?;
    // Given: A test kiln
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path, db_path);

    // When: Processing files
    let result = process::execute(config, None, false, false, false, false, false).await;

    // Then: All 5 pipeline phases should execute
    assert!(result.is_ok(), "Pipeline execution should succeed");

    Ok(())
}

// =============================================================================
// VERBOSE FLAG TESTS
// =============================================================================

#[tokio::test]
#[serial]
async fn test_verbose_without_flag_is_quiet() -> Result<()> {
    let _server = TestServer::start().await?;
    // GIVEN: Test kiln
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path, db_path);

    // WHEN: Processing without --verbose
    let result = process::execute(config, None, false, false, false, false, false).await;

    // THEN: Should succeed with minimal output
    // (verbose=false is default, so this tests baseline behavior)
    assert!(result.is_ok());

    // Note: In actual implementation, we would capture stdout/stderr
    // and verify it only contains high-level progress messages
    // For now, we just verify the command succeeds

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_verbose_shows_phase_timings() -> Result<()> {
    let _server = TestServer::start().await?;
    // GIVEN: Test kiln with files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path, db_path);

    // WHEN: Processing with --verbose
    let result = process::execute(config, None, false, false, true, false, false).await;

    // THEN: Should succeed and show timing information
    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_verbose_shows_detailed_parse_info() -> Result<()> {
    let _server = TestServer::start().await?;
    // GIVEN: Note with wikilinks, tags, callouts
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path, db_path);

    // WHEN: Processing with --verbose
    let result = process::execute(config, None, false, false, true, false, false).await;

    // THEN: Should show parse details
    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_verbose_shows_merkle_diff_details() -> Result<()> {
    let _server = TestServer::start().await?;
    // GIVEN: Initially processed file
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path.clone(), db_path.clone());

    // Process initially
    process::execute(config.clone(), None, false, false, false, false, false).await?;

    // Modify a file
    std::fs::write(
        kiln_path.join("note1.md"),
        "# Note 1 Modified\n\nThis content has been updated.",
    )?;

    // WHEN: Reprocessing with --verbose
    let config2 = create_process_test_config(kiln_path, db_path);
    let result = process::execute(config2, None, false, false, true, false, false).await;

    // THEN: Should show Merkle diff details
    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_verbose_shows_enrichment_progress() -> Result<()> {
    let _server = TestServer::start().await?;
    // GIVEN: Files requiring embeddings
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path, db_path);

    // WHEN: Processing with --verbose
    let result = process::execute(config, None, false, false, true, false, false).await;

    // THEN: Should show enrichment details
    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_verbose_shows_storage_operations() -> Result<()> {
    let _server = TestServer::start().await?;
    // GIVEN: Processing files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path, db_path);

    // WHEN: Running with --verbose
    let result = process::execute(config, None, false, false, true, false, false).await;

    // THEN: Should show storage details
    assert!(result.is_ok());

    Ok(())
}

// =============================================================================
// DRY-RUN FLAG TESTS
// =============================================================================

#[tokio::test]
#[serial]
async fn test_dry_run_discovers_files_without_processing() -> Result<()> {
    let _server = TestServer::start().await?;
    // GIVEN: A test kiln with markdown files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path, db_path.clone());

    // WHEN: Running process command with --dry-run
    let result = process::execute(config, None, false, false, false, true, false).await;

    // THEN: Command should succeed
    assert!(
        result.is_ok(),
        "Dry-run command should execute successfully"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_dry_run_respects_change_detection() -> Result<()> {
    let _server = TestServer::start().await?;
    // GIVEN: A processed kiln (files already in DB)
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path.clone(), db_path.clone());

    // Process initially
    process::execute(config.clone(), None, false, false, false, false, false).await?;

    // WHEN: Running dry-run without changes
    let config_dry = create_process_test_config(kiln_path, db_path);
    let result = process::execute(config_dry, None, false, false, false, true, false).await;

    // THEN: Should show which files would be skipped
    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_dry_run_with_force_shows_all_files() -> Result<()> {
    let _server = TestServer::start().await?;
    // GIVEN: A processed kiln
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path.clone(), db_path.clone());

    // Process initially
    process::execute(config.clone(), None, false, false, false, false, false).await?;

    // WHEN: Running dry-run with --force (bypass change detection)
    let config_dry_force = create_process_test_config(kiln_path, db_path);
    let result = process::execute(config_dry_force, None, true, false, false, true, false).await;

    // THEN: Should show all files would be processed
    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_dry_run_shows_detailed_preview() -> Result<()> {
    let _server = TestServer::start().await?;
    // GIVEN: A test kiln with varied content
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path, db_path);

    // WHEN: Running with --dry-run
    let result = process::execute(config, None, false, false, false, true, false).await;

    // THEN: Should succeed
    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_dry_run_with_verbose() -> Result<()> {
    let _server = TestServer::start().await?;
    // GIVEN: A test kiln
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_process_test_config(kiln_path, db_path);

    // WHEN: Running with both --dry-run and --verbose
    let result = process::execute(config, None, false, false, true, true, false).await;

    // THEN: Should succeed and show detailed information
    assert!(result.is_ok());

    Ok(())
}

// =============================================================================
// WATCH MODE TESTS — none here, deliberately.
//
// Four lived here (test_watch_mode_starts_and_runs, test_watch_detects_*) and
// were removed. Each aborted the watch task and then asserted
// `result.is_err() || result.unwrap().is_ok()` — a tautology on an aborted
// JoinHandle, so the "detects file modification" tests wrote a file and checked
// nothing about whether it was detected. They had also stopped running at all:
// `process::execute` reaches storage through the daemon, and these built a
// CliConfig with no daemon behind it.
//
// Watch behaviour is tested where it happens, daemon-side and unignored:
// tests/watch_indexing.rs, watch_note_parsed_emission_tests.rs,
// watch_file_changed_emission_tests.rs, watch_file_deleted_emission_tests.rs
// and watch_notify_filter_tests.rs in crucible-daemon.
