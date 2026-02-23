//! Quick Sync Module
//!
//! Provides fast staleness detection for CLI chat startup.
//! Uses filesystem mtime comparison instead of hash computation for speed.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

/// Result of a quick sync check
#[derive(Debug, Clone)]
pub struct SyncStatus {
    /// Number of files that are up-to-date
    pub fresh_count: usize,
    /// Files that have been modified since last index
    pub stale_files: Vec<PathBuf>,
    /// New files not yet in the database
    pub new_files: Vec<PathBuf>,
    /// Files in DB but missing from filesystem
    pub deleted_files: Vec<PathBuf>,
    /// Time taken for the sync check
    pub check_duration: Duration,
}

impl SyncStatus {
    /// Check if any files need processing
    pub fn needs_processing(&self) -> bool {
        !self.stale_files.is_empty() || !self.new_files.is_empty()
    }

    /// Total number of files that need processing
    pub fn pending_count(&self) -> usize {
        self.stale_files.len() + self.new_files.len()
    }

    /// Get all files that need processing (stale + new)
    pub fn files_to_process(&self) -> Vec<PathBuf> {
        let mut files = self.stale_files.clone();
        files.extend(self.new_files.iter().cloned());
        files
    }

    /// Create a SyncStatus treating all markdown files in a directory as new.
    ///
    /// Used when quick_sync_check is not available (e.g., non-embedded backends).
    /// This ensures all files get processed on first run.
    pub fn all_new(kiln_path: &Path) -> Result<Self> {
        let start = std::time::Instant::now();
        let mut new_files = Vec::new();

        for entry in WalkDir::new(kiln_path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_excluded_dir(e.path()))
            .filter_map(|e| e.ok())
        {
            let entry_path = entry.path();
            if entry_path.is_file() && is_markdown_file(entry_path) {
                new_files.push(entry_path.to_path_buf());
            }
        }

        Ok(Self {
            fresh_count: 0,
            stale_files: vec![],
            new_files,
            deleted_files: vec![],
            check_duration: start.elapsed(),
        })
    }
}

/// Check if a directory should be excluded from file discovery
fn is_excluded_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            name == ".crucible"
                || name == ".git"
                || name == ".obsidian"
                || name == "node_modules"
                || name == ".trash"
        })
        .unwrap_or(false)
}

/// Check if a path is a markdown file
fn is_markdown_file(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_needs_processing() {
        let status = SyncStatus {
            fresh_count: 10,
            stale_files: vec![],
            new_files: vec![],
            deleted_files: vec![],
            check_duration: Duration::from_millis(50),
        };
        assert!(!status.needs_processing());

        let status_with_stale = SyncStatus {
            fresh_count: 10,
            stale_files: vec![PathBuf::from("test.md")],
            new_files: vec![],
            deleted_files: vec![],
            check_duration: Duration::from_millis(50),
        };
        assert!(status_with_stale.needs_processing());
    }

    #[test]
    fn test_sync_status_pending_count() {
        let status = SyncStatus {
            fresh_count: 10,
            stale_files: vec![PathBuf::from("a.md"), PathBuf::from("b.md")],
            new_files: vec![PathBuf::from("c.md")],
            deleted_files: vec![],
            check_duration: Duration::from_millis(50),
        };
        assert_eq!(status.pending_count(), 3);
    }

    #[test]
    fn test_is_excluded_dir() {
        assert!(is_excluded_dir(Path::new("/path/to/.git")));
        assert!(is_excluded_dir(Path::new("/path/to/.obsidian")));
        assert!(is_excluded_dir(Path::new("/path/to/.crucible")));
        assert!(is_excluded_dir(Path::new("/path/to/node_modules")));
        assert!(!is_excluded_dir(Path::new("/path/to/docs")));
    }

    #[test]
    fn test_is_markdown_file() {
        assert!(is_markdown_file(Path::new("test.md")));
        assert!(is_markdown_file(Path::new("/path/to/note.md")));
        assert!(!is_markdown_file(Path::new("test.txt")));
        assert!(!is_markdown_file(Path::new("test")));
    }

    #[test]
    fn test_is_markdown_file_edge_cases() {
        assert!(is_markdown_file(Path::new(".hidden.md")));
        assert!(!is_markdown_file(Path::new("file.MD")));
        assert!(!is_markdown_file(Path::new("file.markdown")));
        assert!(!is_markdown_file(Path::new("file.mdx")));
        assert!(!is_markdown_file(Path::new("")));
    }

    #[test]
    fn test_is_excluded_dir_edge_cases() {
        assert!(is_excluded_dir(Path::new(".trash")));
        assert!(!is_excluded_dir(Path::new("trash")));
        assert!(!is_excluded_dir(Path::new(".config")));
        assert!(!is_excluded_dir(Path::new("git")));
        assert!(!is_excluded_dir(Path::new("crucible")));
    }

    #[test]
    fn test_sync_status_files_to_process() {
        let status = SyncStatus {
            fresh_count: 5,
            stale_files: vec![PathBuf::from("a.md")],
            new_files: vec![PathBuf::from("b.md"), PathBuf::from("c.md")],
            deleted_files: vec![],
            check_duration: Duration::from_millis(10),
        };
        let files = status.files_to_process();
        assert_eq!(files.len(), 3);
        assert!(files.contains(&PathBuf::from("a.md")));
        assert!(files.contains(&PathBuf::from("b.md")));
        assert!(files.contains(&PathBuf::from("c.md")));
    }

    #[test]
    fn test_sync_status_empty() {
        let status = SyncStatus {
            fresh_count: 0,
            stale_files: vec![],
            new_files: vec![],
            deleted_files: vec![],
            check_duration: Duration::from_millis(1),
        };
        assert!(!status.needs_processing());
        assert_eq!(status.pending_count(), 0);
        assert!(status.files_to_process().is_empty());
    }

    #[test]
    fn test_sync_status_with_new_files_only() {
        let status = SyncStatus {
            fresh_count: 0,
            stale_files: vec![],
            new_files: vec![PathBuf::from("new.md")],
            deleted_files: vec![],
            check_duration: Duration::from_millis(5),
        };
        assert!(status.needs_processing());
        assert_eq!(status.pending_count(), 1);
    }

    #[test]
    fn test_sync_status_deleted_files_not_in_pending() {
        let status = SyncStatus {
            fresh_count: 5,
            stale_files: vec![],
            new_files: vec![],
            deleted_files: vec![PathBuf::from("deleted.md")],
            check_duration: Duration::from_millis(5),
        };
        assert!(!status.needs_processing());
        assert_eq!(status.pending_count(), 0);
    }
}
