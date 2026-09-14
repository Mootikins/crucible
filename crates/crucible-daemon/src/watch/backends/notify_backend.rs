//! Notify-based file watching backend.

use crate::watch::{
    error::{Error, Result},
    events::{EventFilter, EventMetadata, FileEvent, FileEventKind},
    traits::{WatchConfig, WatchHandle},
};

use notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    new_debouncer, DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache,
};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace};

/// Notify-based file watcher with debouncing support.
pub struct NotifyWatcher {
    /// Debounced file system watcher
    debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
    /// Event sender
    event_sender: Option<mpsc::UnboundedSender<FileEvent>>,
    /// Watched paths, keyed by watch id.
    watches: std::collections::HashMap<String, PathBuf>,
    /// Event filter (shared with debouncer callback)
    filter: Arc<RwLock<Option<EventFilter>>>,
}

impl Default for NotifyWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl NotifyWatcher {
    /// Create a new notify-based watcher.
    pub fn new() -> Self {
        Self {
            debouncer: None,
            event_sender: None,
            watches: std::collections::HashMap::new(),
            filter: Arc::new(RwLock::new(None)),
        }
    }

    /// Build the debouncer that delays events by `delay`.
    ///
    /// `notify` debounces at its own level, so the delay is fixed when the
    /// first watch creates the debouncer. Later watches reuse it.
    async fn initialize(
        &mut self,
        event_sender: mpsc::UnboundedSender<FileEvent>,
        delay: Duration,
    ) -> Result<()> {
        let sender = event_sender.clone();
        let filter = self.filter.clone();

        let debouncer = new_debouncer(
            delay,
            None, // Use default tick rate
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
                if let (Some(from), Some(to)) =
                    (event.event.paths.first(), event.event.paths.get(1))
                {
                    FileEventKind::Moved {
                        from: from.clone(),
                        to: to.clone(),
                    }
                } else {
                    FileEventKind::Unknown("Other".to_string())
                }
            }
            EventKind::Any | EventKind::Access(_) => {
                FileEventKind::Unknown(format!("{:?}", event.event.kind))
            }
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
}

impl NotifyWatcher {
    /// Set the channel the watcher sends events on. Call this before `watch`.
    pub fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<FileEvent>) {
        self.event_sender = Some(sender);
    }

    /// Start to watch `path` with `config`.
    pub async fn watch(&mut self, path: PathBuf, config: WatchConfig) -> Result<WatchHandle> {
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
            let delay = Duration::from_millis(config.debounce.delay_ms);
            self.initialize(sender, delay).await?;
        }

        let watch_id = config.id.clone();
        let watch_handle = WatchHandle {
            id: watch_id.clone(),
            path: path.clone(),
        };

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

        self.watches.insert(watch_id.clone(), path.clone());
        info!("Added notify watch: {} -> {}", watch_id, path.display());

        Ok(watch_handle)
    }

    /// Stop the watch behind `handle`, keyed by its id like the other backends.
    #[cfg(test)]
    pub async fn unwatch(&mut self, handle: WatchHandle) -> Result<()> {
        if self.watches.contains_key(&handle.id) {
            if let Some(ref mut debouncer) = self.debouncer {
                debouncer
                    .unwatch(&handle.path)
                    .map_err(|e| Error::Watch(format!("Failed to unwatch path: {}", e)))?;
            }
        }
        self.watches.remove(&handle.id);
        Ok(())
    }

    /// Every watch the backend holds.
    #[cfg(test)]
    pub fn active_watches(&self) -> Vec<WatchHandle> {
        self.watches
            .iter()
            .map(|(id, path)| WatchHandle {
                id: id.clone(),
                path: path.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch::traits::DebounceConfig;
    use tempfile::TempDir;

    /// Build a watcher, or `None` when this box has no inotify instance left.
    async fn watch_dirs(
        dirs: &[(&str, &TempDir)],
        debounce: DebounceConfig,
    ) -> Option<(
        NotifyWatcher,
        Vec<WatchHandle>,
        mpsc::UnboundedReceiver<FileEvent>,
    )> {
        let mut watcher = NotifyWatcher::new();
        let (tx, rx) = mpsc::unbounded_channel();
        watcher.set_event_sender(tx);
        let mut handles = Vec::new();
        for (id, dir) in dirs {
            let config = WatchConfig::new(*id).with_debounce(debounce.clone());
            match watcher.watch(dir.path().to_path_buf(), config).await {
                Ok(handle) => handles.push(handle),
                Err(e) if e.to_string().contains("os error 24") => {
                    eprintln!("skip: inotify limit exhausted: {e}");
                    return None;
                }
                Err(e) => panic!("watch failed: {e}"),
            }
        }
        Some((watcher, handles, rx))
    }

    #[tokio::test]
    async fn unwatch_removes_the_handle_by_id() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        let Some((mut watcher, handles, _rx)) =
            watch_dirs(&[("a", &a), ("b", &b)], DebounceConfig::default()).await
        else {
            return;
        };
        assert_eq!(handles[0].id, "a");
        assert_eq!(handles[1].id, "b");

        watcher.unwatch(handles[0].clone()).await.unwrap();

        let left = watcher.active_watches();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, "b");
        assert_eq!(left[0].path, b.path());
    }

    #[tokio::test]
    async fn first_watch_config_sets_the_debounce_delay() {
        let dir = TempDir::new().unwrap();
        let Some((_watcher, _handles, mut rx)) =
            watch_dirs(&[("a", &dir)], DebounceConfig::new(600)).await
        else {
            return;
        };

        std::fs::write(dir.path().join("note.md"), "x").unwrap();

        // The default delay is 100 ms, so a 600 ms delay holds the event
        // past 250 ms.
        let early = tokio::time::timeout(Duration::from_millis(250), rx.recv()).await;
        assert!(early.is_err(), "event arrived before the debounce delay");
        let late = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await;
        assert!(
            late.is_ok(),
            "event did not arrive after the debounce delay"
        );
    }
}
