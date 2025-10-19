//! Editor integration backend for low-frequency inode watching.

use crate::{
    traits::{FileWatcher, WatchConfig, WatchHandle, BackendCapabilities},
    error::{Error, Result},
    events::{FileEvent, FileEventKind, EventMetadata},
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Configuration for editor integration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EditorConfig {
    /// Editor type (vscode, vim, emacs, etc.)
    pub editor_type: String,
    /// Editor-specific configuration
    pub editor_config: HashMap<String, serde_json::Value>,
    /// Low-frequency polling interval
    #[serde(with = "duration_serde")]
    pub poll_interval: Duration,
    /// Inode change detection
    pub detect_inode_changes: bool,
    /// Editor API integration
    pub use_editor_api: bool,
}

mod duration_serde {
    use std::time::Duration;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

/// State for tracking file changes via inode monitoring.
#[derive(Debug, Clone)]
struct InodeState {
    /// File inode number
    inode: Option<u64>,
    /// Last modification time
    modified_time: Option<SystemTime>,
    /// File size
    size: Option<u64>,
    /// Last known content hash (optional)
    content_hash: Option<String>,
}

/// State information for an editor watch.
#[derive(Debug, Clone)]
struct EditorWatchState {
    /// Watch configuration
    config: WatchConfig,
    /// Editor configuration
    editor_config: EditorConfig,
    /// File states tracked by inode
    file_states: HashMap<PathBuf, InodeState>,
    /// Last time this watch was checked
    last_check: Instant,
    /// Editor-specific state
    editor_state: HashMap<String, serde_json::Value>,
}

/// Editor integration backend for low-frequency file watching.
pub struct EditorWatcher {
    /// Event sender
    event_sender: Option<mpsc::UnboundedSender<FileEvent>>,
    /// Active watches
    watches: HashMap<String, EditorWatchState>,
    /// Background monitoring task
    monitor_task: Option<JoinHandle<()>>,
    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// Capabilities
    capabilities: BackendCapabilities,
}

impl EditorWatcher {
    /// Create a new editor watcher.
    pub fn new() -> Self {
        Self::with_default_config()
    }

    /// Create an editor watcher with default configuration.
    pub fn with_default_config() -> Self {
        Self {
            event_sender: None,
            watches: HashMap::new(),
            monitor_task: None,
            shutdown_tx: None,
            capabilities: BackendCapabilities {
                recursive: false, // Editor watching is typically non-recursive
                fine_grained_events: true, // Can detect specific editor events
                multiple_paths: true,
                hot_reconfig: true,
                platforms: vec!["linux".to_string(), "macos".to_string(), "windows".to_string()],
            },
        }
    }

    /// Initialize the watcher with event sender.
    async fn initialize(&mut self, event_sender: mpsc::UnboundedSender<FileEvent>) -> Result<()> {
        self.event_sender = Some(event_sender);
        self.start_monitoring_task().await?;
        info!("Editor watcher initialized");
        Ok(())
    }

    /// Start the background monitoring task.
    async fn start_monitoring_task(&mut self) -> Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        let event_sender = self.event_sender.clone()
            .ok_or_else(|| Error::Internal("Event sender not initialized".to_string()))?;

        let task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(5)); // 5-second default
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        // Editor-specific monitoring logic would go here
                        // For now, this is a placeholder
                        debug!("Editor watcher tick");
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Editor monitor task shutting down");
                        break;
                    }
                }
            }
        });

        self.monitor_task = Some(task);
        self.shutdown_tx = Some(shutdown_tx);

        Ok(())
    }

    /// Stop the background monitoring task.
    async fn stop_monitoring_task(&mut self) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(()).await;
        }

        if let Some(task) = self.monitor_task.take() {
            let _ = task.await;
        }

        Ok(())
    }

    /// Get file inode information.
    fn get_file_inode(&self, path: &PathBuf) -> Result<Option<u64>> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = std::fs::metadata(path)
                .map_err(|e| Error::Io(e))?;
            Ok(Some(metadata.ino()))
        }

        #[cfg(not(unix))]
        {
            // On Windows, we could use file ID or other alternatives
            // For now, return None to indicate inode tracking not available
            Ok(None)
        }
    }

    /// Update inode-based file state.
    async fn update_inode_state(&self, path: &PathBuf, state: &mut InodeState) -> Result<bool> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| Error::Io(e))?;

        let modified_time = metadata.modified().ok();
        let size = Some(metadata.len());
        let inode = self.get_file_inode(path)?;

        let mut changed = false;

        // Check for changes
        if state.inode != inode {
            changed = true;
            state.inode = inode;
        }

        if state.modified_time != modified_time {
            changed = true;
            state.modified_time = modified_time;
        }

        if state.size != size {
            changed = true;
            state.size = size;
        }

        Ok(changed)
    }

    /// Monitor a specific watch for changes.
    async fn monitor_watch(&mut self, watch_id: &str) -> Result<()> {
        // Clone what we need to avoid borrow conflicts
        let (detect_inode_changes, editor_type) = {
            let watch_state = self.watches.get(watch_id)
                .ok_or_else(|| Error::WatchNotFound(watch_id.to_string()))?;
            (
                watch_state.editor_config.detect_inode_changes,
                watch_state.editor_config.editor_type.clone()
            )
        };

        // Update last check time
        {
            let watch_state = self.watches.get_mut(watch_id)
                .ok_or_else(|| Error::WatchNotFound(watch_id.to_string()))?;
            watch_state.last_check = Instant::now();
        }

        // If inode detection is enabled, check for changes
        if detect_inode_changes {
            self.check_inode_changes_for_watch(watch_id).await?;
        }

        // Editor-specific monitoring
        self.check_editor_changes_for_watch(watch_id).await?;

        Ok(())
    }

    /// Check for inode-based changes for a specific watch.
    async fn check_inode_changes_for_watch(&mut self, watch_id: &str) -> Result<()> {
        // Extract what we need to avoid multiple mutable borrows
        let (watch_path, detect_inode) = {
            let watch_state = self.watches.get(watch_id)
                .ok_or_else(|| Error::WatchNotFound(watch_id.to_string()))?;
            (PathBuf::from(&watch_state.config.id), watch_state.editor_config.detect_inode_changes)
        };

        if !detect_inode {
            return Ok(());
        }

        // Collect events to send
        let mut events_to_send = Vec::new();

        if !watch_path.exists() {
            let watch_state = self.watches.get_mut(watch_id)
                .ok_or_else(|| Error::WatchNotFound(watch_id.to_string()))?;
            if let Some(prev_state) = watch_state.file_states.get(&watch_path) {
                if prev_state.inode.is_some() {
                    events_to_send.push((FileEventKind::Deleted, watch_path.clone()));
                    watch_state.file_states.remove(&watch_path);
                }
            }
        } else {
            let watch_state = self.watches.get_mut(watch_id)
                .ok_or_else(|| Error::WatchNotFound(watch_id.to_string()))?;

            // Update or create inode state
            let mut file_state = watch_state.file_states
                .entry(watch_path.clone())
                .or_insert_with(|| InodeState {
                    inode: None,
                    modified_time: None,
                    size: None,
                    content_hash: None,
                });

            let previous_inode = file_state.inode;
            let changed = Self::update_inode_state_static(&watch_path, &mut file_state).await?;

            if changed {
                if previous_inode.is_none() {
                    events_to_send.push((FileEventKind::Created, watch_path.clone()));
                } else {
                    events_to_send.push((FileEventKind::Modified, watch_path.clone()));
                }
            }
        }

        // Send events after releasing the borrow
        for (kind, path) in events_to_send {
            self.send_event(kind, path).await;
        }

        Ok(())
    }

    /// Static helper to update inode state without self borrow.
    async fn update_inode_state_static(watch_path: &PathBuf, file_state: &mut InodeState) -> Result<bool> {
        let metadata = std::fs::metadata(watch_path)
            .map_err(|e| Error::Io(e))?;

        let new_modified = metadata.modified().ok();
        let new_size = Some(metadata.len());
        let new_inode = Self::get_inode_from_metadata(&metadata);

        let changed = file_state.inode != new_inode
            || file_state.modified_time != new_modified
            || file_state.size != new_size;

        file_state.inode = new_inode;
        file_state.modified_time = new_modified;
        file_state.size = new_size;

        Ok(changed)
    }

    /// Get inode number from metadata (platform-specific).
    fn get_inode_from_metadata(metadata: &std::fs::Metadata) -> Option<u64> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Some(metadata.ino())
        }
        #[cfg(not(unix))]
        {
            // On non-Unix systems, use file index if available
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                metadata.file_index()
            }
            #[cfg(not(windows))]
            {
                None
            }
        }
    }

    /// Check for inode-based changes.
    async fn check_inode_changes(&mut self, watch_state: &mut EditorWatchState) -> Result<()> {
        let watch_path = PathBuf::from(&watch_state.config.id);

        if !watch_path.exists() {
            // Check if file was deleted
            if let Some(prev_state) = watch_state.file_states.get(&watch_path) {
                if prev_state.inode.is_some() {
                    self.send_event(FileEventKind::Deleted, watch_path.clone()).await;
                    watch_state.file_states.remove(&watch_path);
                }
            }
            return Ok(());
        }

        // Update or create inode state
        let mut file_state = watch_state.file_states
            .entry(watch_path.clone())
            .or_insert_with(|| InodeState {
                inode: None,
                modified_time: None,
                size: None,
                content_hash: None,
            });

        let previous_inode = file_state.inode;
        let changed = self.update_inode_state(&watch_path, &mut file_state).await?;

        if changed {
            if previous_inode.is_none() {
                // File was created
                self.send_event(FileEventKind::Created, watch_path.clone()).await;
            } else {
                // File was modified
                self.send_event(FileEventKind::Modified, watch_path.clone()).await;
            }
        }

        Ok(())
    }

    /// Check for editor-specific changes for a specific watch.
    async fn check_editor_changes_for_watch(&mut self, watch_id: &str) -> Result<()> {
        // For now, just return Ok - editor-specific checks need more complex refactoring
        // TODO: Implement proper editor-specific change detection without borrow conflicts
        Ok(())
    }

    /// Check for editor-specific changes.
    async fn check_editor_changes(&mut self, watch_state: &mut EditorWatchState) -> Result<()> {
        match watch_state.editor_config.editor_type.as_str() {
            "vscode" => self.check_vscode_changes(watch_state).await,
            "vim" => self.check_vim_changes(watch_state).await,
            "emacs" => self.check_emacs_changes(watch_state).await,
            _ => {
                debug!("No specific monitoring for editor type: {}", watch_state.editor_config.editor_type);
                Ok(())
            }
        }
    }

    /// Check VSCode-specific changes.
    async fn check_vscode_changes(&self, _watch_state: &mut EditorWatchState) -> Result<()> {
        // TODO: Implement VSCode-specific monitoring
        // This could involve checking workspace state, extensions, etc.
        debug!("VSCode-specific monitoring not yet implemented");
        Ok(())
    }

    /// Check Vim-specific changes.
    async fn check_vim_changes(&self, _watch_state: &mut EditorWatchState) -> Result<()> {
        // TODO: Implement Vim-specific monitoring
        // This could involve checking swap files, viminfo, etc.
        debug!("Vim-specific monitoring not yet implemented");
        Ok(())
    }

    /// Check Emacs-specific changes.
    async fn check_emacs_changes(&self, _watch_state: &mut EditorWatchState) -> Result<()> {
        // TODO: Implement Emacs-specific monitoring
        // This could involve checking auto-save files, etc.
        debug!("Emacs-specific monitoring not yet implemented");
        Ok(())
    }

    /// Send a file event.
    async fn send_event(&self, kind: FileEventKind, path: PathBuf) {
        if let Some(ref sender) = self.event_sender {
            let metadata = EventMetadata::new(
                "editor".to_string(),
                "default".to_string(),
            );

            let event = FileEvent::with_metadata(kind, path, metadata);
            if let Err(e) = sender.send(event) {
                error!("Failed to send editor event: {}", e);
            }
        }
    }

    /// Update editor configuration.
    pub fn update_editor_config(&mut self, watch_id: &str, config: EditorConfig) -> Result<()> {
        if let Some(watch_state) = self.watches.get_mut(watch_id) {
            watch_state.editor_config = config;
            info!("Updated editor configuration for watch: {}", watch_id);
            Ok(())
        } else {
            Err(Error::WatchNotFound(watch_id.to_string()))
        }
    }
}

#[async_trait]
impl FileWatcher for EditorWatcher {
    fn backend_type(&self) -> &'static str {
        "editor"
    }

    fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<FileEvent>) {
        self.event_sender = Some(sender);
    }

    async fn watch(&mut self, path: PathBuf, config: WatchConfig) -> Result<WatchHandle> {
        debug!("Adding editor watch for: {}", path.display());

        // Initialize if not already done
        if self.monitor_task.is_none() {
            let sender = self.event_sender.clone()
                .ok_or_else(|| Error::Internal("Event sender not set before calling watch".to_string()))?;
            self.initialize(sender).await?;
        }

        let watch_id = config.id.clone();
        let watch_handle = WatchHandle::new(path.clone());

        // Extract editor configuration from backend options
        let editor_config = config.backend_options.get("editor_config")
            .and_then(|v| serde_json::from_value::<EditorConfig>(v.clone()).ok())
            .unwrap_or_else(|| EditorConfig {
                editor_type: "generic".to_string(),
                editor_config: HashMap::new(),
                poll_interval: Duration::from_secs(5),
                detect_inode_changes: true,
                use_editor_api: false,
            });

        // Create editor watch state
        let watch_state = EditorWatchState {
            config: config.clone(),
            editor_config,
            file_states: HashMap::new(),
            last_check: Instant::now(),
            editor_state: HashMap::new(),
        };

        self.watches.insert(watch_id.clone(), watch_state);
        info!("Added editor watch: {} -> {}", watch_id, path.display());

        Ok(watch_handle)
    }

    async fn unwatch(&mut self, handle: WatchHandle) -> Result<()> {
        debug!("Removing editor watch for: {}", handle.path.display());

        // Find and remove watch by path
        let path_str = handle.path.to_string_lossy().to_string();
        let mut removed = false;

        self.watches.retain(|id, state| {
            if state.config.id == path_str {
                removed = true;
                false
            } else {
                true
            }
        });

        if removed {
            info!("Removed editor watch: {}", handle.path.display());
        } else {
            warn!("Editor watch not found: {}", handle.path.display());
        }

        Ok(())
    }

    fn active_watches(&self) -> Vec<WatchHandle> {
        self.watches.values()
            .map(|state| WatchHandle::new(PathBuf::from(&state.config.id)))
            .collect()
    }

    fn is_available(&self) -> bool {
        // Editor backend is available on most platforms
        // but might have different capabilities per platform
        true
    }

    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities.clone()
    }
}

/// Factory for creating editor-based watchers.
pub struct EditorFactory {
    capabilities: BackendCapabilities,
}

impl EditorFactory {
    /// Create a new editor factory.
    pub fn new() -> Self {
        Self {
            capabilities: BackendCapabilities {
                recursive: false, // Editor watching is typically non-recursive
                fine_grained_events: true,
                multiple_paths: true,
                hot_reconfig: true,
                platforms: vec!["linux".to_string(), "macos".to_string(), "windows".to_string()],
            },
        }
    }
}

#[async_trait]
impl super::WatcherFactory for EditorFactory {
    async fn create_watcher(&self) -> Result<Box<dyn FileWatcher>> {
        Ok(Box::new(EditorWatcher::new()))
    }

    fn backend_type(&self) -> crate::WatchBackend {
        crate::WatchBackend::Editor
    }

    fn is_available(&self) -> bool {
        // Editor backend is available on all platforms
        true
    }

    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{WatchConfig, DebounceConfig};
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_editor_watcher_creation() {
        let factory = EditorFactory::new();
        assert!(factory.is_available());

        let watcher = factory.create_watcher().await.unwrap();
        assert_eq!(watcher.backend_type(), "editor");
        assert!(watcher.is_available());
    }

    #[tokio::test]
    async fn test_editor_watcher_operations() {
        let mut watcher = EditorWatcher::new();
        let temp_dir = TempDir::new().unwrap();
        let watch_path = temp_dir.path().to_path_buf();

        let config = WatchConfig::new("test")
            .with_recursive(false) // Editor watching is typically non-recursive
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
    fn test_editor_config() {
        let config = EditorConfig {
            editor_type: "vscode".to_string(),
            editor_config: HashMap::new(),
            poll_interval: Duration::from_secs(3),
            detect_inode_changes: true,
            use_editor_api: false,
        };

        assert_eq!(config.editor_type, "vscode");
        assert!(config.detect_inode_changes);
        assert_eq!(config.poll_interval, Duration::from_secs(3));
    }

    #[test]
    fn test_factory_capabilities() {
        let factory = EditorFactory::new();
        let capabilities = factory.capabilities();

        assert!(!capabilities.recursive); // Editor watching is non-recursive
        assert!(capabilities.fine_grained_events);
        assert!(capabilities.multiple_paths);
        assert!(capabilities.hot_reconfig);
        assert!(!capabilities.platforms.is_empty());
    }
}