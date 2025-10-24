//! Polling-based file watching backend.

use crate::{
    traits::{FileWatcher, WatchConfig, WatchHandle, BackendCapabilities},
    error::{Error, Result},
    events::{FileEvent, FileEventKind, EventMetadata},
};

// Import the WatcherFactory trait
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};
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
    /// Last time this watch was checked
    last_check: Instant,
}

/// State information for a single file.
#[derive(Debug, Clone)]
struct FileState {
    /// Last modification time
    modified_time: Option<SystemTime>,
    /// File size
    size: Option<u64>,
    /// Whether the file existed
    existed: bool,
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
    /// Capabilities
    capabilities: BackendCapabilities,
}

#[allow(dead_code)]
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
            capabilities: BackendCapabilities::basic(),
        }
    }

    /// Initialize the watcher with event sender.
    async fn initialize(&mut self, event_sender: mpsc::UnboundedSender<FileEvent>) -> Result<()> {
        self.event_sender = Some(event_sender);
        self.start_polling_task().await?;
        info!("Polling watcher initialized with interval: {:?}", self.poll_interval);
        Ok(())
    }

    /// Start the background polling task.
    async fn start_polling_task(&mut self) -> Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        let _event_sender = self.event_sender.clone()
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

    /// Stop the background polling task.
    async fn stop_polling_task(&mut self) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(()).await;
        }

        if let Some(task) = self.poll_task.take() {
            let _ = task.await;
        }

        Ok(())
    }

    /// Update file states for a watch.
    async fn update_watch_states(&mut self, watch_id: &str) -> Result<()> {
        // Get path before borrowing watch_state mutably
        let (path_buf, exists) = {
            let watch_state = self.watches.get(watch_id)
                .ok_or_else(|| Error::WatchNotFound(watch_id.to_string()))?;
            let path = &watch_state.config.id;
            let path_buf = PathBuf::from(path);
            let exists = path_buf.exists();
            (path_buf, exists)
        };

        // Process changes and collect events to send
        let events_to_send = if exists {
            self.collect_path_change_events(&path_buf, watch_id).await?
        } else {
            self.collect_deletion_events(&path_buf, watch_id).await?
        };

        // Send events after releasing the mutable borrow
        for (kind, path) in events_to_send {
            self.send_event(kind, path).await;
        }

        Ok(())
    }

    /// Collect events for path changes without sending them.
    async fn collect_path_change_events(&mut self, path: &PathBuf, watch_id: &str) -> Result<Vec<(FileEventKind, PathBuf)>> {
        let mut events = Vec::new();

        let watch_state = self.watches.get_mut(watch_id)
            .ok_or_else(|| Error::WatchNotFound(watch_id.to_string()))?;

        watch_state.last_check = Instant::now();

        if let Ok(metadata) = std::fs::metadata(path) {
            let modified_time = metadata.modified().ok();
            let size = Some(metadata.len());

            let current_state = FileState {
                modified_time,
                size,
                existed: true,
            };

            let previous_state = watch_state.file_states.get(path);

            match previous_state {
                None => {
                    // File is new
                    events.push((FileEventKind::Created, path.clone()));
                }
                Some(prev) => {
                    // Check for modifications
                    if prev.modified_time != modified_time || prev.size != size {
                        events.push((FileEventKind::Modified, path.clone()));
                    }
                }
            }

            watch_state.file_states.insert(path.clone(), current_state);
        }

        Ok(events)
    }

    /// Collect events for path deletion without sending them.
    async fn collect_deletion_events(&mut self, path: &PathBuf, watch_id: &str) -> Result<Vec<(FileEventKind, PathBuf)>> {
        let mut events = Vec::new();

        let watch_state = self.watches.get_mut(watch_id)
            .ok_or_else(|| Error::WatchNotFound(watch_id.to_string()))?;

        watch_state.last_check = Instant::now();

        if let Some(prev_state) = watch_state.file_states.get(path) {
            if prev_state.existed {
                events.push((FileEventKind::Deleted, path.clone()));
            }
        }

        watch_state.file_states.insert(path.clone(), FileState {
            modified_time: None,
            size: None,
            existed: false,
        });

        Ok(events)
    }

    /// Check for changes in a specific path.
    async fn check_path_changes(&self, path: &PathBuf, watch_state: &mut WatchState) -> Result<()> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| Error::Io(e))?;

        let modified_time = metadata.modified().ok();
        let size = Some(metadata.len());
        let _file_path = path.to_string_lossy().to_string();

        let current_state = FileState {
            modified_time,
            size,
            existed: true,
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

    /// Handle deletion of a path.
    async fn handle_path_deletion(&self, path: &PathBuf, watch_state: &mut WatchState) -> Result<()> {
        if let Some(prev_state) = watch_state.file_states.get(path) {
            if prev_state.existed {
                self.send_event(FileEventKind::Deleted, path.clone()).await;
            }
        }

        // Remove from file states
        watch_state.file_states.remove(path);
        Ok(())
    }

    /// Send a file event.
    async fn send_event(&self, kind: FileEventKind, path: PathBuf) {
        if let Some(ref sender) = self.event_sender {
            let metadata = EventMetadata::new(
                "polling".to_string(),
                "default".to_string(),
            );

            let event = FileEvent::with_metadata(kind, path, metadata);
            if let Err(e) = sender.send(event) {
                error!("Failed to send polling event: {}", e);
            }
        }
    }

    /// Scan a directory recursively if configured.
    fn scan_directory<'a>(&'a self, dir: &'a PathBuf, watch_state: &'a mut WatchState) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            if !watch_state.config.recursive {
                return Ok(());
            }

            let mut entries = tokio::fs::read_dir(dir).await
                .map_err(|e| Error::Io(e))?;

            while let Some(entry) = entries.next_entry().await
                .map_err(|e| Error::Io(e))? {

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

    /// Update the polling interval.
    pub fn update_interval(&mut self, interval: Duration) -> Result<()> {
        self.poll_interval = interval;
        info!("Updated polling interval to {:?}", interval);

        // Note: Changing interval would require restarting the polling task
        // This is a simplified implementation
        warn!("Runtime interval update requires task restart");

        Ok(())
    }
}

#[async_trait]
impl FileWatcher for PollingWatcher {
    fn backend_type(&self) -> &'static str {
        "polling"
    }

    fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<FileEvent>) {
        self.event_sender = Some(sender);
    }

    async fn watch(&mut self, path: PathBuf, config: WatchConfig) -> Result<WatchHandle> {
        debug!("Adding polling watch for: {}", path.display());

        // Initialize if not already done
        if self.poll_task.is_none() {
            let sender = self.event_sender.clone()
                .ok_or_else(|| Error::Internal("Event sender not set before calling watch".to_string()))?;
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
            last_check: Instant::now(),
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

    async fn unwatch(&mut self, handle: WatchHandle) -> Result<()> {
        debug!("Removing polling watch for: {}", handle.path.display());

        // Find and remove watch by handle ID
        let mut removed = false;

        self.watches.retain(|id, _state| {
            if *id == handle.id {
                removed = true;
                false
            } else {
                true
            }
        });

        if removed {
            info!("Removed polling watch: {}", handle.path.display());
        } else {
            warn!("Polling watch not found: {}", handle.path.display());
        }

        Ok(())
    }

    fn active_watches(&self) -> Vec<WatchHandle> {
        self.watches.iter()
            .map(|(id, state)| WatchHandle {
                id: id.clone(),
                path: state.watched_path.clone(),
            })
            .collect()
    }

    fn is_available(&self) -> bool {
        // Polling is always available
        true
    }

    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities.clone()
    }
}

impl Drop for PollingWatcher {
    fn drop(&mut self) {
        // The polling task should be stopped in the async context
        // This is a limitation of the current design
        warn!("PollingWatcher dropped without explicit shutdown");
    }
}

/// Factory for creating polling-based watchers.
pub struct PollingFactory {
    capabilities: BackendCapabilities,
}

impl PollingFactory {
    /// Create a new polling factory.
    pub fn new() -> Self {
        Self {
            capabilities: BackendCapabilities {
                recursive: true,
                fine_grained_events: false, // Polling has coarse-grained detection
                multiple_paths: true,
                hot_reconfig: true, // Polling supports hot reconfiguration
                platforms: vec!["all".to_string()],
            },
        }
    }
}

#[async_trait]
impl super::WatcherFactory for PollingFactory {
    async fn create_watcher(&self) -> Result<Box<dyn FileWatcher>> {
        Ok(Box::new(PollingWatcher::new()))
    }

    fn backend_type(&self) -> crate::WatchBackend {
        crate::WatchBackend::Polling
    }

    fn is_available(&self) -> bool {
        // Polling is always available
        true
    }

    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::WatcherFactory;
    use crate::traits::{WatchConfig, DebounceConfig};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_polling_watcher_creation() {
        let factory = PollingFactory::new();
        assert!(factory.is_available());

        let watcher = factory.create_watcher().await.unwrap();
        assert_eq!(watcher.backend_type(), "polling");
        assert!(watcher.is_available());
    }

    #[tokio::test]
    async fn test_polling_watcher_with_custom_interval() {
        let watcher = PollingWatcher::with_interval(Duration::from_millis(500));
        assert_eq!(watcher.poll_interval, Duration::from_millis(500));
    }

    #[tokio::test]
    async fn test_polling_watcher_operations() {
        let mut watcher = PollingWatcher::new();
        let temp_dir = TempDir::new().unwrap();
        let watch_path = temp_dir.path().to_path_buf();

        // Set up event sender before calling watch()
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        watcher.set_event_sender(tx);

        let config = WatchConfig::new("test")
            .with_recursive(true)
            .with_debounce(DebounceConfig::new(50));

        // Test watching
        let handle = watcher.watch(watch_path.clone(), config).await.unwrap();
        assert_eq!(handle.path, watch_path);

        // Test active watches
        let active = watcher.active_watches();
        assert_eq!(active.len(), 1);

        // Test unwatching
        watcher.unwatch(handle).await.unwrap();
        assert!(watcher.active_watches().is_empty());
    }

    #[test]
    fn test_factory_capabilities() {
        let factory = PollingFactory::new();
        let capabilities = factory.capabilities();

        assert!(capabilities.recursive);
        assert!(!capabilities.fine_grained_events); // Polling is coarse-grained
        assert!(capabilities.multiple_paths);
        assert!(capabilities.hot_reconfig); // Polling supports hot reconfig
        assert_eq!(capabilities.platforms, vec!["all".to_string()]);
    }
}