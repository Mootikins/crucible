//! Quick Sync Module
//!
//! Provides fast staleness detection for CLI chat startup.
//! Uses filesystem mtime comparison instead of hash computation for speed.

use anyhow::Result;
#[cfg(feature = "storage-surrealdb")]
use crucible_surrealdb::adapters::SurrealClientHandle;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{debug, trace, warn};
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

/// Stored file metadata from the database
#[derive(Debug)]
struct StoredFileMeta {
    modified_time: SystemTime,
    #[allow(dead_code)]
    file_size: u64,
}

#[cfg(feature = "storage-surrealdb")]
/// Perform a quick sync check comparing filesystem mtimes against database
///
/// This is much faster than computing file hashes because it only reads
/// filesystem metadata, not file contents.
///
/// # Arguments
/// * `storage` - SurrealDB client handle
/// * `kiln_path` - Root path of the kiln (vault)
///
/// # Returns
/// `SyncStatus` with counts of fresh, stale, new, and deleted files
pub async fn quick_sync_check(
    storage: &SurrealClientHandle,
    kiln_path: &Path,
) -> Result<SyncStatus> {
    let start = std::time::Instant::now();

    // Step 1: Fetch all stored file states from database in one query
    let stored_states = fetch_all_file_states(storage).await?;
    debug!(
        "Fetched {} file states from database in {:?}",
        stored_states.len(),
        start.elapsed()
    );

    // Step 2: Walk filesystem and compare mtimes
    let fs_start = std::time::Instant::now();
    let mut fresh_count = 0;
    let mut stale_files = Vec::new();
    let mut new_files = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for entry in WalkDir::new(kiln_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_excluded_dir(e.path()))
        .filter_map(|e| e.ok())
    {
        let entry_path = entry.path();
        if !entry_path.is_file() || !is_markdown_file(entry_path) {
            continue;
        }

        // Get relative path for comparison with stored states
        let relative_path = match entry_path.strip_prefix(kiln_path) {
            Ok(p) => p.to_path_buf(),
            Err(_) => entry_path.to_path_buf(),
        };
        let path_str = relative_path.to_string_lossy().to_string();
        seen_paths.insert(path_str.clone());

        // Get filesystem mtime
        let fs_mtime = match entry.metadata() {
            Ok(meta) => meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            Err(e) => {
                warn!("Failed to get metadata for {}: {}", entry_path.display(), e);
                continue;
            }
        };

        // Compare with stored state
        match stored_states.get(&path_str) {
            Some(stored) => {
                // Allow 1 second tolerance for filesystem timestamp precision differences
                let is_stale = fs_mtime
                    .duration_since(stored.modified_time)
                    .map(|d| d.as_secs() > 1)
                    .unwrap_or(true);

                if is_stale {
                    trace!("Stale: {} (fs newer than stored)", path_str);
                    stale_files.push(entry_path.to_path_buf());
                } else {
                    fresh_count += 1;
                }
            }
            None => {
                trace!("New: {}", path_str);
                new_files.push(entry_path.to_path_buf());
            }
        }
    }

    debug!(
        "Filesystem walk completed in {:?}, found {} markdown files",
        fs_start.elapsed(),
        fresh_count + stale_files.len() + new_files.len()
    );

    // Step 3: Find deleted files (in DB but not on filesystem)
    let deleted_files: Vec<PathBuf> = stored_states
        .keys()
        .filter(|p| !seen_paths.contains(*p))
        .map(PathBuf::from)
        .collect();

    let check_duration = start.elapsed();
    debug!(
        "Quick sync completed in {:?}: {} fresh, {} stale, {} new, {} deleted",
        check_duration,
        fresh_count,
        stale_files.len(),
        new_files.len(),
        deleted_files.len()
    );

    Ok(SyncStatus {
        fresh_count,
        stale_files,
        new_files,
        deleted_files,
        check_duration,
    })
}

#[cfg(feature = "storage-surrealdb")]
/// Fetch all file states from database in a single query
async fn fetch_all_file_states(
    storage: &SurrealClientHandle,
) -> Result<HashMap<String, StoredFileMeta>> {
    let query = "SELECT relative_path, modified_time, file_size FROM file_state";

    let result = storage
        .inner()
        .query(query, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch file states: {}", e))?;

    let mut states = HashMap::new();

    for record in result.records {
        let data = serde_json::to_value(&record.data)?;
        if let Some(obj) = data.as_object() {
            let relative_path = obj
                .get("relative_path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            // Parse datetime string to SystemTime
            let modified_time = obj
                .get("modified_time")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| {
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(dt.timestamp() as u64)
                })
                .unwrap_or(SystemTime::UNIX_EPOCH);

            let file_size = obj.get("file_size").and_then(|v| v.as_i64()).unwrap_or(0) as u64;

            if !relative_path.is_empty() {
                states.insert(
                    relative_path,
                    StoredFileMeta {
                        modified_time,
                        file_size,
                    },
                );
            }
        }
    }

    Ok(states)
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
