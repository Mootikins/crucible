//! Notify-based file watching backend.

use crate::{
    error::{Error, Result},
    events::{EventFilter, EventMetadata, FileEvent, FileEventKind},
    traits::{BackendCapabilities, FileWatcher, WatchConfig, WatchHandle},
};

// Import the WatcherFactory trait
use async_trait::async_trait;
use notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, DebouncedEvent, Debouncer, FileIdMap};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};

/// Notify-based file watcher with debouncing support.
pub struct NotifyWatcher {
    /// Debounced file system watcher
    debouncer: Option<Debouncer<RecommendedWatcher, FileIdMap>>,
    /// Event sender
    event_sender: Option<mpsc::UnboundedSender<FileEvent>>,
    /// Active watches
    watches: std::collections::HashMap<String, WatchHandle>,
    /// Capabilities
    capabilities: BackendCapabilities,
    /// Event filter (shared with debouncer callback)
    filter: Arc<RwLock<Option<EventFilter>>>,
}

impl NotifyWatcher {
    /// Create a new notify-based watcher.
    pub fn new() -> Self {
        Self {
            debouncer: None,
            event_sender: None,
            watches: std::collections::HashMap::new(),
            capabilities: BackendCapabilities::full_support(),
            filter: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize the watcher with event sender.
    async fn initialize(&mut self, event_sender: mpsc::UnboundedSender<FileEvent>) -> Result<()> {
        let sender = event_sender.clone();
        let filter = self.filter.clone();

        // Create debounced watcher
        let debouncer = new_debouncer(
            Duration::from_millis(100), // Default debounce time
            None,                       // No file ID map for now
            move |result: DebounceEventResult| match result {
                Ok(events) => {
                    // Get the filter once per batch (read lock)
                    let filter_guard = filter.read().ok();
                    let filter_ref = filter_guard.as_ref().and_then(|g| g.as_ref());

                    for event in events {
                        match Self::convert_notify_event(event) {
                            Ok(file_event) => {
                                // Apply filter if configured
                                if let Some(f) = filter_ref {
                                    if !f.matches(&file_event) {
                                        trace!("Event filtered out: {}", file_event.path.display());
                                        continue;
                                    }
                                }

                                if let Err(e) = sender.send(file_event) {
                                    error!("Failed to send file event: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("Failed to convert notify event: {}", e);
                            }
                        }
                    }
                }
                Err(errors) => {
                    for error in errors {
                        error!("Notify error: {:?}", error);
                    }
                }
            },
        )
        .map_err(|e| Error::Watch(format!("Failed to create notify watcher: {}", e)))?;

        // NoCache doesn't have add_root, so we skip it

        self.debouncer = Some(debouncer);
        self.event_sender = Some(event_sender);

        info!("Notify watcher initialized");
        Ok(())
    }

    /// Convert notify event to our file event format.
    fn convert_notify_event(event: DebouncedEvent) -> Result<FileEvent> {
        let kind = match event.event.kind {
            EventKind::Create(_) => FileEventKind::Created,
            EventKind::Modify(_) => FileEventKind::Modified,
            EventKind::Remove(_) => FileEventKind::Deleted,
            EventKind::Other => {
                // Check if this is a move event
                if let (Some(from), Some(to)) = (event.event.paths.get(0), event.event.paths.get(1))
                {
                    FileEventKind::Moved {
                        from: from.clone(),
                        to: to.clone(),
                    }
                } else {
                    FileEventKind::Unknown("Other".to_string())
                }
            }
            _ => FileEventKind::Unknown(format!("{:?}", event.event.kind)),
        };

        // For batch events, create a single event for each path
        if event.event.paths.len() > 1 && !matches!(event.event.kind, EventKind::Other) {
            // Create a batch event
            let mut batch_events = Vec::new();
            for path in &event.event.paths {
                let metadata = EventMetadata::new("notify".to_string(), "default".to_string());
                batch_events.push(FileEvent::with_metadata(
                    kind.clone(),
                    path.clone(),
                    metadata,
                ));
            }
            return Ok(FileEvent::new(
                FileEventKind::Batch(batch_events),
                PathBuf::new(),
            ));
        }

        let path = event
            .event
            .paths
            .into_iter()
            .next()
            .ok_or_else(|| Error::Watch("Event has no path".to_string()))?;

        let metadata = EventMetadata::new("notify".to_string(), "default".to_string());

        Ok(FileEvent::with_metadata(kind, path, metadata))
    }

    /// Update debounce configuration.
    pub fn update_debounce_config(
        &mut self,
        _debounce_config: &crate::traits::DebounceConfig,
    ) -> Result<()> {
        // Note: notify-debouncer-full doesn't support runtime reconfiguration
        // This would require recreating the debouncer
        warn!("Runtime debounce reconfiguration not supported by notify backend");
        Ok(())
    }
}

#[async_trait]
impl FileWatcher for NotifyWatcher {
    fn backend_type(&self) -> &'static str {
        "notify"
    }

    fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<FileEvent>) {
        self.event_sender = Some(sender);
    }

    async fn watch(&mut self, path: PathBuf, config: WatchConfig) -> Result<WatchHandle> {
        debug!("Adding watch for: {}", path.display());

        // Store filter from config (if provided and not already set)
        if let Some(filter) = config.filter.clone() {
            let mut filter_guard = self.filter.write().map_err(|e| {
                Error::Internal(format!("Failed to acquire filter write lock: {}", e))
            })?;
            if filter_guard.is_none() {
                debug!("Setting event filter for notify watcher");
                *filter_guard = Some(filter);
            } else {
                debug!("Filter already set, ignoring new filter from config");
            }
        }

        // Initialize if not already done
        if self.debouncer.is_none() {
            let sender = self.event_sender.clone().ok_or_else(|| {
                Error::Internal("Event sender not set before calling watch".to_string())
            })?;
            self.initialize(sender).await?;
        }

        let watch_id = config.id.clone();
        let watch_handle = WatchHandle::new(path.clone());

        // Add path to notify watcher
        if let Some(ref mut debouncer) = self.debouncer {
            let mode = if config.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };

            debouncer
                .watch(&path, mode)
                .map_err(|e| Error::Watch(format!("Failed to watch path: {}", e)))?;
        }

        self.watches.insert(watch_id.clone(), watch_handle.clone());
        info!("Added notify watch: {} -> {}", watch_id, path.display());

        Ok(watch_handle)
    }

    async fn unwatch(&mut self, handle: WatchHandle) -> Result<()> {
        debug!("Removing watch for: {}", handle.path.display());

        // Find and remove watch by path
        for (_id, watch_handle) in &self.watches {
            if watch_handle.path == handle.path {
                // This is the watch to remove
                if let Some(ref mut debouncer) = self.debouncer {
                    debouncer
                        .unwatch(&watch_handle.path)
                        .map_err(|e| Error::Watch(format!("Failed to unwatch path: {}", e)))?;
                }
                break;
            }
        }

        // Remove from our tracking
        self.watches.retain(|_, h| h.path != handle.path);
        info!("Removed notify watch: {}", handle.path.display());

        Ok(())
    }

    fn active_watches(&self) -> Vec<WatchHandle> {
        self.watches.values().cloned().collect()
    }

    fn is_available(&self) -> bool {
        // Notify is available on most platforms
        true
    }

    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities.clone()
    }
}

/// Factory for creating notify-based watchers.
pub struct NotifyFactory {
    capabilities: BackendCapabilities,
}

impl NotifyFactory {
    /// Create a new notify factory.
    pub fn new() -> Self {
        Self {
            capabilities: BackendCapabilities {
                recursive: true,
                fine_grained_events: true,
                multiple_paths: true,
                hot_reconfig: false, // Notify doesn't support hot reconfiguration
                platforms: vec![
                    "linux".to_string(),
                    "macos".to_string(),
                    "windows".to_string(),
                ],
            },
        }
    }
}

#[async_trait]
impl super::WatcherFactory for NotifyFactory {
    async fn create_watcher(&self) -> Result<Box<dyn FileWatcher>> {
        Ok(Box::new(NotifyWatcher::new()))
    }

    fn backend_type(&self) -> crate::WatchBackend {
        crate::WatchBackend::Notify
    }

    fn is_available(&self) -> bool {
        // Check if notify is available
        // This is a simple check - in reality, you might want to test
        // if the underlying file system notifications work
        true
    }

    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities.clone()
    }
}
