//! Editor integration backend for low-frequency inode watching.



use crate::{
    error::{Error, Result},
    events::FileEvent,
    traits::{BackendCapabilities, FileWatcher, WatchConfig, WatchHandle},
};

// Import the WatcherFactory trait
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

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
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

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

/// State information for an editor watch.
#[derive(Debug, Clone)]
struct EditorWatchState {
    /// Path being watched
    watched_path: PathBuf,
    /// Editor configuration
    editor_config: EditorConfig,
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
                recursive: false,          // Editor watching is typically non-recursive
                fine_grained_events: true, // Can detect specific editor events
                multiple_paths: true,
                hot_reconfig: true,
                platforms: vec![
                    "linux".to_string(),
                    "macos".to_string(),
                    "windows".to_string(),
                ],
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
        let _event_sender = self
            .event_sender
            .clone()
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
            let sender = self.event_sender.clone().ok_or_else(|| {
                Error::Internal("Event sender not set before calling watch".to_string())
            })?;
            self.initialize(sender).await?;
        }

        let watch_id = config.id.clone();
        let watch_handle = WatchHandle {
            id: watch_id.clone(),
            path: path.clone(),
        };

        // Extract editor configuration from backend options
        let editor_config = config
            .backend_options
            .get("editor_config")
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
            watched_path: path.clone(),
            editor_config,
        };

        self.watches.insert(watch_id.clone(), watch_state);
        info!("Added editor watch: {} -> {}", watch_id, path.display());

        Ok(watch_handle)
    }

    async fn unwatch(&mut self, handle: WatchHandle) -> Result<()> {
        debug!("Removing editor watch for: {}", handle.path.display());

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
            info!("Removed editor watch: {}", handle.path.display());
        } else {
            warn!("Editor watch not found: {}", handle.path.display());
        }

        Ok(())
    }

    fn active_watches(&self) -> Vec<WatchHandle> {
        self.watches
            .iter()
            .map(|(id, state)| WatchHandle {
                id: id.clone(),
                path: state.watched_path.clone(),
            })
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

impl Default for EditorFactory {
    fn default() -> Self {
        Self::new()
    }
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
