//! Common test utilities for pipeline tests.

use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a temporary test file with markdown content.
///
/// Returns the temp directory (which must be kept alive) and the file path.
pub fn create_test_file(content: &str) -> Result<(TempDir, PathBuf)> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test_note.md");
    std::fs::write(&file_path, content)?;
    Ok((temp_dir, file_path))
}
