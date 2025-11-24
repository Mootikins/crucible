//! Integration tests for process command
//!
//! These tests verify the process command correctly:
//! 1. Uses persistent SurrealDB storage (not in-memory)
//! 2. Produces consistent output with chat command's pre-processing
//! 3. Implements proper change detection
//! 4. Executes all 5 pipeline phases

use anyhow::Result;
use crucible_cli::commands::process;
use crucible_cli::config::{CliConfig, KilnConfig, LlmConfig};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

/// Helper to create a test kiln with sample markdown files
fn create_test_kiln() -> Result<TempDir> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    std::fs::create_dir_all(&kiln_path)?;

    // Create sample markdown files with different content
    std::fs::write(
        kiln_path.join("note1.md"),
        "# Note 1\n\nThis is the first test note with some content.",
    )?;

    std::fs::write(
        kiln_path.join("note2.md"),
        "# Note 2\n\nThis is the second test note.\n\n## Section\n\nWith multiple blocks.",
    )?;

    std::fs::write(
        kiln_path.join("note3.md"),
        "# Note 3\n\n[[note1]] is linked here.\n\n#tag1 #tag2",
    )?;

    Ok(temp_dir)
}

/// Helper to create test CLI config
fn create_test_config(kiln_path: PathBuf, db_path: PathBuf) -> CliConfig {
    CliConfig {
        kiln: KilnConfig {
            path: kiln_path,
            embedding_url: "https://llama.terminal.krohnos.io".to_string(),
            embedding_model: Some("nomic-embed-text-v1.5-q8_0".to_string()),
        },
        llm: LlmConfig {
            default_agent: Some("test-agent".to_string()),
            ..Default::default()
        },
        custom_database_path: Some(db_path),
        ..Default::default()
    }
}

#[tokio::test]
async fn test_process_executes_pipeline() -> Result<()> {
    // Given: A test kiln with markdown files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().join("test-kiln");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path);

    // When: Running the process command
    let result = process::execute(config, None, false, false, false).await;

    // Then: Command should succeed
    assert!(result.is_ok(), "Process command should execute successfully");

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
    let kiln_path = temp_dir.path().join("test-kiln");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // When: Running process command the first time
    process::execute(config.clone(), None, false, false, false).await?;

    // Give time for database to fully close
    sleep(Duration::from_millis(100)).await;

    // And: Running process command a second time (same database)
    let config2 = create_test_config(kiln_path, db_path);
    let result = process::execute(config2, None, false, false, false).await;

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
    let kiln_path = temp_dir.path().join("test-kiln");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // When: Processing files initially
    process::execute(config.clone(), None, false, false, false).await?;

    // Give time for database to fully close
    sleep(Duration::from_millis(100)).await;

    // And: Processing again without any file changes
    let config2 = create_test_config(kiln_path.clone(), db_path.clone());
    let result = process::execute(config2, None, false, false, false).await;

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
    let result2 = process::execute(config3, None, false, false, false).await;

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
    let kiln_path = temp_dir.path().join("test-kiln");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // When: Processing initially
    process::execute(config.clone(), None, false, false, false).await?;

    // Give time for database to fully close
    sleep(Duration::from_millis(100)).await;

    // And: Processing again with --force flag
    let config_force = create_test_config(kiln_path, db_path);
    let result = process::execute(config_force, None, true, false, false).await;

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
    let kiln_path = temp_dir.path().join("test-kiln");
    let target_file = kiln_path.join("note1.md");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // When: Processing only a specific file
    let result = process::execute(config, Some(target_file.clone()), false, false, false).await;

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
    let kiln_path = temp_dir.path().join("test-kiln");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // When: Processing files
    let result = process::execute(config, None, false, false, false).await;

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
    let kiln_path = temp_dir.path().join("test-kiln");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Processing without --verbose
    let result = process::execute(config, None, false, false, false).await;

    // THEN: Should succeed with minimal output
    // (verbose=false is default, so this tests baseline behavior)
    assert!(result.is_ok());

    // Note: In actual implementation, we would capture stdout/stderr
    // and verify it only contains high-level progress messages
    // For now, we just verify the command succeeds

    Ok(())
}

#[tokio::test]
#[ignore = "Requires verbose flag implementation"]
async fn test_verbose_shows_phase_timings() -> Result<()> {
    // GIVEN: Test kiln with files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().join("test-kiln");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Processing with --verbose
    let result = process::execute(config, None, false, false, true).await;

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
#[ignore = "Requires verbose flag implementation"]
async fn test_verbose_shows_detailed_parse_info() -> Result<()> {
    // GIVEN: Note with wikilinks, tags, callouts
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().join("test-kiln");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Processing with --verbose
    let result = process::execute(config, None, false, false, true).await;

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
#[ignore = "Requires verbose flag implementation"]
async fn test_verbose_shows_merkle_diff_details() -> Result<()> {
    // GIVEN: Initially processed file
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().join("test-kiln");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path.clone(), db_path.clone());

    // Process initially
    process::execute(config.clone(), None, false, false, false).await?;

    sleep(Duration::from_millis(100)).await;

    // Modify a file
    std::fs::write(
        kiln_path.join("note1.md"),
        "# Note 1 Modified\n\nThis content has been updated.",
    )?;

    // WHEN: Reprocessing with --verbose
    let config2 = create_test_config(kiln_path, db_path);
    let result = process::execute(config2, None, false, false, true).await;

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
#[ignore = "Requires verbose flag implementation"]
async fn test_verbose_shows_enrichment_progress() -> Result<()> {
    // GIVEN: Files requiring embeddings
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().join("test-kiln");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Processing with --verbose
    let result = process::execute(config, None, false, false, true).await;

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
#[ignore = "Requires verbose flag implementation"]
async fn test_verbose_shows_storage_operations() -> Result<()> {
    // GIVEN: Processing files
    let temp_dir = create_test_kiln()?;
    let kiln_path = temp_dir.path().join("test-kiln");

    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("test.db");

    let config = create_test_config(kiln_path, db_path);

    // WHEN: Running with --verbose
    let result = process::execute(config, None, false, false, true).await;

    // THEN: Should show storage details
    assert!(result.is_ok());

    // TODO: After implementation, verify output shows:
    // - "Updating file_state table"
    // - "Storing 3 enriched notes"
    // - "Updating Merkle trees"
    // - Record counts

    Ok(())
}
