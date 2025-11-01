//! Kiln Scanner - Phase 1A TDD Implementation
//!
//! This module provides functionality to scan kiln directories and discover markdown files.
//! Implemented to make the failing tests pass with minimal functionality.

use crate::kiln_types::{KilnError, KilnResult};
use std::path::PathBuf;
use walkdir::{DirEntry, WalkDir};

/// Scanner for discovering markdown files in kiln directories
#[derive(Debug, Clone)]
pub struct KilnScanner {
    /// Root path of the kiln
    kiln_path: String,
}

impl KilnScanner {
    /// Create a new kiln scanner
    #[must_use] 
    pub fn new(kiln_path: &str) -> Self {
        Self {
            kiln_path: kiln_path.to_string(),
        }
    }

    /// Scan for markdown files recursively
    pub async fn scan_markdown_files(&self) -> KilnResult<Vec<PathBuf>> {
        let kiln_path = self.kiln_path.clone();

        tokio::task::spawn_blocking(move || Self::_scan_markdown_files_recursive(&kiln_path))
            .await
            .map_err(|e| KilnError::HashError(format!("Task join error: {e}")))?
    }

    /// Scan for markdown files non-recursively (root only)
    pub async fn scan_markdown_files_non_recursive(&self) -> KilnResult<Vec<PathBuf>> {
        let kiln_path = self.kiln_path.clone();

        tokio::task::spawn_blocking(move || Self::_scan_markdown_files_non_recursive(&kiln_path))
            .await
            .map_err(|e| KilnError::HashError(format!("Task join error: {e}")))?
    }

    /// Internal recursive implementation (runs in blocking thread)
    fn _scan_markdown_files_recursive(kiln_path: &str) -> KilnResult<Vec<PathBuf>> {
        let mut markdown_files = Vec::new();

        let walk_dir = WalkDir::new(kiln_path).follow_links(false).max_depth(10); // Reasonable depth limit

        for entry in walk_dir.into_iter().filter_map(std::result::Result::ok) {
            if Self::is_markdown_file(&entry) {
                if let Ok(path) = entry.path().strip_prefix(kiln_path) {
                    markdown_files.push(path.to_path_buf());
                }
            }
        }

        // Sort for consistent results
        markdown_files.sort();
        Ok(markdown_files)
    }

    /// Internal non-recursive implementation (runs in blocking thread)
    fn _scan_markdown_files_non_recursive(kiln_path: &str) -> KilnResult<Vec<PathBuf>> {
        let mut markdown_files = Vec::new();

        let walk_dir = WalkDir::new(kiln_path).follow_links(false).max_depth(1); // Root directory only

        for entry in walk_dir.into_iter().filter_map(std::result::Result::ok) {
            if Self::is_markdown_file(&entry) {
                if let Ok(path) = entry.path().strip_prefix(kiln_path) {
                    markdown_files.push(path.to_path_buf());
                }
            }
        }

        // Sort for consistent results
        markdown_files.sort();
        Ok(markdown_files)
    }

    /// Check if a directory entry is a markdown file
    fn is_markdown_file(entry: &DirEntry) -> bool {
        if !entry.file_type().is_file() {
            return false;
        }

        entry
            .path()
            .extension()
            .is_some_and(|ext| ext.to_string_lossy().to_lowercase() == "md")
    }

    /// Get the kiln root path
    #[must_use] 
    pub fn kiln_path(&self) -> &str {
        &self.kiln_path
    }

    /// Check if the kiln path exists
    pub async fn kiln_exists(&self) -> bool {
        let kiln_path = self.kiln_path.clone();

        tokio::task::spawn_blocking(move || std::path::Path::new(&kiln_path).exists())
            .await
            .unwrap_or(false)
    }

    /// Get absolute file paths from relative paths
    #[must_use] 
    pub fn get_absolute_path(&self, relative_path: &PathBuf) -> PathBuf {
        std::path::Path::new(&self.kiln_path).join(relative_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_scanner_creates_successfully() {
        let scanner = KilnScanner::new("/test/path");
        assert_eq!(scanner.kiln_path(), "/test/path");
    }

    #[tokio::test]
    async fn test_markdown_file_detection() {
        // Create a temporary directory with test files
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create test files
        fs::write(temp_dir.path().join("test.md"), "# Test\nContent").unwrap();
        fs::write(temp_dir.path().join("readme.txt"), "Not markdown").unwrap();
        fs::write(temp_dir.path().join("notes.MD"), "# Upper case\nContent").unwrap();

        let scanner = KilnScanner::new(&kiln_path);
        let files = scanner.scan_markdown_files().await.unwrap();

        assert_eq!(files.len(), 2);
        assert!(files
            .iter()
            .any(|p| p.to_string_lossy().contains("test.md")));
        assert!(files
            .iter()
            .any(|p| p.to_string_lossy().contains("notes.MD")));
    }

    #[tokio::test]
    async fn test_recursive_vs_non_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create directory structure
        fs::create_dir_all(temp_dir.path().join("subdir")).unwrap();
        fs::write(temp_dir.path().join("root.md"), "# Root\nContent").unwrap();
        fs::write(
            temp_dir.path().join("subdir/nested.md"),
            "# Nested\nContent",
        )
        .unwrap();

        let scanner = KilnScanner::new(&kiln_path);

        // Non-recursive should only find root file
        let root_files = scanner.scan_markdown_files_non_recursive().await.unwrap();
        assert_eq!(root_files.len(), 1);
        assert!(root_files[0].to_string_lossy().contains("root.md"));

        // Recursive should find both files
        let all_files = scanner.scan_markdown_files().await.unwrap();
        assert_eq!(all_files.len(), 2);
    }
}
