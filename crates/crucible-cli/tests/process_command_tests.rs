//! Integration tests for process command

#![allow(clippy::field_reassign_with_default)]

//!
//! These tests verify the process command correctly:
//! 1. Uses persistent SurrealDB storage (not in-memory)
//! 2. Produces consistent output with chat command's pre-processing
//! 3. Implements proper change detection
//! 4. Executes all 5 pipeline phases

use anyhow::Result;
use crucible_cli::commands::process;
use crucible_cli::config::{CliAppConfig, CliConfig};
use crucible_config::{
    AcpConfig, ChatConfig, EmbeddingConfig, EmbeddingProviderType, LlmConfig, ProcessingConfig,
    ProvidersConfig, StorageConfig, StorageMode,
};
use crucible_core::test_support::fixtures::{create_kiln, KilnFixture};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

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
fn create_test_config(kiln_path: PathBuf, _db_path: PathBuf) -> CliConfig {
    CliConfig {
        kiln_path,
        agent_directories: Vec::new(),
        embedding: EmbeddingConfig {
            provider: EmbeddingProviderType::Mock,
            model: None,
            api_url: None,
            batch_size: 16,
            max_concurrent: None,
        },
        acp: AcpConfig {
            default_agent: Some("test-agent".to_string()),
            ..Default::default()
        },
        chat: ChatConfig::default(),
        llm: LlmConfig::default(),
        cli: CliAppConfig::default(),
        logging: None,
        processing: ProcessingConfig::default(),
        providers: ProvidersConfig::default(),
        context: None,
        storage: Some(StorageConfig {
            mode: StorageMode::Embedded,
            idle_timeout_secs: 300,
        }),
        mcp: None,
        plugins: std::collections::HashMap::new(),
        web: None,
        source_map: None,
    }
}

#[tokio::test]
async fn test_process_executes_pipeline() -> Result<()> {
    // Given: A test kiln with markdown files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // When: Running the process command
    let result = process::execute(config, None, false, false, false, false, None).await;

    // Then: Command should succeed
    assert!(
        result.is_ok(),
        "Process command should execute successfully"
    );

    // TODO: After implementation, verify:
    // 1. Pipeline was actually invoked (not just a stub)
    // 2. Files were processed through all 5 phases
    // 3. Data was written to SurrealDB

    Ok(())
}

#[tokio::test]
async fn test_storage_persists_across_runs() -> Result<()> {
    // Given: A test kiln and persistent database
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // When: Running process command the first time
    process::execute(config.clone(), None, false, false, false, false, None).await?;

    // Give time for database to fully close
    sleep(Duration::from_millis(100)).await;

    // And: Running process command a second time (same database)
    let config2 = create_test_config(kiln_path, db_path);
    let result = process::execute(config2, None, false, false, false, false, None).await;

    // Then: Second run should succeed
    assert!(result.is_ok(), "Second run should access persisted storage");

    // TODO: After implementation, verify:
    // 1. Data from first run is still present
    // 2. Change detection recognizes files haven't changed
    // 3. Files are not reprocessed unnecessarily

    Ok(())
}

#[tokio::test]
async fn test_change_detection_skips_unchanged_files() -> Result<()> {
    // Given: A processed kiln
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // When: Processing files initially
    process::execute(config.clone(), None, false, false, false, false, None).await?;

    // Give time for database to fully close
    sleep(Duration::from_millis(100)).await;

    // And: Processing again without any file changes
    let config2 = create_test_config(kiln_path.clone(), db_path.clone());
    let result = process::execute(config2, None, false, false, false, false, None).await;

    assert!(result.is_ok());

    // TODO: After implementation, verify:
    // 1. Change detection identified files as unchanged
    // 2. Pipeline quick filter skipped unchanged files
    // 3. No embeddings were regenerated

    // Give time for database to fully close
    sleep(Duration::from_millis(100)).await;

    // When: Modifying one file
    std::fs::write(
        kiln_path.join("note1.md"),
        "# Note 1 Modified\n\nThis content has been updated.",
    )?;

    // And: Processing again
    let config3 = create_test_config(kiln_path, db_path);
    let result2 = process::execute(config3, None, false, false, false, false, None).await;

    assert!(result2.is_ok());

    // TODO: After implementation, verify:
    // 1. Only note1.md was reprocessed
    // 2. note2.md and note3.md were skipped
    // 3. Embeddings only generated for modified file

    Ok(())
}

#[tokio::test]
async fn test_force_flag_overrides_change_detection() -> Result<()> {
    // Given: A processed kiln with no file changes
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // When: Processing initially
    process::execute(config.clone(), None, false, false, false, false, None).await?;

    // Give time for database to fully close
    sleep(Duration::from_millis(100)).await;

    // And: Processing again with --force flag
    let config_force = create_test_config(kiln_path, db_path);
    let result = process::execute(config_force, None, true, false, false, false, None).await;

    // Then: Should reprocess all files despite no changes
    assert!(result.is_ok(), "Force flag should cause reprocessing");

    // TODO: After implementation, verify:
    // 1. All files were reprocessed
    // 2. Change detection was bypassed
    // 3. New embeddings were generated

    Ok(())
}

#[tokio::test]
async fn test_process_single_file() -> Result<()> {
    // Given: A test kiln with multiple files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();
    let target_file = kiln_path.join("note1.md");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // When: Processing only a specific file
    let result = process::execute(
        config,
        Some(target_file.clone()),
        false,
        false,
        false,
        false,
        None,
    )
    .await;

    // Then: Should succeed
    assert!(result.is_ok(), "Processing single file should succeed");

    // TODO: After implementation, verify:
    // 1. Only note1.md was processed
    // 2. note2.md and note3.md were not touched
    // 3. Storage contains data only for processed file

    Ok(())
}

#[tokio::test]
async fn test_all_pipeline_phases_execute() -> Result<()> {
    // Given: A test kiln
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // When: Processing files
    let result = process::execute(config, None, false, false, false, false, None).await;

    // Then: All 5 pipeline phases should execute
    assert!(result.is_ok(), "Pipeline execution should succeed");

    // TODO: After implementation, verify each phase executed:
    // Phase 1: Quick Filter (change detection)
    // Phase 2: Parse (extract blocks, links, tags)
    // Phase 3: Merkle Diff (identify changed blocks)
    // Phase 4: Enrich (generate embeddings)
    // Phase 5: Store (persist to SurrealDB)
    //
    // Verification approaches:
    // - Query SurrealDB tables (file_state, enriched_notes, embeddings)
    // - Check for parsed content (blocks, wikilinks, tags)
    // - Verify embeddings were generated for content
    // - Confirm Merkle trees were computed

    Ok(())
}

// Note: Output consistency test will be added after implementation
// This test will compare process command output with chat command's pre-processing
// and verify they produce identical results for the same input files.
#[tokio::test]
#[ignore = "Requires both commands to be fully implemented"]
async fn test_output_consistency_with_chat_preprocessing() -> Result<()> {
    // This test will:
    // 1. Run process command on test files
    // 2. Run chat command with pre-processing on same files
    // 3. Compare results from SurrealDB
    // 4. Assert identical: embeddings, Merkle trees, metadata

    Ok(())
}

// =============================================================================
// VERBOSE FLAG TESTS
// =============================================================================

#[tokio::test]
async fn test_verbose_without_flag_is_quiet() -> Result<()> {
    // GIVEN: Test kiln
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Processing without --verbose
    let result = process::execute(config, None, false, false, false, false, None).await;

    // THEN: Should succeed with minimal output
    // (verbose=false is default, so this tests baseline behavior)
    assert!(result.is_ok());

    // Note: In actual implementation, we would capture stdout/stderr
    // and verify it only contains high-level progress messages
    // For now, we just verify the command succeeds

    Ok(())
}

#[tokio::test]
async fn test_verbose_shows_phase_timings() -> Result<()> {
    // GIVEN: Test kiln with files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Processing with --verbose
    let result = process::execute(config, None, false, false, true, false, None).await;

    // THEN: Should succeed and show timing information
    assert!(result.is_ok());

    // TODO: After implementation, capture output and verify:
    // - "Phase 1: Quick filter" with duration
    // - "Phase 2: Parse" with duration
    // - "Phase 3: Merkle Diff" with duration
    // - "Phase 4: Enrich" with duration
    // - "Phase 5: Store" with duration
    // - Total pipeline time

    Ok(())
}

#[tokio::test]
async fn test_verbose_shows_detailed_parse_info() -> Result<()> {
    // GIVEN: Note with wikilinks, tags, callouts
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Processing with --verbose
    let result = process::execute(config, None, false, false, true, false, None).await;

    // THEN: Should show parse details
    assert!(result.is_ok());

    // TODO: After implementation, verify output shows:
    // - Extracted wikilinks with targets
    // - Found tags (#tag1, #tag2)
    // - Block count
    // - File hash (first 8 chars)

    Ok(())
}

#[tokio::test]
async fn test_verbose_shows_merkle_diff_details() -> Result<()> {
    // GIVEN: Initially processed file
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // Process initially
    process::execute(config.clone(), None, false, false, false, false, None).await?;

    sleep(Duration::from_millis(100)).await;

    // Modify a file
    std::fs::write(
        kiln_path.join("note1.md"),
        "# Note 1 Modified\n\nThis content has been updated.",
    )?;

    // WHEN: Reprocessing with --verbose
    let config2 = create_test_config(kiln_path, db_path);
    let result = process::execute(config2, None, false, false, true, false, None).await;

    // THEN: Should show Merkle diff details
    assert!(result.is_ok());

    // TODO: After implementation, verify output shows:
    // - Old Merkle root hash
    // - New Merkle root hash
    // - Changed section indices
    // - "2 blocks changed" or similar

    Ok(())
}

#[tokio::test]
async fn test_verbose_shows_enrichment_progress() -> Result<()> {
    // GIVEN: Files requiring embeddings
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Processing with --verbose
    let result = process::execute(config, None, false, false, true, false, None).await;

    // THEN: Should show enrichment details
    assert!(result.is_ok());

    // TODO: After implementation, verify output shows:
    // - Embedding service URL
    // - Model name (nomic-embed-text-v1.5-q8_0)
    // - "Generating embeddings for 5 blocks" or similar
    // - Embedding API call timing

    Ok(())
}

#[tokio::test]
async fn test_verbose_shows_storage_operations() -> Result<()> {
    // GIVEN: Processing files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Running with --verbose
    let result = process::execute(config, None, false, false, true, false, None).await;

    // THEN: Should show storage details
    assert!(result.is_ok());

    // TODO: After implementation, verify output shows:
    // - "Updating file_state table"
    // - "Storing 3 enriched notes"
    // - "Updating Merkle trees"
    // - Record counts

    Ok(())
}

// =============================================================================
// DRY-RUN FLAG TESTS
// =============================================================================

#[tokio::test]
async fn test_dry_run_discovers_files_without_processing() -> Result<()> {
    // GIVEN: A test kiln with markdown files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path.clone());

    // WHEN: Running process command with --dry-run
    let result = process::execute(config, None, false, false, false, true, None).await;

    // THEN: Command should succeed
    assert!(
        result.is_ok(),
        "Dry-run command should execute successfully"
    );

    // AND: Database should be empty (no files were actually processed)
    // TODO: After implementation, verify:
    // 1. Files were discovered (shown in output)
    // 2. No data was written to SurrealDB
    // 3. Summary showed "Would process: 3 files"

    Ok(())
}

#[tokio::test]
async fn test_dry_run_respects_change_detection() -> Result<()> {
    // GIVEN: A processed kiln (files already in DB)
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // Process initially
    process::execute(config.clone(), None, false, false, false, false, None).await?;

    sleep(Duration::from_millis(100)).await;

    // WHEN: Running dry-run without changes
    let config_dry = create_test_config(kiln_path, db_path);
    let result = process::execute(config_dry, None, false, false, false, true, None).await;

    // THEN: Should show which files would be skipped
    assert!(result.is_ok());

    // TODO: After implementation, verify:
    // 1. Output shows "Would skip: 3 files (unchanged)"
    // 2. Change detection logic was still applied
    // 3. No database modifications occurred

    Ok(())
}

#[tokio::test]
async fn test_dry_run_with_force_shows_all_files() -> Result<()> {
    // GIVEN: A processed kiln
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // Process initially
    process::execute(config.clone(), None, false, false, false, false, None).await?;

    sleep(Duration::from_millis(100)).await;

    // WHEN: Running dry-run with --force (bypass change detection)
    let config_dry_force = create_test_config(kiln_path, db_path);
    let result = process::execute(config_dry_force, None, true, false, false, true, None).await;

    // THEN: Should show all files would be processed
    assert!(result.is_ok());

    // TODO: After implementation, verify:
    // 1. Output shows "Would process: 3 files" (not skipped)
    // 2. Force flag bypassed change detection
    // 3. No database modifications occurred

    Ok(())
}

#[tokio::test]
async fn test_dry_run_shows_detailed_preview() -> Result<()> {
    // GIVEN: A test kiln with varied content
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Running with --dry-run
    let result = process::execute(config, None, false, false, false, true, None).await;

    // THEN: Should succeed
    assert!(result.is_ok());

    // TODO: After implementation, verify output shows:
    // - "DRY RUN MODE - No changes will be made to database"
    // - For each file: "Would process: note1.md (1 section, 0 links)"
    // - Summary: "Would process: X files, Would skip: Y files"
    // - File hash preview (first 8 chars)

    Ok(())
}

#[tokio::test]
async fn test_dry_run_with_verbose() -> Result<()> {
    // GIVEN: A test kiln
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Running with both --dry-run and --verbose
    let result = process::execute(config, None, false, false, true, true, None).await;

    // THEN: Should succeed and show detailed information
    assert!(result.is_ok());

    // TODO: After implementation, verify output shows:
    // - All verbose phase timing information
    // - Detailed file preview for each file
    // - "DRY RUN MODE" indicator in verbose output
    // - Change detection details per file

    Ok(())
}

// =============================================================================
// WATCH MODE TESTS
// =============================================================================
//
// These tests verify file watching behavior using timeout patterns:
// 1. Spawn watch mode in background task
// 2. Make file changes after watcher initializes
// 3. Use timeout to limit execution (watch runs indefinitely otherwise)
// 4. Verify behavior via timeout completion (Err = timed out = watch was running)

#[tokio::test]
#[ignore = "slow watch integration test - run with just test slow"]
async fn test_watch_mode_starts_and_runs() -> Result<()> {
    // GIVEN: A test kiln with initial processing complete
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Running watch mode with a timeout
    // Watch mode runs indefinitely, so timeout = success (it was running)
    let watch_result = tokio::time::timeout(
        Duration::from_secs(2),
        process::execute(config, None, false, true, false, false, None),
    )
    .await;

    // THEN: Should timeout (watch was actively running)
    // Timeout error means watch started and was running correctly
    assert!(
        watch_result.is_err(),
        "Watch mode should run until timeout (not return early)"
    );

    Ok(())
}

#[tokio::test]
#[ignore = "slow watch integration test - run with just test slow"]
async fn test_watch_detects_file_modification() -> Result<()> {
    // GIVEN: A test kiln with initial processing
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // Initial processing to populate database
    process::execute(config.clone(), None, false, false, false, false, None).await?;

    // WHEN: Start watch mode in background
    let watch_config = create_test_config(kiln_path.clone(), db_path);
    let watch_handle = tokio::spawn(async move {
        process::execute(watch_config, None, false, true, true, false, None).await
    });

    // Wait for watcher to initialize
    sleep(Duration::from_millis(500)).await;

    // Modify a file while watch is active
    std::fs::write(
        kiln_path.join("note1.md"),
        "# Note 1 Modified\n\nUpdated content detected by watcher.",
    )?;

    // Allow time for debounce (500ms) + processing
    sleep(Duration::from_secs(2)).await;

    // Cancel watch mode
    watch_handle.abort();

    // THEN: Watch should have been running (abort returns Err)
    let result = watch_handle.await;
    assert!(
        result.is_err() || result.unwrap().is_ok(),
        "Watch should either be aborted or complete successfully"
    );

    Ok(())
}

#[tokio::test]
#[ignore = "slow watch integration test - run with just test slow"]
async fn test_watch_detects_new_file_creation() -> Result<()> {
    // GIVEN: A test kiln with watch mode starting
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path);

    // Start watch mode in background
    let watch_handle = tokio::spawn(async move {
        process::execute(config, None, false, true, true, false, None).await
    });

    // Wait for watcher to initialize
    sleep(Duration::from_millis(500)).await;

    // WHEN: Creating a new markdown file while watching
    std::fs::write(
        kiln_path.join("note_new.md"),
        "# New Note\n\nCreated during watch mode.",
    )?;

    // Allow time for debounce + processing
    sleep(Duration::from_secs(2)).await;

    // Cancel watch mode
    watch_handle.abort();

    // THEN: Watch should have been running
    let result = watch_handle.await;
    assert!(
        result.is_err() || result.unwrap().is_ok(),
        "Watch should either be aborted or complete successfully"
    );

    Ok(())
}

#[tokio::test]
#[ignore = "slow watch integration test - run with just test slow"]
async fn test_watch_detects_file_deletion() -> Result<()> {
    // GIVEN: A test kiln with initial processing
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // Initial processing
    process::execute(config.clone(), None, false, false, false, false, None).await?;

    // Start watch mode in background
    let watch_config = create_test_config(kiln_path.clone(), db_path);
    let watch_handle = tokio::spawn(async move {
        process::execute(watch_config, None, false, true, true, false, None).await
    });

    // Wait for watcher to initialize
    sleep(Duration::from_millis(500)).await;

    // WHEN: Deleting a file while watching
    std::fs::remove_file(kiln_path.join("note1.md"))?;

    // Allow time for event detection
    sleep(Duration::from_secs(2)).await;

    // Cancel watch mode
    watch_handle.abort();

    // THEN: Watch should have been running
    let result = watch_handle.await;
    assert!(
        result.is_err() || result.unwrap().is_ok(),
        "Watch should either be aborted or complete successfully"
    );

    Ok(())
}

// NOTE: The following stub tests were removed as they were broken:
// - test_watch_ignores_non_markdown_files: Would hang (no timeout/spawn)
// - test_watch_handles_rapid_changes_with_debounce: Would hang (no timeout/spawn)
// - test_watch_handles_errors_gracefully: Would hang (no timeout/spawn)
// - test_watch_can_be_cancelled: Would hang (no timeout/spawn)
// - test_watch_respects_change_detection: Would hang (no timeout/spawn)
// - test_watch_with_verbose: Would hang (no timeout/spawn)
//
// The working watch tests (test_watch_mode_starts_and_runs, test_watch_detects_*)
// properly use timeout/spawn patterns. Add new watch tests following that pattern.

// =============================================================================
// NOTE LIFECYCLE EVENT TESTS
// =============================================================================
//
// These tests verify that processing emits note lifecycle events through
// the Reactor, allowing Rune handlers to react to note changes.

use async_trait::async_trait;
use crucible_core::events::{Handler, HandlerContext, HandlerResult, Reactor, SessionEvent};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

/// A test handler that counts events matching a pattern
struct CountingHandler {
    name: &'static str,
    pattern: &'static str,
    event_count: Arc<AtomicUsize>,
}

impl CountingHandler {
    fn new(name: &'static str, pattern: &'static str, counter: Arc<AtomicUsize>) -> Self {
        Self {
            name,
            pattern,
            event_count: counter,
        }
    }
}

#[async_trait]
impl Handler for CountingHandler {
    fn name(&self) -> &str {
        self.name
    }

    fn event_pattern(&self) -> &str {
        self.pattern
    }

    fn priority(&self) -> i32 {
        100
    }

    fn dependencies(&self) -> &[&str] {
        &[]
    }

    async fn handle(
        &self,
        _ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        self.event_count.fetch_add(1, AtomicOrdering::SeqCst);
        HandlerResult::Continue(event)
    }
}

#[tokio::test]
async fn test_process_emits_note_events_to_reactor() -> Result<()> {
    // GIVEN: A test kiln with markdown files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().to_path_buf();

    // Create handlers directory with a simple Rune handler
    let handlers_dir = kiln_path.join(".crucible").join("handlers");
    std::fs::create_dir_all(&handlers_dir)?;

    // Create a Rune handler that matches note events
    // The handler uses the wildcard pattern to receive all events
    let handler_content = r#"
// Handler that receives note events
pub fn handle(event) {
    // Just pass through - we're testing that it gets called
    event
}
"#;
    std::fs::write(handlers_dir.join("note_handler.rn"), handler_content)?;

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Processing files
    let result = process::execute(config, None, false, false, false, false, None).await;

    // THEN: Should succeed and Rune handlers should have been loaded
    assert!(result.is_ok(), "Process command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_reactor_receives_note_modified_events() -> Result<()> {
    // GIVEN: A reactor with a handler that counts NoteModified events
    let note_modified_count = Arc::new(AtomicUsize::new(0));
    let note_parsed_count = Arc::new(AtomicUsize::new(0));

    let mut reactor = Reactor::new();
    reactor
        .register(Box::new(CountingHandler::new(
            "note_modified_counter",
            "note_modified",
            note_modified_count.clone(),
        )))
        .expect("Should register handler");
    reactor
        .register(Box::new(CountingHandler::new(
            "note_parsed_counter",
            "note_parsed",
            note_parsed_count.clone(),
        )))
        .expect("Should register handler");

    // WHEN: Emitting note events (simulating what process.rs does)
    let note_modified_event = SessionEvent::NoteModified {
        path: PathBuf::from("/test/note.md"),
        change_type: crucible_core::events::NoteChangeType::Content,
    };
    let note_parsed_event = SessionEvent::NoteParsed {
        path: PathBuf::from("/test/note.md"),
        block_count: 5,
        payload: None,
    };

    reactor
        .emit(note_modified_event)
        .await
        .expect("Should emit");
    reactor.emit(note_parsed_event).await.expect("Should emit");

    // THEN: Handlers should have received the events
    assert_eq!(
        note_modified_count.load(AtomicOrdering::SeqCst),
        1,
        "NoteModified handler should receive 1 event"
    );
    assert_eq!(
        note_parsed_count.load(AtomicOrdering::SeqCst),
        1,
        "NoteParsed handler should receive 1 event"
    );

    Ok(())
}

#[tokio::test]
async fn test_reactor_wildcard_pattern_receives_note_events() -> Result<()> {
    // GIVEN: A reactor with a wildcard handler (like Rune handlers use)
    let all_events_count = Arc::new(AtomicUsize::new(0));

    let mut reactor = Reactor::new();
    reactor
        .register(Box::new(CountingHandler::new(
            "wildcard_counter",
            "*", // Matches all events
            all_events_count.clone(),
        )))
        .expect("Should register handler");

    // WHEN: Emitting different note lifecycle events
    let events = vec![
        SessionEvent::NoteModified {
            path: PathBuf::from("/test/note1.md"),
            change_type: crucible_core::events::NoteChangeType::Content,
        },
        SessionEvent::NoteParsed {
            path: PathBuf::from("/test/note1.md"),
            block_count: 3,
            payload: None,
        },
        SessionEvent::NoteModified {
            path: PathBuf::from("/test/note2.md"),
            change_type: crucible_core::events::NoteChangeType::Content,
        },
        SessionEvent::NoteParsed {
            path: PathBuf::from("/test/note2.md"),
            block_count: 7,
            payload: None,
        },
    ];

    for event in events {
        reactor.emit(event).await.expect("Should emit");
    }

    // THEN: Wildcard handler should have received all events
    assert_eq!(
        all_events_count.load(AtomicOrdering::SeqCst),
        4,
        "Wildcard handler should receive all 4 events"
    );

    Ok(())
}
