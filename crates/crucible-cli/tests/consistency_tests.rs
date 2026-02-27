//! Integration test: process/stats consistency
//!
//! Proves that `cru process` (via KilnManager) and `cru stats`
//! (via FileSystemKilnStatsService) agree on the same file counts
//! for a given kiln. This is the keystone test for cross-command
//! consistency on file discovery and exclusion.

use anyhow::Result;
use crucible_cli::commands::stats::{FileSystemKilnStatsService, KilnStatsService};
use crucible_daemon::kiln_manager::KilnManager;
use tempfile::TempDir;

/// Create a test kiln with:
/// - 3 `.md` files in root (should be counted)
/// - 1 `.md` in `subdir/` (should be counted)
/// - 1 `.md` in `.crucible/` (excluded)
/// - 1 `.md` in `.git/` (excluded)
/// - 1 `.txt` file (not markdown)
///
/// Expected markdown count: 4
fn create_consistency_kiln() -> Result<TempDir> {
    let temp = TempDir::new()?;
    let root = temp.path();

    // --- Files that SHOULD be discovered ---
    std::fs::write(root.join("note1.md"), "# Note 1\n\nFirst note.")?;
    std::fs::write(root.join("note2.md"), "# Note 2\n\nSecond note.")?;
    std::fs::write(
        root.join("note3.md"),
        "# Note 3\n\n[[note1]] link.\n\n#tag1 #tag2",
    )?;

    // Subdirectory markdown (should be discovered)
    std::fs::create_dir(root.join("subdir"))?;
    std::fs::write(
        root.join("subdir").join("nested.md"),
        "# Nested\n\nIn subdir.",
    )?;

    // --- Files that should NOT be discovered ---
    // Excluded directories
    std::fs::create_dir(root.join(".crucible"))?;
    std::fs::write(
        root.join(".crucible").join("internal.md"),
        "# Internal config",
    )?;

    std::fs::create_dir(root.join(".git"))?;
    std::fs::write(root.join(".git").join("config.md"), "# Git config")?;

    // Non-markdown file
    std::fs::write(root.join("readme.txt"), "Not a markdown file")?;

    Ok(temp)
}

/// Keystone consistency test:
/// Process discovers the same markdown files that stats counts.
#[tokio::test]
async fn process_and_stats_agree_on_markdown_file_count() -> Result<()> {
    let temp = create_consistency_kiln()?;
    let kiln_path = temp.path();

    // 3 root .md + 1 subdir .md = 4 expected
    const EXPECTED_MD: usize = 4;

    // === 1. Process via KilnManager (equivalent of `cru process`) ===
    let km = KilnManager::new();
    let (discovered, processed, skipped, errors) = km.open_and_process(kiln_path, false).await?;

    // The total files the pipeline saw = discovered count

    // Assert: discovered == actual .md file count (excludes .crucible/.git)
    assert_eq!(
        discovered, EXPECTED_MD,
        "Process discovered {} markdown files, expected {}",
        discovered, EXPECTED_MD
    );

    // Assert: processed + skipped + errors == discovered (accounting invariant)
    assert_eq!(
        processed + skipped + errors.len(),
        discovered,
        "Accounting invariant broken: processed({}) + skipped({}) + errors({}) != discovered({})",
        processed,
        skipped,
        errors.len(),
        discovered
    );

    // Assert: no errors during processing
    assert!(
        errors.is_empty(),
        "Expected zero processing errors, got: {:?}",
        errors
    );

    // === 2. Stats via FileSystemKilnStatsService (equivalent of `cru stats`) ===
    let stats_service = FileSystemKilnStatsService;
    let stats = stats_service.collect(kiln_path)?;

    // Assert: stats.markdown_files == discovered
    assert_eq!(
        stats.markdown_files as usize, EXPECTED_MD,
        "Stats reports {} markdown files, expected {}",
        stats.markdown_files, EXPECTED_MD
    );

    // === 3. Cross-command consistency: process == stats ===
    assert_eq!(
        discovered, stats.markdown_files as usize,
        "CONSISTENCY FAILURE: process discovered {} but stats reports {} markdown files",
        discovered, stats.markdown_files
    );

    // === 4. Stats also counts non-markdown files correctly ===
    // Total files = 4 markdown + 1 txt = 5 (in non-excluded dirs)
    assert_eq!(
        stats.total_files, 5,
        "Stats total_files should be 5 (4 md + 1 txt), got {}",
        stats.total_files
    );

    Ok(())
}

/// Second run should skip all files (change detection), but discovered count stays the same.
#[tokio::test]
async fn second_process_run_skips_unchanged_files() -> Result<()> {
    let temp = create_consistency_kiln()?;
    let kiln_path = temp.path();
    const EXPECTED_MD: usize = 4;

    let km = KilnManager::new();

    // First run: all files get processed
    let (discovered1, processed1, skipped1, errors1) =
        km.open_and_process(kiln_path, false).await?;
    assert_eq!(discovered1, EXPECTED_MD);
    assert_eq!(discovered1, EXPECTED_MD);
    assert!(errors1.is_empty());

    // Second run: all files should be skipped (no changes)
    let (discovered2, processed2, skipped2, errors2) =
        km.open_and_process(kiln_path, false).await?;

    // Discovered count must be identical across runs
    assert_eq!(
        discovered2, discovered1,
        "Second run discovered {} files, first run discovered {}",
        discovered2, discovered1
    );

    // Everything should be skipped on second run
    assert_eq!(
        skipped2, EXPECTED_MD,
        "Second run should skip all {} files, but only skipped {}",
        EXPECTED_MD, skipped2
    );
    assert_eq!(
        processed2, 0,
        "Second run should process 0 files, but processed {}",
        processed2
    );

    Ok(())
}

/// Force flag should reprocess all files, but discovered count stays the same.
#[tokio::test]
async fn force_reprocess_agrees_with_stats() -> Result<()> {
    let temp = create_consistency_kiln()?;
    let kiln_path = temp.path();

    let km = KilnManager::new();

    // First run
    km.open_and_process(kiln_path, false).await?;

    // Force reprocess
    let (discovered, processed, skipped, errors) = km.open_and_process(kiln_path, true).await?;

    // Discovered count must match stats even after force
    let stats = FileSystemKilnStatsService.collect(kiln_path)?;
    assert_eq!(
        discovered, stats.markdown_files as usize,
        "After force: process discovered {} but stats reports {}",
        discovered, stats.markdown_files
    );

    Ok(())
}
