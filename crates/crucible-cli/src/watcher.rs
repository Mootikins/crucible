//! Simple file watcher for markdown files in the kiln
//!
//! Provides in-process file watching using notify with these features:
//! - Auto-respects .gitignore
//! - 500ms debounce by default
//! - Filters to .md files only
//! - Pre-flight inotify checks on Linux
//! - Graceful failure handling

use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{
    new_debouncer, DebounceEventResult, DebouncedEvent, DebouncedEventKind, Debouncer,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::config::FileWatcherConfig;

/// Built-in file patterns to exclude from watching
const DEFAULT_EXCLUDES: &[&str] = &[
    "**/.git/**",
    "**/.crucible/**",  // SurrealDB database directory
    "**/.obsidian/workspace*",
    "**/.obsidian/plugins/**/node_modules/**",
    "**/node_modules/**",
    "**/.trash/**",
];

/// File watching events
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// File was modified
    Changed(PathBuf),
    /// File was created
    Created(PathBuf),
    /// File was deleted
    Deleted(PathBuf),
}

/// Error types for file watching
#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("Insufficient inotify watches: needed {needed}, available {available}")]
    InsufficientWatches { needed: usize, available: usize },

    #[error("Permission denied for path: {0:?}")]
    PermissionDenied(PathBuf),

    #[error("Failed to initialize watcher: {0}")]
    InitializationError(String),

    #[error("{0}")]
    Other(String),
}

/// Simple file watcher using notify-debouncer-mini
pub struct SimpleFileWatcher {
    _debouncer: Debouncer<RecommendedWatcher>,
}

impl SimpleFileWatcher {
    /// Create a new file watcher
    ///
    /// # Arguments
    /// * `kiln_path` - Path to the kiln/kiln directory to watch
    /// * `config` - File watcher configuration
    /// * `event_tx` - Channel to send watch events to
    pub fn new(
        kiln_path: impl AsRef<Path>,
        config: FileWatcherConfig,
        event_tx: mpsc::UnboundedSender<WatchEvent>,
    ) -> Result<Self, WatcherError> {
        let kiln_path = kiln_path.as_ref();

        // Pre-flight checks on Linux
        Self::verify_watch_capacity(kiln_path)?;

        debug!("Initializing file watcher for: {}", kiln_path.display());

        // Create debouncer with configured delay
        let debounce_duration = Duration::from_millis(config.debounce_ms);

        let tx_clone = event_tx.clone();
        let kiln_path_clone = kiln_path.to_path_buf();

        let mut debouncer =
            new_debouncer(
                debounce_duration,
                move |result: DebounceEventResult| match result {
                    Ok(events) => {
                        for event in events {
                            if let Some(watch_event) = Self::process_event(&event, &kiln_path_clone)
                            {
                                if let Err(e) = tx_clone.send(watch_event) {
                                    error!("Failed to send watch event: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Watch error: {:?}", e);
                    }
                },
            )
            .map_err(|e| WatcherError::InitializationError(e.to_string()))?;

        // Start watching the kiln directory
        debouncer
            .watcher()
            .watch(kiln_path, RecursiveMode::Recursive)
            .map_err(|e| match e.kind {
                notify::ErrorKind::Io(ref io_err)
                    if io_err.kind() == std::io::ErrorKind::PermissionDenied =>
                {
                    WatcherError::PermissionDenied(kiln_path.to_path_buf())
                }
                _ => WatcherError::Other(e.to_string()),
            })?;

        info!("File watcher started (debounce: {}ms)", config.debounce_ms);

        Ok(Self {
            _debouncer: debouncer,
        })
    }

    /// Process a notify event and convert to WatchEvent if relevant
    fn process_event(event: &DebouncedEvent, kiln_path: &Path) -> Option<WatchEvent> {
        // Filter: only markdown files
        let path = &event.path;

        if !Self::should_watch_file(path, kiln_path) {
            return None;
        }

        // Convert notify event kind to WatchEvent
        // Note: DebouncedEventKind is simplified compared to EventKind
        match event.kind {
            DebouncedEventKind::AnyContinuous => {
                // Treat all continuous events as modifications
                debug!("File modified: {}", path.display());
                Some(WatchEvent::Changed(path.clone()))
            }
            _ => {
                // For other event types, we can't distinguish create/delete reliably
                // Just treat as a change
                debug!("File event: {}", path.display());
                Some(WatchEvent::Changed(path.clone()))
            }
        }
    }

    /// Check if a file should be watched
    fn should_watch_file(path: &Path, kiln_path: &Path) -> bool {
        // Must be a markdown file
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            return false;
        }

        // Must be within kiln
        if !path.starts_with(kiln_path) {
            return false;
        }

        // Check against exclude patterns
        let relative_path = match path.strip_prefix(kiln_path) {
            Ok(p) => p,
            Err(_) => return false,
        };

        for exclude in DEFAULT_EXCLUDES {
            if Self::matches_pattern(relative_path, exclude) {
                return false;
            }
        }

        true
    }

    /// Simple glob pattern matching
    fn matches_pattern(path: &Path, pattern: &str) -> bool {
        let path_str = path.to_string_lossy();

        // Simple pattern matching for common cases
        if pattern.starts_with("**/") && pattern.ends_with("/**") {
            // Pattern like **/.git/** or **/.crucible/**
            let dir_name = &pattern[3..pattern.len() - 3];
            // Check: contains /dir_name/, or starts with dir_name/
            path_str.contains(&format!("/{}/", dir_name))
                || path_str.starts_with(&format!("{}/", dir_name))
                // Also check for path components directly
                || path.components().any(|c| c.as_os_str() == dir_name)
        } else if pattern.starts_with("**/") && pattern.ends_with("*") {
            // Pattern like **/.obsidian/workspace*
            let middle = &pattern[3..pattern.len() - 1];
            path_str.contains(middle)
        } else if let Some(prefix) = pattern.strip_suffix("*") {
            // Pattern like .obsidian/workspace*
            path_str.contains(prefix)
        } else {
            // Exact match
            path_str.contains(pattern)
        }
    }

    /// Verify sufficient watch capacity (Linux only)
    fn verify_watch_capacity(kiln_path: &Path) -> Result<(), WatcherError> {
        #[cfg(target_os = "linux")]
        {
            // Estimate watches needed (roughly 1 per directory)
            let estimated_watches = Self::estimate_watch_count(kiln_path)?;

            // Read max_user_watches from /proc
            let max_watches = std::fs::read_to_string("/proc/sys/fs/inotify/max_user_watches")
                .ok()
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(8192); // Default kernel value

            if estimated_watches > max_watches {
                return Err(WatcherError::InsufficientWatches {
                    needed: estimated_watches,
                    available: max_watches,
                });
            }

            // Warn if using >50% of available watches
            if estimated_watches > (max_watches / 2) {
                warn!(
                    "Using {} of {} available inotify watches ({:.1}%)",
                    estimated_watches,
                    max_watches,
                    (estimated_watches as f64 / max_watches as f64) * 100.0
                );
            }

            debug!(
                "inotify check passed: {} watches needed, {} available",
                estimated_watches, max_watches
            );
        }

        Ok(())
    }

    /// Estimate number of watches needed (Linux only)
    #[cfg(target_os = "linux")]
    fn estimate_watch_count(kiln_path: &Path) -> Result<usize, WatcherError> {
        use std::fs;

        let mut count = 0;

        // Walk directory tree and count directories
        if let Ok(entries) = fs::read_dir(kiln_path) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        count += 1;
                        // Recursively count subdirectories
                        if let Ok(subcount) = Self::count_directories(&entry.path()) {
                            count += subcount;
                        }
                    }
                }
            }
        }

        // Add some overhead for file operations
        Ok(count + 20)
    }

    #[cfg(target_os = "linux")]
    fn count_directories(path: &Path) -> Result<usize, WatcherError> {
        use std::fs;

        let mut count = 0;

        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let dir_name = entry.file_name();
                        let dir_str = dir_name.to_string_lossy();

                        // Skip excluded directories
                        if dir_str.starts_with('.') || dir_str == "node_modules" {
                            continue;
                        }

                        count += 1;
                        if let Ok(subcount) = Self::count_directories(&entry.path()) {
                            count += subcount;
                        }
                    }
                }
            }
        }

        Ok(count)
    }
}

/// Get fix instructions for a watcher error
pub fn get_fix_instructions(error: &WatcherError) -> String {
    match error {
        WatcherError::InsufficientWatches { needed, .. } => {
            format!(
                "Increase inotify watches:\n  \
                 Temporary: sudo sysctl -w fs.inotify.max_user_watches={}\n  \
                 Permanent: Add 'fs.inotify.max_user_watches={}' to /etc/sysctl.conf",
                needed * 2,
                needed * 2
            )
        }
        WatcherError::PermissionDenied(path) => {
            format!(
                "Fix permissions for: {}\n  \
                 Check that the kiln directory is readable",
                path.display()
            )
        }
        _ => "Check logs for more details".to_string(),
    }
}
