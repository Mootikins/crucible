//! Vault Scanner - Phase 1A TDD Implementation
//!
//! This module provides functionality to scan vault directories and discover markdown files.
//! Implemented to make the failing tests pass with minimal functionality.

use crate::vault_types::{VaultError, VaultResult};
use std::path::PathBuf;
use walkdir::{WalkDir, DirEntry};

/// Scanner for discovering markdown files in vault directories
#[derive(Debug, Clone)]
pub struct VaultScanner {
    /// Root path of the vault
    vault_path: String,
}

impl VaultScanner {
    /// Create a new vault scanner
    pub fn new(vault_path: &str) -> Self {
        Self {
            vault_path: vault_path.to_string(),
        }
    }

    /// Scan for markdown files recursively
    pub async fn scan_markdown_files(&self) -> VaultResult<Vec<PathBuf>> {
        let vault_path = self.vault_path.clone();

        tokio::task::spawn_blocking(move || {
            Self::_scan_markdown_files_recursive(&vault_path)
        }).await.map_err(|e| VaultError::HashError(format!("Task join error: {}", e)))?
    }

    /// Scan for markdown files non-recursively (root only)
    pub async fn scan_markdown_files_non_recursive(&self) -> VaultResult<Vec<PathBuf>> {
        let vault_path = self.vault_path.clone();

        tokio::task::spawn_blocking(move || {
            Self::_scan_markdown_files_non_recursive(&vault_path)
        }).await.map_err(|e| VaultError::HashError(format!("Task join error: {}", e)))?
    }

    /// Internal recursive implementation (runs in blocking thread)
    fn _scan_markdown_files_recursive(vault_path: &str) -> VaultResult<Vec<PathBuf>> {
        let mut markdown_files = Vec::new();

        let walk_dir = WalkDir::new(vault_path)
            .follow_links(false)
            .max_depth(10); // Reasonable depth limit

        for entry in walk_dir.into_iter().filter_map(|e| e.ok()) {
            if Self::is_markdown_file(&entry) {
                if let Ok(path) = entry.path().strip_prefix(vault_path) {
                    markdown_files.push(path.to_path_buf());
                }
            }
        }

        // Sort for consistent results
        markdown_files.sort();
        Ok(markdown_files)
    }

    /// Internal non-recursive implementation (runs in blocking thread)
    fn _scan_markdown_files_non_recursive(vault_path: &str) -> VaultResult<Vec<PathBuf>> {
        let mut markdown_files = Vec::new();

        let walk_dir = WalkDir::new(vault_path)
            .follow_links(false)
            .max_depth(1); // Root directory only

        for entry in walk_dir.into_iter().filter_map(|e| e.ok()) {
            if Self::is_markdown_file(&entry) {
                if let Ok(path) = entry.path().strip_prefix(vault_path) {
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

        entry.path()
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase() == "md")
            .unwrap_or(false)
    }

    /// Get the vault root path
    pub fn vault_path(&self) -> &str {
        &self.vault_path
    }

    /// Check if the vault path exists
    pub async fn vault_exists(&self) -> bool {
        let vault_path = self.vault_path.clone();

        tokio::task::spawn_blocking(move || {
            std::path::Path::new(&vault_path).exists()
        }).await.unwrap_or(false)
    }

    /// Get absolute file paths from relative paths
    pub fn get_absolute_path(&self, relative_path: &PathBuf) -> PathBuf {
        std::path::Path::new(&self.vault_path).join(relative_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[tokio::test]
    async fn test_scanner_creates_successfully() {
        let scanner = VaultScanner::new("/test/path");
        assert_eq!(scanner.vault_path(), "/test/path");
    }

    #[tokio::test]
    async fn test_markdown_file_detection() {
        // Create a temporary directory with test files
        let temp_dir = TempDir::new().unwrap();
        let vault_path = temp_dir.path().to_string_lossy().to_string();

        // Create test files
        fs::write(temp_dir.path().join("test.md"), "# Test\nContent").unwrap();
        fs::write(temp_dir.path().join("readme.txt"), "Not markdown").unwrap();
        fs::write(temp_dir.path().join("notes.MD"), "# Upper case\nContent").unwrap();

        let scanner = VaultScanner::new(&vault_path);
        let files = scanner.scan_markdown_files().await.unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|p| p.to_string_lossy().contains("test.md")));
        assert!(files.iter().any(|p| p.to_string_lossy().contains("notes.MD")));
    }

    #[tokio::test]
    async fn test_recursive_vs_non_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let vault_path = temp_dir.path().to_string_lossy().to_string();

        // Create directory structure
        fs::create_dir_all(temp_dir.path().join("subdir")).unwrap();
        fs::write(temp_dir.path().join("root.md"), "# Root\nContent").unwrap();
        fs::write(temp_dir.path().join("subdir/nested.md"), "# Nested\nContent").unwrap();

        let scanner = VaultScanner::new(&vault_path);

        // Non-recursive should only find root file
        let root_files = scanner.scan_markdown_files_non_recursive().await.unwrap();
        assert_eq!(root_files.len(), 1);
        assert!(root_files[0].to_string_lossy().contains("root.md"));

        // Recursive should find both files
        let all_files = scanner.scan_markdown_files().await.unwrap();
        assert_eq!(all_files.len(), 2);
    }
}