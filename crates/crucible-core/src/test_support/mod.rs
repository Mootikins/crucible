//! Shared testing utilities for Crucible components.
//!
//! These helpers create temporary kilns with predictable content so tests
//! across crates can exercise document indexing and search behaviour without
//! duplicating setup logic.

use anyhow::{Context, Result};
use std::path::Path;
use tempfile::TempDir;

/// Create a temporary kiln populated with the provided files.
///
/// Each entry is `(relative_path, file_contents)`. Directories are created
/// automatically. The returned [`TempDir`] remains responsible for cleaning up
/// the kiln when dropped.
pub fn create_kiln_with_files(files: &[(&str, &str)]) -> Result<TempDir> {
    let temp_dir = TempDir::new().context("failed to create temporary kiln directory")?;
    let kiln_path = temp_dir.path();

    for (relative_path, contents) in files {
        let file_path = kiln_path.join(relative_path);

        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create kiln subdirectory {:?}", parent.display())
            })?;
        }

        std::fs::write(&file_path, contents)
            .with_context(|| format!("failed to write kiln file {:?}", file_path.display()))?;
    }

    Ok(temp_dir)
}

/// Create a kiln with a minimal set of markdown documents that cover the most
/// common CLI test scenarios.
pub fn create_basic_kiln() -> Result<TempDir> {
    create_kiln_with_files(&[
        (
            "Getting Started.md",
            "# Getting Started\n\nThis is a getting started guide for the kiln.",
        ),
        (
            "Project Architecture.md",
            "# Project Architecture\n\nThis document describes the architecture.",
        ),
        ("Testing Notes.md", "# Testing\n\nSome testing notes here."),
        ("README.md", "# README\n\nThis is the main README file."),
        (
            "Development.md",
            "# Development\n\nDevelopment documentation.",
        ),
    ])
}

/// Convenience helper to convert a kiln path into a string for configuration.
pub fn kiln_path_str(path: &Path) -> String {
    path.to_string_lossy().to_string()
}
