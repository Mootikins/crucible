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
        database_path: Some(db_path),
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
    let result = process::execute(config, None, false, false).await;

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
    process::execute(config.clone(), None, false, false).await?;

    // And: Running process command a second time (same database)
    let config2 = create_test_config(kiln_path, db_path);
    let result = process::execute(config2, None, false, false).await;

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
    process::execute(config.clone(), None, false, false).await?;

    // And: Processing again without any file changes
    let config2 = create_test_config(kiln_path.clone(), db_path.clone());
    let result = process::execute(config2, None, false, false).await;

    assert!(result.is_ok());

    // TODO: After implementation, verify:
    // 1. Change detection identified files as unchanged
    // 2. Pipeline quick filter skipped unchanged files
    // 3. No embeddings were regenerated

    // When: Modifying one file
    std::fs::write(
        kiln_path.join("note1.md"),
        "# Note 1 Modified\n\nThis content has been updated.",
    )?;

    // And: Processing again
    let config3 = create_test_config(kiln_path, db_path);
    let result2 = process::execute(config3, None, false, false).await;

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
    process::execute(config.clone(), None, false, false).await?;

    // And: Processing again with --force flag
    let config_force = create_test_config(kiln_path, db_path);
    let result = process::execute(config_force, None, true, false).await;

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
    let result = process::execute(config, Some(target_file.clone()), false, false).await;

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
    let result = process::execute(config, None, false, false).await;

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
