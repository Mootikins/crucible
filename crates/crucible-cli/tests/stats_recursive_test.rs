//! TDD tests for recursive directory scanning in stats command
//!
//! This test file validates that the stats command correctly counts files
//! in nested directories, not just the top-level directory.

use anyhow::Result;
use crucible_cli::commands::stats::{FileSystemKilnStatsService, KilnStatsService};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper to create a nested test kiln structure
fn create_nested_test_kiln() -> Result<TempDir> {
    let temp_dir = TempDir::new()?;
    let root = temp_dir.path();

    // Create nested directory structure:
    // root/
    //   file1.md
    //   file2.txt
    //   subdir1/
    //     file3.md
    //     file4.md
    //   subdir2/
    //     subdir2a/
    //       file5.md
    //       file6.json

    // Root level files
    fs::write(root.join("file1.md"), "# Root File 1")?;
    fs::write(root.join("file2.txt"), "Root text file")?;

    // Subdir1 with markdown files
    fs::create_dir(root.join("subdir1"))?;
    fs::write(root.join("subdir1/file3.md"), "# Subdir1 File 3")?;
    fs::write(root.join("subdir1/file4.md"), "# Subdir1 File 4")?;

    // Subdir2 with nested subdir2a
    fs::create_dir(root.join("subdir2"))?;
    fs::create_dir(root.join("subdir2/subdir2a"))?;
    fs::write(root.join("subdir2/subdir2a/file5.md"), "# Nested File 5")?;
    fs::write(root.join("subdir2/subdir2a/file6.json"), "{}")?;

    Ok(temp_dir)
}

/// Unit test: FileSystemKilnStatsService should count all files recursively
#[test]
fn test_stats_service_counts_nested_files() -> Result<()> {
    let temp_kiln = create_nested_test_kiln()?;
    let kiln_path = temp_kiln.path();

    let service = FileSystemKilnStatsService;
    let stats = service.collect(kiln_path)?;

    // Should count ALL files across all directories
    assert_eq!(
        stats.total_files, 6,
        "Should count all 6 files (root + subdirs), got {}",
        stats.total_files
    );

    // Should count all .md files
    assert_eq!(
        stats.markdown_files, 4,
        "Should count all 4 markdown files, got {}",
        stats.markdown_files
    );

    // Total size should include all files
    assert!(
        stats.total_size_bytes > 0,
        "Total size should include nested files"
    );

    Ok(())
}

/// Unit test: Stats should work with deeply nested directories
#[test]
fn test_stats_service_with_deep_nesting() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let root = temp_dir.path();

    // Create deep nesting: root/a/b/c/d/file.md
    fs::create_dir_all(root.join("a/b/c/d"))?;
    fs::write(root.join("a/b/c/d/deep_file.md"), "# Deep file")?;

    let service = FileSystemKilnStatsService;
    let stats = service.collect(root)?;

    assert_eq!(
        stats.total_files, 1,
        "Should find file in deeply nested directory"
    );
    assert_eq!(stats.markdown_files, 1, "Should count deeply nested .md file");

    Ok(())
}

/// Unit test: Empty directories should not break stats
#[test]
fn test_stats_service_with_empty_subdirs() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let root = temp_dir.path();

    // Create some empty directories
    fs::create_dir(root.join("empty1"))?;
    fs::create_dir(root.join("empty2"))?;
    fs::create_dir_all(root.join("nested/empty"))?;

    // And one file in root
    fs::write(root.join("file.md"), "content")?;

    let service = FileSystemKilnStatsService;
    let stats = service.collect(root)?;

    assert_eq!(
        stats.total_files, 1,
        "Should only count the one file, not directories"
    );
    assert_eq!(stats.markdown_files, 1);

    Ok(())
}

/// Integration test: CLI stats command with nested vault
#[tokio::test]
async fn test_cli_stats_command_recursive() -> Result<()> {
    let temp_kiln = create_nested_test_kiln()?;
    let kiln_path = temp_kiln.path();

    // Create config pointing to test kiln
    let config = crucible_cli::config::CliConfig::builder()
        .kiln_path(kiln_path)
        .build()?;

    // Execute stats command
    let service: std::sync::Arc<dyn KilnStatsService> =
        std::sync::Arc::new(FileSystemKilnStatsService);

    // This should use the service's collect method which should be recursive
    let stats = service.collect(kiln_path)?;

    assert_eq!(
        stats.total_files, 6,
        "CLI stats should count all nested files"
    );
    assert_eq!(
        stats.markdown_files, 4,
        "CLI stats should count all nested markdown files"
    );

    Ok(())
}

/// Regression test: Ensure stats works with real-world vault structure
#[test]
fn test_stats_with_realistic_vault_structure() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let root = temp_dir.path();

    // Simulate realistic Obsidian vault structure
    fs::create_dir(root.join("Projects"))?;
    fs::create_dir(root.join("Projects/ProjectA"))?;
    fs::create_dir(root.join("Projects/ProjectB"))?;
    fs::create_dir(root.join("Daily Notes"))?;
    fs::create_dir(root.join("Templates"))?;
    fs::create_dir(root.join(".obsidian"))?; // Should still be counted

    // Add files
    fs::write(root.join("README.md"), "# Vault README")?;
    fs::write(root.join("Projects/ProjectA/overview.md"), "# Project A")?;
    fs::write(root.join("Projects/ProjectA/notes.md"), "# Notes")?;
    fs::write(root.join("Projects/ProjectB/plan.md"), "# Plan")?;
    fs::write(root.join("Daily Notes/2024-01-01.md"), "# Daily")?;
    fs::write(root.join("Templates/template.md"), "# Template")?;
    fs::write(root.join(".obsidian/workspace"), "{}")?; // Not markdown

    let service = FileSystemKilnStatsService;
    let stats = service.collect(root)?;

    // Should find all files including in .obsidian
    assert_eq!(
        stats.total_files, 7,
        "Should count all files in realistic vault structure, got {}",
        stats.total_files
    );

    // Should count only .md files
    assert_eq!(
        stats.markdown_files, 6,
        "Should count only markdown files, got {}",
        stats.markdown_files
    );

    Ok(())
}
