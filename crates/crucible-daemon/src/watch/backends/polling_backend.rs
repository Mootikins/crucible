//! Polling-based file watching backend.

use crate::watch::{
    error::{Error, Result},
    events::{EventMetadata, FileEvent, FileEventKind},
    traits::{WatchConfig, WatchHandle},
};

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// State information for a watched path.
#[derive(Debug, Clone)]
struct WatchState {
    /// Watch configuration
    config: WatchConfig,
    /// Path being watched
    watched_path: PathBuf,
    /// Last known modification times for files
    file_states: HashMap<PathBuf, FileState>,
}

/// State information for a single file.
#[derive(Debug, Clone)]
struct FileState {
    /// Last modification time
    modified_time: Option<SystemTime>,
    /// File size
    size: Option<u64>,
}

/// Polling-based file watcher for compatibility and low-frequency monitoring.
pub struct PollingWatcher {
    /// Event sender
    event_sender: Option<mpsc::UnboundedSender<FileEvent>>,
    /// Active watches
    watches: HashMap<String, WatchState>,
    /// Polling interval
    poll_interval: Duration,
    /// Background polling task
    poll_task: Option<JoinHandle<()>>,
    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl Default for PollingWatcher {
    fn default() -> Self {
        Self::with_interval(Duration::from_secs(1))
    }
}

impl PollingWatcher {
    /// Create a new polling watcher.
    pub fn new() -> Self {
        Self::with_interval(Duration::from_secs(1))
    }

    /// Create a polling watcher with custom interval.
    pub fn with_interval(interval: Duration) -> Self {
        Self {
            event_sender: None,
            watches: HashMap::new(),
            poll_interval: interval,
            poll_task: None,
            shutdown_tx: None,
        }
    }

    /// Initialize the watcher with event sender.
    async fn initialize(&mut self, event_sender: mpsc::UnboundedSender<FileEvent>) -> Result<()> {
        self.event_sender = Some(event_sender);
        self.start_polling_task().await?;
        info!(
            "Polling watcher initialized with interval: {:?}",
            self.poll_interval
        );
        Ok(())
    }

    /// Start the background polling task.
    async fn start_polling_task(&mut self) -> Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        let _event_sender = self
            .event_sender
            .clone()
            .ok_or_else(|| Error::Internal("Event sender not initialized".to_string()))?;

        let poll_interval = self.poll_interval;
        let _watches_snapshot: HashMap<String, WatchState> = HashMap::new();

        let task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(poll_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        // Update watches snapshot (this would need proper synchronization)
                        // For now, we'll use a simplified approach
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Polling task shutting down");
                        break;
                    }
                }
            }
        });

        self.poll_task = Some(task);
        self.shutdown_tx = Some(shutdown_tx);

        Ok(())
    }

    /// Check for changes in a specific path.
    async fn check_path_changes(&self, path: &PathBuf, watch_state: &mut WatchState) -> Result<()> {
        let metadata = std::fs::metadata(path).map_err(Error::Io)?;

        let modified_time = metadata.modified().ok();
        let size = Some(metadata.len());
        let _file_path = path.to_string_lossy().to_string();

        let current_state = FileState {
            modified_time,
            size,
        };

        let previous_state = watch_state.file_states.get(path);

        match previous_state {
            None => {
                // File is new
                self.send_event(FileEventKind::Created, path.clone()).await;
            }
            Some(prev) => {
                // Check for modifications
                if prev.modified_time != modified_time || prev.size != size {
                    self.send_event(FileEventKind::Modified, path.clone()).await;
                }
            }
        }

        watch_state.file_states.insert(path.clone(), current_state);
        Ok(())
    }

    /// Send a file event.
    async fn send_event(&self, kind: FileEventKind, path: PathBuf) {
        if let Some(ref sender) = self.event_sender {
            let metadata = EventMetadata::new("polling".to_string(), "default".to_string());

            let event = FileEvent::with_metadata(kind, path, metadata);
            if let Err(e) = sender.send(event) {
                error!("Failed to send polling event: {}", e);
            }
        }
    }

    /// Scan a directory recursively if configured.
    fn scan_directory<'a>(
        &'a self,
        dir: &'a PathBuf,
        watch_state: &'a mut WatchState,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            if !watch_state.config.recursive {
                return Ok(());
            }

            let mut entries = tokio::fs::read_dir(dir).await.map_err(Error::Io)?;

            while let Some(entry) = entries.next_entry().await.map_err(Error::Io)? {
                let path = entry.path();

                if path.is_dir() {
                    // Recursively scan subdirectory
                    self.scan_directory(&path, watch_state).await?;
                } else {
                    // Check file
                    self.check_path_changes(&path, watch_state).await?;
                }
            }

            Ok(())
        })
    }
}

impl PollingWatcher {
    /// Set the channel the watcher sends events on. Call this before `watch`.
    pub fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<FileEvent>) {
        self.event_sender = Some(sender);
    }

    /// Start to watch `path` with `config`.
    pub async fn watch(&mut self, path: PathBuf, config: WatchConfig) -> Result<WatchHandle> {
        debug!("Adding polling watch for: {}", path.display());

        // Initialize if not already done
        if self.poll_task.is_none() {
            let sender = self.event_sender.clone().ok_or_else(|| {
                Error::Internal("Event sender not set before calling watch".to_string())
            })?;
            self.initialize(sender).await?;
        }

        let watch_id = config.id.clone();
        let _path_str = path.to_string_lossy().to_string();
        let watch_handle = WatchHandle {
            id: watch_id.clone(),
            path: path.clone(),
        };

        // Create initial watch state
        let mut watch_state = WatchState {
            config: config.clone(),
            watched_path: path.clone(),
            file_states: HashMap::new(),
        };

        // Initial scan of the directory/file
        if path.exists() {
            if path.is_dir() {
                self.scan_directory(&path, &mut watch_state).await?;
            } else {
                self.check_path_changes(&path, &mut watch_state).await?;
            }
        }

        self.watches.insert(watch_id.clone(), watch_state);
        info!("Added polling watch: {} -> {}", watch_id, path.display());

        Ok(watch_handle)
    }

    /// Stop the watch behind `handle`.
    pub async fn unwatch(&mut self, handle: WatchHandle) -> Result<()> {
        super::remove_watch(&mut self.watches, &handle, "polling");
        Ok(())
    }

    /// Every watch the backend holds.
    pub fn active_watches(&self) -> Vec<WatchHandle> {
        super::watch_handles(&self.watches, |state| &state.watched_path)
    }
}

impl Drop for PollingWatcher {
    fn drop(&mut self) {
        // The polling task should be stopped in the async context
        // This is a limitation of the current design
        warn!("PollingWatcher dropped without explicit shutdown");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_interval_1_second() {
        let watcher = PollingWatcher::default();
        assert_eq!(watcher.poll_interval, Duration::from_secs(1));
    }

    #[test]
    fn custom_interval() {
        let watcher = PollingWatcher::with_interval(Duration::from_secs(5));
        assert_eq!(watcher.poll_interval, Duration::from_secs(5));
    }

    #[test]
    fn initial_watches_empty() {
        let watcher = PollingWatcher::new();
        assert!(watcher.active_watches().is_empty());
    }
}
